use std::ops::Range;
use std::sync::Mutex;

/// Distributes the frames of a video between rendering workers.
///
/// Every worker starts with an equal, contiguous share of the timeline and renders it in
/// order into its own encoded segment. A worker that runs out of frames steals the second
/// half of the largest range still left to another worker and starts a new segment with
/// it, so all workers keep busy until the very end even when some frames are much more
/// expensive than others or the cores are not equally fast (performance and efficiency
/// cores). Contiguous ranges keep per-worker caches and video decoders sequential.
///
/// Every segment is at least `min_segment_frames` long (unless the whole video is
/// shorter), because each segment is encoded separately and starts with a keyframe.
#[doc(hidden)]
pub struct FrameScheduler {
    /// The frames left to each worker: `start` is the next frame it renders.
    ranges: Vec<Mutex<Range<usize>>>,
    min_segment_frames: usize,
}

impl FrameScheduler {
    pub fn new(total_frames: usize, workers: usize, min_segment_frames: usize) -> Self {
        let min_segment_frames = min_segment_frames.max(1);
        let workers = workers
            .min(total_frames / min_segment_frames)
            .max(1)
            .min(total_frames.max(1));

        let ranges = (0..workers)
            .map(|worker| {
                let start = total_frames * worker / workers;
                let end = total_frames * (worker + 1) / workers;
                Mutex::new(start..end)
            })
            .collect();

        Self {
            ranges,
            min_segment_frames,
        }
    }

    /// The number of workers that received an initial range.
    pub fn workers(&self) -> usize {
        self.ranges.len()
    }

    /// The first frame of the worker's current segment, `None` once the worker is done.
    pub fn initial_segment(&self, worker: usize) -> Option<usize> {
        let range = self.ranges.get(worker)?.lock().unwrap();
        (!range.is_empty()).then_some(range.start)
    }

    /// Claims the next frame of the worker's current segment. Returns `None` when the
    /// segment is finished (possibly earlier than planned because it was stolen from).
    pub fn next_frame(&self, worker: usize) -> Option<usize> {
        let mut range = self.ranges[worker].lock().unwrap();
        let frame = range.start;
        (frame < range.end).then(|| {
            range.start += 1;
            frame
        })
    }

    /// Moves the second half of the largest remaining range to `worker`, which must have
    /// finished its current segment. Returns the first frame of the new segment, or `None`
    /// when nothing is left that is worth splitting.
    pub fn steal(&self, worker: usize) -> Option<usize> {
        loop {
            let (victim, remaining) = self
                .ranges
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != worker)
                .map(|(index, range)| (index, range.lock().unwrap().len()))
                .max_by_key(|(_, remaining)| *remaining)?;

            if remaining < self.min_segment_frames * 2 {
                return None;
            }

            let stolen = {
                let mut victim_range = self.ranges[victim].lock().unwrap();
                // the victim kept rendering since we measured it
                if victim_range.len() < self.min_segment_frames * 2 {
                    continue;
                }

                let middle = victim_range.start + victim_range.len() / 2;
                let stolen = middle..victim_range.end;
                victim_range.end = middle;
                stolen
            };

            let start = stolen.start;
            *self.ranges[worker].lock().unwrap() = stolen;
            return Some(start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FrameScheduler;
    use std::collections::BTreeSet;

    fn drain(scheduler: &FrameScheduler, worker: usize, segment: &mut Vec<usize>) {
        while let Some(frame) = scheduler.next_frame(worker) {
            segment.push(frame);
        }
    }

    #[test]
    fn splits_evenly_and_respects_minimum() {
        let scheduler = FrameScheduler::new(100, 16, 24);
        assert_eq!(scheduler.workers(), 4);
        assert_eq!(scheduler.initial_segment(0), Some(0));
        assert_eq!(scheduler.initial_segment(3), Some(75));

        let tiny = FrameScheduler::new(3, 8, 24);
        assert_eq!(tiny.workers(), 1);
        let mut frames = vec![];
        drain(&tiny, 0, &mut frames);
        assert_eq!(frames, vec![0, 1, 2]);
    }

    #[test]
    fn stealing_covers_every_frame_once_in_contiguous_segments() {
        let scheduler = FrameScheduler::new(1000, 4, 10);
        let mut segments: Vec<Vec<usize>> = vec![];

        // worker 0 is fast: it finishes and keeps stealing while the others render one
        // frame per round
        let mut current = vec![vec![]; 4];
        let mut done = [false; 4];
        while done.iter().any(|done| !done) {
            for worker in 0..4 {
                if done[worker] {
                    continue;
                }
                let steps = if worker == 0 { 8 } else { 1 };
                for _ in 0..steps {
                    match scheduler.next_frame(worker) {
                        Some(frame) => current[worker].push(frame),
                        None => {
                            segments.push(std::mem::take(&mut current[worker]));
                            if scheduler.steal(worker).is_none() {
                                done[worker] = true;
                                break;
                            }
                        }
                    }
                }
            }
        }

        let mut all = BTreeSet::new();
        for segment in &segments {
            assert!(segment.windows(2).all(|w| w[1] == w[0] + 1));
            for frame in segment {
                assert!(all.insert(*frame), "frame {frame} rendered twice");
            }
        }
        assert_eq!(all.len(), 1000);
        assert!(segments.len() > 4, "the fast worker should have stolen");
        assert!(segments.iter().all(|segment| segment.len() >= 10));
    }
}
