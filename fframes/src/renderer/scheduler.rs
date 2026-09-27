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

#[derive(Debug)]
struct Slot {
    segment: usize,
    next: usize,
    end: usize,
}

impl Slot {
    fn remaining(&self) -> usize {
        self.end - self.next
    }

    fn claim(&mut self) -> Option<FrameClaim> {
        (self.next < self.end).then(|| {
            let frame = self.next;
            self.next += 1;
            FrameClaim {
                segment: self.segment,
                frame,
                last_in_segment: self.next == self.end,
            }
        })
    }
}

/// Distributes the frames of a video between rendering workers.
///
/// Every worker starts with an equal, contiguous share of the timeline and renders it in
/// order into its own encoded segment, which keeps per-worker caches and video decoders
/// sequential. A worker that runs out of frames:
///
/// 1. takes the second half of the largest range left to another worker as a new
///    segment, when both halves are at least `min_segment_frames` long (every segment
///    is encoded separately and starts with a keyframe);
/// 2. otherwise helps with the frames right after the ones another worker is rendering,
///    in that worker's segment, so nobody idles while the last frames render. The
///    segment writer puts such frames back in order.
#[doc(hidden)]
pub struct FrameScheduler {
    slots: Vec<Mutex<Slot>>,
    min_segment_frames: usize,
}

impl FrameScheduler {
    pub fn new(total_frames: usize, workers: usize, min_segment_frames: usize) -> Self {
        let min_segment_frames = min_segment_frames.max(1);
        let segments = workers
            .min(total_frames / min_segment_frames)
            .max(1)
            .min(total_frames.max(1));

        let mut slots: Vec<_> = (0..segments)
            .map(|segment| {
                let start = total_frames * segment / segments;
                let end = total_frames * (segment + 1) / segments;
                Mutex::new(Slot {
                    segment: start,
                    next: start,
                    end,
                })
            })
            .collect();

        // workers without an initial range only help the others
        slots.extend((segments..workers.max(1)).map(|_| {
            Mutex::new(Slot {
                segment: total_frames,
                next: total_frames,
                end: total_frames,
            })
        }));

        Self {
            slots,
            min_segment_frames,
        }
    }

    pub fn workers(&self) -> usize {
        self.slots.len()
    }

    /// The next frame `worker` should render, `None` once every frame was handed out.
    pub fn claim(&self, worker: usize) -> Option<FrameClaim> {
        if let Some(claim) = self.slots[worker].lock().unwrap().claim() {
            return Some(claim);
        }

        loop {
            let (victim, remaining) = self
                .slots
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != worker)
                .map(|(index, slot)| (index, slot.lock().unwrap().remaining()))
                .max_by_key(|(_, remaining)| *remaining)?;

            if remaining == 0 {
                return None;
            }

            let mut victim_slot = self.slots[victim].lock().unwrap();
            if victim_slot.remaining() == 0 {
                // finished since we measured it
                continue;
            }

            if victim_slot.remaining() >= self.min_segment_frames * 2 {
                let middle = victim_slot.next + victim_slot.remaining() / 2;
                let mut own = Slot {
                    segment: middle,
                    next: middle,
                    end: victim_slot.end,
                };
                victim_slot.end = middle;
                drop(victim_slot);

                let claim = own.claim();
                *self.slots[worker].lock().unwrap() = own;
                return claim;
            }

            return victim_slot.claim();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FrameScheduler;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn splits_evenly_and_respects_minimum() {
        let scheduler = FrameScheduler::new(100, 16, 24);
        assert_eq!(scheduler.workers(), 16);
        assert_eq!(scheduler.claim(0).unwrap().frame, 0);
        assert_eq!(scheduler.claim(3).unwrap().frame, 75);

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
            let mut segments: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            let mut last_flags: BTreeMap<usize, usize> = BTreeMap::new();
            let mut done = vec![false; workers];

            // workers progress at different speeds
            let mut round = 0;
            while done.iter().any(|done| !done) {
                round += 1;
                for (worker, done) in done.iter_mut().enumerate() {
                    if *done || round % (worker % 3 + 1) != 0 {
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

            let mut all = BTreeSet::new();
            for (start, frames) in &segments {
                let mut sorted = frames.clone();
                sorted.sort();
                assert_eq!(sorted[0], *start);
                assert!(sorted.windows(2).all(|w| w[1] == w[0] + 1));
                assert_eq!(last_flags[start], *sorted.last().unwrap());
                assert!(sorted.len() >= min.min(total));
                for frame in frames {
                    assert!(all.insert(*frame), "frame {frame} claimed twice");
                }
            }
            assert_eq!(all.len(), total);
        }
    }
}
