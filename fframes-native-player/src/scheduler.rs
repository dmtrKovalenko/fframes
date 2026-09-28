//! Generates `usvgr::Tree`s ahead of the playhead on worker threads.
//!
//! Playback is expressed in *positions*: a monotonic frame counter that starts at the
//! seek target. Without looping a position is the frame index; with looping the frame is
//! `position % duration`, so frames prefetched across the loop boundary stay ordered.

use std::collections::BTreeMap;
use std::sync::{Condvar, Mutex, MutexGuard};

use fframes::{FFramesContext, TextCache, Video, VideoDecodersWorker, usvgr};

pub(crate) struct Scheduler {
    state: Mutex<State>,
    workers_wakeup: Condvar,
    duration: u64,
    lookahead: u64,
}

struct State {
    /// Bumped on every seek so that frames rendered for a previous playhead are dropped.
    epoch: u64,
    /// Next position a worker should render.
    next: u64,
    /// Position the player wants to show now.
    playhead: u64,
    /// Last position handed to the player, anything at or before it is stale.
    displayed: Option<u64>,
    looping: bool,
    ready: BTreeMap<u64, usvgr::Tree>,
    shutdown: bool,
}

struct Job {
    position: u64,
    epoch: u64,
    frame: usize,
}

impl Scheduler {
    pub(crate) fn new(duration_in_frames: usize, lookahead: usize, looping: bool) -> Self {
        Self {
            state: Mutex::new(State {
                epoch: 0,
                next: 0,
                playhead: 0,
                displayed: None,
                looping,
                ready: BTreeMap::new(),
                shutdown: false,
            }),
            workers_wakeup: Condvar::new(),
            duration: duration_in_frames.max(1) as u64,
            lookahead: lookahead.max(1) as u64,
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // A panicking worker must not take the player down with a poisoned lock.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn frame_of(&self, position: u64, looping: bool) -> usize {
        if looping {
            (position % self.duration) as usize
        } else {
            position.min(self.duration - 1) as usize
        }
    }

    /// Drops everything that was scheduled and restarts generation at `position`.
    pub(crate) fn seek(&self, position: u64, looping: bool) {
        let mut state = self.lock();
        state.epoch += 1;
        state.next = position;
        state.playhead = position;
        state.displayed = None;
        state.looping = looping;
        state.ready.clear();
        drop(state);
        self.workers_wakeup.notify_all();
    }

    /// Moves the playhead and returns the newest ready frame at or before it.
    /// Frames that are late are still returned so heavy videos degrade to a lower
    /// frame rate instead of freezing.
    pub(crate) fn take(&self, playhead: u64) -> Option<(u64, usvgr::Tree)> {
        let mut state = self.lock();
        state.playhead = playhead;

        let newer = state.ready.split_off(&(playhead + 1));
        let due = std::mem::replace(&mut state.ready, newer);
        let taken = due.into_iter().next_back();
        if let Some((position, _)) = &taken {
            state.displayed = Some(*position);
        }
        drop(state);

        self.workers_wakeup.notify_all();
        taken
    }

    pub(crate) fn shutdown(&self) {
        self.lock().shutdown = true;
        self.workers_wakeup.notify_all();
    }

    fn next_job(&self) -> Option<Job> {
        let mut state = self.lock();
        loop {
            if state.shutdown {
                return None;
            }

            // Skip frames the playhead already passed: this is where frames get dropped
            // when rendering can not keep up with real time.
            if state.next < state.playhead {
                state.next = state.playhead;
            }

            let within_lookahead = state.next < state.playhead + self.lookahead;
            let within_video = state.looping || state.next < self.duration;
            if within_lookahead && within_video {
                let position = state.next;
                state.next += 1;

                return Some(Job {
                    position,
                    epoch: state.epoch,
                    frame: self.frame_of(position, state.looping),
                });
            }

            state = self
                .workers_wakeup
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
    }

    fn complete(&self, job: &Job, tree: usvgr::Tree) -> bool {
        let mut state = self.lock();
        let is_stale = state.epoch != job.epoch
            || state
                .displayed
                .is_some_and(|displayed| job.position <= displayed);

        if !is_stale {
            state.ready.insert(job.position, tree);
        }

        !is_stale
    }

    /// Runs on a worker thread until `shutdown` is called. `on_frame_ready` is invoked
    /// after every frame that was added to the queue (used to wake the event loop).
    pub(crate) fn run_worker<'a, 'media: 'a, TVideo: Video + Sync>(
        &self,
        video: &'a TVideo,
        ctx: &FFramesContext<'a, 'media>,
        usvg_options: &usvgr::Options<'_>,
        font_db: &usvgr::fontdb::Database,
        video_decoders: VideoDecodersWorker,
        on_frame_ready: &(dyn Fn() + Sync),
    ) {
        // Text caches are `Rc` based and therefore owned by the worker.
        let text_cache = TextCache::new(100);
        let mut converter_cache = usvgr::Cache::new_with_text_cache(100);

        while let Some(job) = self.next_job() {
            let frame = fframes::Frame::__internal_make_for_renderer(
                job.frame,
                job.frame,
                ctx.time_base.fps,
                text_cache.clone(),
                video_decoders.clone(),
            );

            // A panicking frame is reported with its time and scene and skipped.
            let tree = fframes::render_frame_guarded(video, frame, ctx)
                .map_err(fframes::FFramesRendererError::from)
                .and_then(|svgr| {
                    Ok(svgr.into_svg_tree(usvg_options, &mut converter_cache, font_db)?)
                });

            match tree {
                Ok(tree) => {
                    if self.complete(&job, tree) {
                        on_frame_ready();
                    }
                }
                Err(err) => eprintln!(
                    "fframes player: failed to render frame {}: {err}",
                    job.frame
                ),
            }
        }
    }
}
