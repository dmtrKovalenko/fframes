use std::sync::Mutex;

/// A frame handed to a worker by the [`FrameScheduler`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameClaim {
    /// Identifies the encoded segment the frame belongs to (its first frame).
    pub segment: usize,
    pub frame: usize,
    /// No other frame of this segment will be handed out after this one.
    pub last_in_segment: bool,
}

/// The most segments a timeline is cut into, unless that would make them shorter than
/// `min_segment_frames`. Each segment is a separate encoder run. Fewer, longer segments
/// leave threads idle for longer at the end of a render, while the last ones encode, and
/// the Skia pipeline encodes up to 1.5 segments per thread at once.
const MAX_SEGMENTS: usize = 128;

/// The segments a timeline is encoded in. They depend only on the number of frames and
/// `min_segment_frames`, never on the number of workers or on how fast they go.
#[derive(Debug, Clone, Copy)]
struct Segments {
    total_frames: usize,
    /// The length of every segment but the last, a multiple of `min_segment_frames`.
    length: usize,
    count: usize,
}

impl Segments {
    fn new(total_frames: usize, min_segment_frames: usize) -> Self {
        let length = total_frames
            .div_ceil(MAX_SEGMENTS)
            .next_multiple_of(min_segment_frames)
            .max(min_segment_frames);
        // a last segment shorter than the minimum joins the one before it
        let count = if total_frames % length >= min_segment_frames {
            total_frames.div_ceil(length)
        } else {
            total_frames / length
        }
        .max(1);

        Self {
            total_frames,
            length,
            count,
        }
    }

    /// The segment `frame` is in.
    fn index(&self, frame: usize) -> usize {
        (frame / self.length).min(self.count - 1)
    }

    /// The first frame of segment `index`, the end of the timeline past the last segment.
    fn start(&self, index: usize) -> usize {
        if index < self.count {
            index * self.length
        } else {
            self.total_frames
        }
    }
}

#[derive(Debug)]
struct Slot {
    next: usize,
    /// The start of a segment, or the end of the timeline.
    end: usize,
}

impl Slot {
    fn remaining(&self) -> usize {
        self.end - self.next
    }

    fn claim(&mut self, segments: &Segments) -> Option<FrameClaim> {
        (self.next < self.end).then(|| {
            let frame = self.next;
            self.next += 1;
            let index = segments.index(frame);
            FrameClaim {
                segment: segments.start(index),
                frame,
                last_in_segment: self.next == segments.start(index + 1),
            }
        })
    }

    /// Where another worker can take over the frames left: the segment start at or before
    /// their middle, but after the segment of the next frame.
    fn split(&self, segments: &Segments) -> Option<usize> {
        if self.remaining() == 0 {
            return None;
        }
        let first = segments.start(segments.index(self.next) + 1);
        let last = segments.start(segments.index(self.end - 1));
        (first <= last)
            .then(|| align(self.next + self.remaining() / 2, segments.length).clamp(first, last))
    }
}

/// `frame` rounded down to a multiple of `step`.
fn align(frame: usize, step: usize) -> usize {
    frame - frame % step
}

/// Distributes the frames of a video between rendering workers.
///
/// The timeline is cut into segments before rendering starts: at most `MAX_SEGMENTS` of the
/// same length, a multiple of `min_segment_frames`. Every segment is encoded separately, and
/// the cuts depend only on the number of frames and `min_segment_frames`, so two renders of
/// the same frames encode to the same bytes, whatever the number of workers and however
/// fast each one goes.
///
/// Every worker starts with an equal, contiguous run of segments and renders it in order,
/// which keeps per-worker caches and video decoders sequential. A worker that runs out of
/// frames:
///
/// 1. takes the second half of the segments another worker has not started yet, from the
///    worker with the most frames left;
/// 2. otherwise helps with the frames right after the ones another worker is rendering,
///    in that worker's segment, so nobody idles while the last frames render. The
///    segment writer puts such frames back in order.
///
/// Segments start on multiples of `min_segment_frames` (the GOP). Every segment opens with
/// a keyframe, and there the encoder would have placed one anyway.
#[doc(hidden)]
pub struct FrameScheduler {
    slots: Vec<Mutex<Slot>>,
    segments: Segments,
}

impl FrameScheduler {
    pub fn new(total_frames: usize, workers: usize, min_segment_frames: usize) -> Self {
        let segments = Segments::new(total_frames, min_segment_frames.max(1));
        let workers = workers.max(1);
        // workers without an initial run of segments only help the others
        let runs = workers.min(segments.count);

        let slots = (0..workers)
            .map(|worker| {
                let (first, last) = if worker < runs {
                    (
                        segments.count * worker / runs,
                        segments.count * (worker + 1) / runs,
                    )
                } else {
                    (segments.count, segments.count)
                };
                Mutex::new(Slot {
                    next: segments.start(first),
                    end: segments.start(last),
                })
            })
            .collect();

        Self { slots, segments }
    }

    pub fn workers(&self) -> usize {
        self.slots.len()
    }

    /// The next frame `worker` should render, `None` once every frame was handed out.
    pub fn claim(&self, worker: usize) -> Option<FrameClaim> {
        if let Some(claim) = self.slots[worker].lock().unwrap().claim(&self.segments) {
            return Some(claim);
        }

        loop {
            // the most frames left, preferring segments another worker can take over
            let (victim, (_, remaining)) = self
                .slots
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != worker)
                .map(|(index, slot)| {
                    let slot = slot.lock().unwrap();
                    (
                        index,
                        (slot.split(&self.segments).is_some(), slot.remaining()),
                    )
                })
                .max_by_key(|(_, key)| *key)?;

            if remaining == 0 {
                return None;
            }

            let mut victim_slot = self.slots[victim].lock().unwrap();
            if victim_slot.remaining() == 0 {
                // finished since we measured it
                continue;
            }

            if let Some(start) = victim_slot.split(&self.segments) {
                let mut own = Slot {
                    next: start,
                    end: victim_slot.end,
                };
                victim_slot.end = start;
                drop(victim_slot);

                let claim = own.claim(&self.segments);
                *self.slots[worker].lock().unwrap() = own;
                return claim;
            }

            return victim_slot.claim(&self.segments);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameScheduler, MAX_SEGMENTS};
    use std::collections::{BTreeMap, BTreeSet};

    /// Claims every frame with workers that go at different speeds, returns the frames of
    /// each segment in the order they were claimed.
    fn claim_all(scheduler: &FrameScheduler, pace: usize) -> BTreeMap<usize, Vec<usize>> {
        let workers = scheduler.workers();
        let mut segments: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut last_flags: BTreeMap<usize, usize> = BTreeMap::new();
        let mut done = vec![false; workers];

        let mut round = 0;
        while done.iter().any(|done| !done) {
            round += 1;
            for (worker, done) in done.iter_mut().enumerate() {
                if *done || round % ((worker * pace) % 5 + 1) != 0 {
                    continue;
                }
                match scheduler.claim(worker) {
                    Some(claim) => {
                        segments.entry(claim.segment).or_default().push(claim.frame);
                        if claim.last_in_segment {
                            assert!(last_flags.insert(claim.segment, claim.frame).is_none());
                        }
                    }
                    None => *done = true,
                }
            }
        }

        for (start, frames) in &segments {
            assert_eq!(last_flags[start], *frames.iter().max().unwrap());
        }
        segments
    }

    #[test]
    fn splits_evenly_and_respects_minimum() {
        let scheduler = FrameScheduler::new(100, 16, 24);
        assert_eq!(scheduler.workers(), 16);
        assert_eq!(scheduler.claim(0).unwrap().frame, 0);
        assert_eq!(scheduler.claim(3).unwrap().frame, 72);

        let tiny = FrameScheduler::new(3, 8, 24);
        let frames: Vec<_> = std::iter::from_fn(|| tiny.claim(5)).collect();
        assert_eq!(frames.len(), 3);
        assert!(frames.iter().all(|claim| claim.segment == 0));
        assert!(frames[2].last_in_segment);
        assert_eq!(tiny.claim(0), None);
    }

    #[test]
    fn every_frame_is_claimed_once_and_segments_are_contiguous() {
        for (total, workers, min) in [(1000, 4, 10), (97, 16, 24), (48, 3, 24), (5000, 16, 24)] {
            let scheduler = FrameScheduler::new(total, workers, min);
            let segments = claim_all(&scheduler, 1);
            assert!(segments.len() <= MAX_SEGMENTS);

            let mut all = BTreeSet::new();
            for (start, frames) in &segments {
                let mut sorted = frames.clone();
                sorted.sort_unstable();
                assert_eq!(sorted[0], *start);
                assert_eq!(start % min, 0, "segment {start} does not start on the GOP");
                assert!(sorted.windows(2).all(|w| w[1] == w[0] + 1));
                assert!(sorted.len() >= min.min(total));
                for frame in frames {
                    assert!(all.insert(*frame), "frame {frame} claimed twice");
                }
            }
            assert_eq!(all.len(), total);
        }
    }

    #[test]
    fn segments_do_not_depend_on_the_workers() {
        for (total, min) in [
            (4885, 24),
            (7650, 24),
            (1000, 10),
            (141, 24),
            (97, 24),
            (3, 24),
        ] {
            // where every segment starts and how many frames it has
            let cuts = |workers: usize, pace: usize| -> Vec<(usize, usize)> {
                let scheduler = FrameScheduler::new(total, workers, min);
                claim_all(&scheduler, pace)
                    .into_iter()
                    .map(|(start, frames)| (start, frames.len()))
                    .collect()
            };

            let one_worker = cuts(1, 1);
            for workers in [2, 3, 6, 12, 18, 24, 40] {
                let expected = cuts(workers, 1);
                for pace in [2, 3] {
                    assert_eq!(
                        cuts(workers, pace),
                        expected,
                        "{total} frames, {workers} workers at pace {pace} and 1"
                    );
                }
                assert_eq!(
                    expected, one_worker,
                    "{total} frames, {workers} workers and 1"
                );
            }
        }
    }
}
