//! Generates `usvgr::Tree`s ahead of the playhead on worker threads.
//!
//! Playback is expressed in *positions*: a monotonic frame counter that starts at the
//! seek target. Without looping a position is the frame index; with looping the frame is
//! `position % duration`, so frames prefetched across the loop boundary stay ordered.

use std::collections::BTreeMap;
use std::sync::{Condvar, Mutex, MutexGuard};

use crate::{app::FrameReady, studio::thumbnail};
use fframes::{FFramesContext, TextCache, Video, VideoDecodersWorker, usvgr};

pub(crate) const MAX_STAGED_TREES: usize = 4;

pub(crate) struct Scheduler {
    state: Mutex<State>,
    workers_wakeup: Condvar,
    duration: u64,
    lookahead: u64,
    thumbnails: usize,
    playback_decoders: VideoDecodersWorker,
    software_decode: bool,
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
    in_flight: usize,
    shutdown: bool,
    next_thumbnail: usize,
}

struct Job {
    frame: usize,
    kind: JobKind,
}

enum JobKind {
    Playback { position: u64, epoch: u64 },
    Thumbnail(usize),
}

#[derive(Clone, Copy)]
pub(crate) enum WorkerKind {
    Playback,
    Thumbnails,
}

impl Scheduler {
    pub(crate) fn new(
        duration_in_frames: usize,
        lookahead: usize,
        looping: bool,
        thumbnails: usize,
        software_decode: bool,
    ) -> Self {
        Self {
            state: Mutex::new(State {
                epoch: 0,
                next: 0,
                playhead: 0,
                displayed: None,
                looping,
                ready: BTreeMap::new(),
                in_flight: 0,
                shutdown: false,
                next_thumbnail: 0,
            }),
            workers_wakeup: Condvar::new(),
            duration: duration_in_frames.max(1) as u64,
            lookahead: lookahead.min(duration_in_frames) as u64,
            thumbnails,
            playback_decoders: VideoDecodersWorker::for_preview(software_decode),
            software_decode,
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // A panicking worker must not take the player down with a poisoned lock.
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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

    /// A forward seek within the finished buffer can show its target immediately.
    pub(crate) fn seek_buffered(&self, position: u64, looping: bool) -> bool {
        let mut state = self.lock();
        if state.looping != looping
            || position < state.playhead
            || !state.ready.contains_key(&position)
        {
            return false;
        }
        state.playhead = position;
        state.ready.retain(|&p, _| p >= position);
        drop(state);
        self.workers_wakeup.notify_all();
        true
    }

    /// Moves the playhead and returns the newest ready frame at or before it.
    /// Frames that are late are still returned so heavy videos degrade to a lower
    /// frame rate instead of freezing.
    pub(crate) fn take(&self, playhead: u64) -> Option<(u64, usvgr::Tree)> {
        let mut state = self.lock();
        let advanced = state.playhead != playhead;
        state.playhead = playhead;

        let taken = take_due(&mut state.ready, playhead);
        if let Some((position, _)) = &taken {
            state.displayed = Some(*position);
        }
        drop(state);

        if advanced || taken.is_some() {
            self.workers_wakeup.notify_all();
        }
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

            if let Some(job) = self.claim_playback(&mut state) {
                return Some(job);
            }

            state = self
                .workers_wakeup
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    fn claim_playback(&self, state: &mut State) -> Option<Job> {
        let position = state.next.max(state.playhead);
        let staged = state.ready.len() + state.in_flight;
        if position > state.playhead.saturating_add(self.lookahead)
            || (!state.looping && position >= self.duration)
            || (position > state.playhead && staged >= self.lookahead.max(1) as usize)
        {
            return None;
        }
        // Bound prepared trees and in-flight work. A new live position
        // can bypass that limit if the clock jumped ahead of all assigned work.
        state.next = position + 1;
        state.in_flight += 1;
        Some(Job {
            frame: self.frame_of(position, state.looping),
            kind: JobKind::Playback {
                position,
                epoch: state.epoch,
            },
        })
    }

    // The filmstrip seeks across the video with its own decoder.
    fn next_thumbnail_job(&self) -> Option<Job> {
        let mut state = self.lock();
        while !state.shutdown && state.displayed.is_none() {
            state = self
                .workers_wakeup
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        if state.shutdown || state.next_thumbnail >= self.thumbnails {
            return None;
        }
        let slot = state.next_thumbnail;
        state.next_thumbnail += 1;
        let frame = ((2 * slot + 1) as u128 * u128::from(self.duration)
            / (2 * self.thumbnails) as u128) as usize;
        Some(Job {
            frame,
            kind: JobKind::Thumbnail(slot),
        })
    }

    pub(crate) fn is_current_seek(&self, position: u64, epoch: u64) -> bool {
        let state = self.lock();
        state.playhead == position && state.epoch == epoch
    }

    fn complete(&self, position: u64, epoch: u64, tree: Option<usvgr::Tree>) -> bool {
        let mut state = self.lock();
        state.in_flight -= 1;
        let is_stale = state.epoch != epoch
            || state
                .displayed
                .is_some_and(|displayed| position <= displayed);

        if !is_stale && let Some(tree) = tree {
            state.ready.insert(position, tree);
            return true;
        }
        drop(state);
        self.workers_wakeup.notify_one();
        false
    }

    pub(crate) fn run_worker<'a, 'media: 'a, TVideo: Video + Sync>(
        &self,
        video: &'a TVideo,
        ctx: &FFramesContext<'a, 'media>,
        usvg_options: &usvgr::Options<'_>,
        font_db: &usvgr::fontdb::Database,
        kind: WorkerKind,
        on_frame_ready: &(dyn Fn(FrameReady) + Sync),
    ) {
        // Text caches are `Rc` based and therefore owned by the worker.
        // A changing frame counter would otherwise retain 100 sets of glyph outlines per worker.
        const TEXT_CACHE_ENTRIES: usize = 8;
        let text_cache = TextCache::new(TEXT_CACHE_ENTRIES);
        let mut converter_cache = usvgr::Cache::new_with_text_cache(TEXT_CACHE_ENTRIES);
        let video_decoders = match kind {
            WorkerKind::Playback => self.playback_decoders.clone(),
            WorkerKind::Thumbnails => VideoDecodersWorker::for_preview(self.software_decode),
        };

        while let Some(job) = match kind {
            WorkerKind::Playback => self.next_job(),
            WorkerKind::Thumbnails => self.next_thumbnail_job(),
        } {
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

            let concurrent_jobs = match kind {
                WorkerKind::Playback => self.lock().in_flight,
                WorkerKind::Thumbnails => 1,
            };
            video_decoders.finish_preview_frame(concurrent_jobs);
            match (tree, job.kind) {
                (Ok(tree), JobKind::Thumbnail(slot)) => {
                    if let Some(image) = thumbnail(&tree, TVideo::BACKGROUND_COLOR) {
                        on_frame_ready(FrameReady::Thumbnail(slot, image));
                    }
                }
                (Ok(tree), JobKind::Playback { position, epoch }) => {
                    if self.complete(position, epoch, Some(tree)) {
                        on_frame_ready(FrameReady::Playback);
                    }
                }
                (Err(err), kind) => {
                    eprintln!(
                        "fframes player: failed to render frame {}: {err}",
                        job.frame
                    );
                    if let JobKind::Playback { position, epoch } = kind {
                        self.complete(position, epoch, None);
                        on_frame_ready(FrameReady::Failed { position, epoch });
                    }
                }
            }
        }
    }
}

fn take_due<T>(ready: &mut BTreeMap<u64, T>, playhead: u64) -> Option<(u64, T)> {
    let mut latest = None;
    while ready.first_key_value().is_some_and(|(&p, _)| p <= playhead) {
        latest = ready.pop_first();
    }
    latest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> usvgr::Tree {
        usvgr::Tree::from_str(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"/>"#,
            &usvgr::Options::default(),
            &usvgr::fontdb::Database::new(),
        )
        .unwrap()
    }

    #[test]
    fn taking_out_of_order_frames_preserves_future_frames_and_rejects_stale_jobs() {
        let scheduler = Scheduler::new(100, 4, false, 2, false);
        for index in 0..=3 {
            assert_eq!(scheduler.next_job().unwrap().frame, index);
        }
        for index in [2, 1] {
            assert!(scheduler.complete(index, 0, Some(tree())));
        }
        assert_eq!(scheduler.take(1).unwrap().0, 1);
        assert!(scheduler.take(1).is_none());
        assert!(!scheduler.complete(0, 0, Some(tree())));
        assert_eq!(scheduler.take(2).unwrap().0, 2);
        scheduler.seek(50, false);
        assert!(!scheduler.complete(3, 0, Some(tree())));
        assert_eq!(scheduler.next_job().unwrap().frame, 50);
        assert!(scheduler.complete(50, 1, Some(tree())));
        assert_eq!(scheduler.take(50).unwrap().0, 50);
        scheduler.shutdown();
        assert!(scheduler.next_job().is_none());
    }

    #[test]
    fn prepared_frames_are_bounded_and_forward_seeks_reuse_trees() {
        let scheduler = Scheduler::new(100, 4, false, 0, false);
        for p in 0..4 {
            assert_eq!(scheduler.next_job().unwrap().frame, p);
            assert!(scheduler.complete(p as u64, 0, Some(tree())));
        }
        assert!(scheduler.claim_playback(&mut scheduler.lock()).is_none());
        assert_eq!(scheduler.take(0).unwrap().0, 0);
        assert_eq!(scheduler.next_job().unwrap().frame, 4);
        scheduler.complete(4, 0, Some(tree()));
        assert!(scheduler.seek_buffered(3, false));
        assert_eq!(scheduler.take(3).unwrap().0, 3);
        assert!(!scheduler.seek_buffered(4, true));
        assert!(!scheduler.seek_buffered(0, false));
        assert_eq!(scheduler.take(4).unwrap().0, 4);
    }

    #[test]
    fn live_frames_take_priority_over_queued_trees() {
        let scheduler = Scheduler::new(100, 4, false, 0, false);
        for p in 0..4 {
            assert_eq!(scheduler.next_job().unwrap().frame as u64, p);
            scheduler.complete(p, 0, Some(tree()));
        }
        assert!(scheduler.claim_playback(&mut scheduler.lock()).is_none());
        assert_eq!(scheduler.take(30).unwrap().0, 3);
        assert_eq!(scheduler.next_job().unwrap().frame, 30);
        scheduler.complete(30, 0, Some(tree()));
        assert_eq!(scheduler.take(30).unwrap().0, 30);
    }

    #[test]
    fn workers_bound_in_flight_frames_and_release_failed_jobs() {
        let scheduler = Scheduler::new(100, 4, false, 0, false);
        for p in 0..4 {
            assert_eq!(scheduler.next_job().unwrap().frame, p);
        }
        assert!(scheduler.claim_playback(&mut scheduler.lock()).is_none());
        scheduler.complete(2, 0, None);
        assert_eq!(scheduler.next_job().unwrap().frame, 4);
        scheduler.take(30);
        assert_eq!(scheduler.next_job().unwrap().frame, 30);
        scheduler.seek(60, false);
        assert!(!scheduler.complete(1, 0, Some(tree())));
        assert_eq!(scheduler.next_job().unwrap().frame, 60);
    }

    #[test]
    fn zero_prefetch_only_prepares_the_live_frame() {
        let scheduler = Scheduler::new(3, 0, false, 0, false);
        for p in 0..3 {
            scheduler.take(p);
            assert_eq!(scheduler.next_job().unwrap().frame as u64, p);
            assert!(scheduler.claim_playback(&mut scheduler.lock()).is_none());
            assert!(scheduler.complete(p, 0, Some(tree())));
            assert_eq!(scheduler.take(p).unwrap().0, p);
        }
        assert!(scheduler.claim_playback(&mut scheduler.lock()).is_none());
    }

    #[test]
    fn looping_buffer_uses_monotonic_positions_across_the_end() {
        let scheduler = Scheduler::new(3, 2, true, 0, false);
        scheduler.seek(2, true);
        for p in 2..4 {
            let job = scheduler.next_job().unwrap();
            assert_eq!(job.frame, p % 3);
            assert!(scheduler.complete(p as u64, 1, Some(tree())));
        }
        assert_eq!(scheduler.take(2).unwrap().0, 2);
        assert!(scheduler.seek_buffered(3, true));
        assert_eq!(scheduler.take(3).unwrap().0, 3);
        assert_eq!(scheduler.next_job().unwrap().frame, 1);
    }

    #[test]
    fn thumbnails_do_not_consume_playback_jobs_and_stop_when_finished() {
        let scheduler = Scheduler::new(100, 3, false, 2, false);
        assert_eq!(scheduler.next_job().unwrap().frame, 0);
        scheduler.complete(0, 0, Some(tree()));
        scheduler.take(0).unwrap();
        assert_eq!(scheduler.next_thumbnail_job().unwrap().frame, 25);
        assert_eq!(scheduler.next_job().unwrap().frame, 1);
        assert_eq!(scheduler.next_thumbnail_job().unwrap().frame, 75);
        assert!(scheduler.next_thumbnail_job().is_none());
        assert_eq!(scheduler.next_job().unwrap().frame, 2);
        let stopped = Scheduler::new(100, 3, false, 2, false);
        stopped.shutdown();
        assert!(stopped.next_thumbnail_job().is_none());
    }
}
