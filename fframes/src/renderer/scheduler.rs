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

/// The smallest run of frames a worker takes over from another worker inside that
/// worker's segment when the render decodes video: a worker that joins a segment
/// mid-way pays for positioning its decoders (a seek and every frame since the previous
/// keyframe), so single frames are never worth it. Without video sources any frame is.
pub const MIN_HELP_FRAMES_WITH_VIDEO: usize = 8;

#[derive(Debug)]
struct Slot {
    /// The segment the frames of this slot are encoded into.
    segment: usize,
    next: usize,
    end: usize,
    /// End of the segment; the slot may cover only a part of it.
    segment_end: usize,
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
                last_in_segment: self.next == self.segment_end,
            }
        })
    }
}

/// Distributes the frames of a video between rendering workers.
///
/// Every worker starts with a contiguous share of the timeline and renders it in order
/// into its own encoded segment, which keeps per-worker caches and video decoders
/// sequential. Segment boundaries prefer the frames where a scene starts
/// ([`Self::with_cut_points`]): a video clip usually starts with its scene, and a worker
/// that begins there decodes the clip from its first frame instead of seeking into it
/// and decoding everything since the previous keyframe first.
///
/// A worker that runs out of frames:
///
/// 1. takes the second half of the largest range left to another worker as a new
///    segment, when both halves are at least `min_segment_frames` long (every segment
///    is encoded separately and starts with a keyframe). The split prefers a scene start
///    in the middle half of the range;
/// 2. otherwise takes the second half of the frames another worker still has to render,
///    in that worker's segment, as long as both halves are at least [`MIN_HELP_FRAMES`]
///    long. The segment writer puts such frames back in order.
#[doc(hidden)]
pub struct FrameScheduler {
    slots: Vec<Mutex<Slot>>,
    min_segment_frames: usize,
    min_help_frames: usize,
    /// Sorted frames where a scene starts, relative to the first rendered frame.
    cut_points: Vec<usize>,
}

impl FrameScheduler {
    pub fn new(total_frames: usize, workers: usize, min_segment_frames: usize) -> Self {
        Self::with_cut_points(total_frames, workers, min_segment_frames, &[], 1)
    }

    /// Like [`Self::new`], with the frames where segments should preferably start and the
    /// smallest run of frames a worker takes over inside another worker's segment.
    pub fn with_cut_points(
        total_frames: usize,
        workers: usize,
        min_segment_frames: usize,
        cut_points: &[usize],
        min_help_frames: usize,
    ) -> Self {
        let min_segment_frames = min_segment_frames.max(1);
        let min_help_frames = min_help_frames.max(1);
        let segments = workers
            .min(total_frames / min_segment_frames)
            .max(1)
            .min(total_frames.max(1));

        let mut cut_points: Vec<usize> = cut_points
            .iter()
            .copied()
            .filter(|&frame| frame > 0 && frame < total_frames)
            .collect();
        cut_points.sort_unstable();
        cut_points.dedup();

        // Even boundaries, each moved to the closest cut point that is not further away
        // than half a segment and keeps every segment long enough.
        let ideal = |segment: usize| total_frames * segment / segments;
        let tolerance = (total_frames / segments) / 2;
        let mut boundaries = Vec::with_capacity(segments + 1);
        boundaries.push(0);
        for segment in 1..segments {
            let previous = boundaries[segment - 1];
            let wanted = ideal(segment);
            let snapped = Self::nearest_cut_point(&cut_points, wanted, tolerance).filter(|&cut| {
                cut >= previous + min_segment_frames
                    && total_frames - cut >= min_segment_frames * (segments - segment)
            });
            boundaries.push(snapped.unwrap_or(wanted).max(previous));
        }
        boundaries.push(total_frames);

        let mut slots: Vec<_> = boundaries
            .windows(2)
            .map(|range| {
                Mutex::new(Slot {
                    segment: range[0],
                    next: range[0],
                    end: range[1],
                    segment_end: range[1],
                })
            })
            .collect();

        // workers without an initial range only help the others
        slots.extend((segments..workers.max(1)).map(|_| {
            Mutex::new(Slot {
                segment: total_frames,
                next: total_frames,
                end: total_frames,
                segment_end: total_frames,
            })
        }));

        Self {
            slots,
            min_segment_frames,
            min_help_frames,
            cut_points,
        }
    }

    /// The cut point closest to `frame` within `tolerance`.
    fn nearest_cut_point(cut_points: &[usize], frame: usize, tolerance: usize) -> Option<usize> {
        let index = cut_points.partition_point(|&cut| cut < frame);
        let after = cut_points.get(index).copied();
        let before = index.checked_sub(1).map(|index| cut_points[index]);
        [before, after]
            .into_iter()
            .flatten()
            .filter(|&cut| cut.abs_diff(frame) <= tolerance)
            .min_by_key(|&cut| cut.abs_diff(frame))
    }

    /// Where to split `next..end` for a worker that takes the second half: a scene start
    /// in the middle half of the range, otherwise the middle itself.
    fn split_point(&self, next: usize, end: usize) -> usize {
        let remaining = end - next;
        let middle = next + remaining / 2;
        Self::nearest_cut_point(&self.cut_points, middle, remaining / 4)
            .filter(|&cut| cut > next && cut < end)
            .unwrap_or(middle)
    }

    pub fn workers(&self) -> usize {
        self.slots.len()
    }

    /// The next frame `worker` should render, `None` once this worker has nothing left to do.
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
            let remaining = victim_slot.remaining();
            if remaining == 0 {
                // finished since we measured it
                continue;
            }

            let mut own = if remaining >= self.min_segment_frames * 2 {
                let split = self.split_point(victim_slot.next, victim_slot.end);
                let split = split.clamp(
                    victim_slot.next + self.min_segment_frames,
                    victim_slot.end - self.min_segment_frames,
                );
                // a new segment, encoded on its own
                let own = Slot {
                    segment: split,
                    next: split,
                    end: victim_slot.end,
                    segment_end: victim_slot.segment_end,
                };
                victim_slot.end = split;
                victim_slot.segment_end = split;
                own
            } else if remaining >= self.min_help_frames * 2 {
                // the tail of the victim's run, inside its segment
                let split = victim_slot.next + remaining / 2;
                let own = Slot {
                    segment: victim_slot.segment,
                    next: split,
                    end: victim_slot.end,
                    segment_end: victim_slot.segment_end,
                };
                victim_slot.end = split;
                own
            } else {
                // Too few frames to be worth positioning this worker's decoders: the
                // victim finishes them alone.
                return None;
            };
            drop(victim_slot);

            let claim = own.claim();
            *self.slots[worker].lock().unwrap() = own;
            return claim;
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

        // one segment of three frames; a second worker takes its tail as a block
        let tiny = FrameScheduler::new(3, 8, 24);
        let helped: Vec<_> = std::iter::from_fn(|| tiny.claim(5)).collect();
        assert_eq!(
            helped.iter().map(|claim| claim.frame).collect::<Vec<_>>(),
            [1, 2]
        );
        assert!(helped.iter().all(|claim| claim.segment == 0));
        assert!(helped[1].last_in_segment);
        let own = tiny.claim(0).unwrap();
        assert_eq!((own.frame, own.last_in_segment), (0, false));
        assert_eq!(tiny.claim(0), None);
    }

    #[test]
    fn boundaries_snap_to_nearby_cut_points() {
        // Eight shots of uneven length over 926 frames: every worker starts a shot.
        let ends = [123, 238, 386, 500, 613, 729, 840, 926];
        let scheduler = FrameScheduler::with_cut_points(926, 8, 24, &ends, 1);
        let firsts: Vec<_> = (0..8)
            .map(|worker| scheduler.claim(worker).unwrap().frame)
            .collect();
        assert_eq!(firsts, [0, 123, 238, 386, 500, 613, 729, 840]);

        // A cut point further away than half a segment is ignored.
        let scheduler = FrameScheduler::with_cut_points(1000, 2, 24, &[100], 1);
        assert_eq!(scheduler.claim(1).unwrap().frame, 500);

        // Snapping never produces a segment shorter than the minimum.
        let scheduler = FrameScheduler::with_cut_points(100, 4, 24, &[20, 26, 74], 1);
        let firsts: Vec<_> = (0..4)
            .map(|worker| scheduler.claim(worker).unwrap().frame)
            .collect();
        assert_eq!(firsts, [0, 26, 50, 74]);
    }

    #[test]
    fn stealing_prefers_a_cut_point() {
        // The cut point is too far from the even split to move it, and outside the
        // middle half of worker 0's range: worker 1 takes the second half at 100.
        let scheduler = FrameScheduler::with_cut_points(400, 2, 10, &[30], 1);
        let own: Vec<_> = std::iter::from_fn(|| scheduler.claim(1))
            .take(200)
            .collect();
        assert_eq!(own[0].frame, 200);
        assert!(own[199].last_in_segment);
        let stolen = scheduler.claim(1).unwrap();
        assert_eq!((stolen.segment, stolen.frame), (100, 100));

        // A cut point in the middle half of the range splits it there.
        let scheduler = FrameScheduler::with_cut_points(400, 2, 10, &[60], 1);
        for _ in 0..200 {
            scheduler.claim(1).unwrap();
        }
        let stolen = scheduler.claim(1).unwrap();
        assert_eq!((stolen.segment, stolen.frame), (60, 60));
    }

    #[test]
    fn helping_takes_blocks_and_leaves_short_tails_alone() {
        let min_help = 8;
        let total = 30 + 2 * min_help + 2;
        let scheduler = FrameScheduler::with_cut_points(total, 2, 100, &[], min_help);
        for _ in 0..30 {
            scheduler.claim(0).unwrap();
        }
        // the tail of worker 0's run, as one block inside segment 0
        let help = scheduler.claim(1).unwrap();
        assert_eq!(help.segment, 0);
        assert_eq!(help.frame, 30 + (total - 30) / 2);
        let helped: Vec<_> = std::iter::once(help)
            .chain(std::iter::from_fn(|| scheduler.claim(1)))
            .collect();
        assert_eq!(helped.len(), total - help.frame);
        assert!(helped.last().unwrap().last_in_segment);
        let rest: Vec<_> = std::iter::from_fn(|| scheduler.claim(0)).collect();
        assert_eq!(rest.len(), help.frame - 30);
        assert!(rest.iter().all(|claim| !claim.last_in_segment));

        // fewer than two blocks left: not worth another worker's decoder setup
        let scheduler =
            FrameScheduler::with_cut_points(30 + 2 * min_help - 1, 2, 100, &[], min_help);
        for _ in 0..30 {
            scheduler.claim(0).unwrap();
        }
        assert_eq!(scheduler.claim(1), None);
        assert_eq!(
            std::iter::from_fn(|| scheduler.claim(0)).count(),
            2 * min_help - 1
        );

        // without video sources single frames are still handed out
        let scheduler = FrameScheduler::new(32, 2, 100);
        for _ in 0..30 {
            scheduler.claim(0).unwrap();
        }
        assert_eq!(scheduler.claim(1).unwrap().frame, 31);
    }

    #[test]
    fn every_frame_is_claimed_once_and_segments_are_contiguous() {
        for (total, workers, min, cuts, help) in [
            (1000, 4, 10, vec![], 1),
            (97, 16, 24, vec![], 1),
            (48, 3, 24, vec![], 1),
            (5000, 16, 24, vec![], 1),
            (926, 8, 24, vec![123, 238, 386, 500, 613, 729, 840, 926], 30),
            (1000, 6, 24, vec![3, 170, 171, 500, 999], 8),
            (480, 16, 24, vec![], 30),
        ] {
            let scheduler = FrameScheduler::with_cut_points(total, workers, min, &cuts, help);
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
                sorted.sort_unstable();
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
