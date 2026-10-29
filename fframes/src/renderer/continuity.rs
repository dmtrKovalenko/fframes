//! Finds pops: parts of a picture that change in a single frame between frames in which they
//! stand still. An element that appears, vanishes or jumps in front of another element without
//! moving there is easy to miss in a contact sheet but obvious when the video plays.
//!
//! Frames are compared in square blocks. A block pops when it changes a lot from one frame to
//! the next while it, and the blocks around it, are still in the comparison before and after.
//! A single frame that differs from the frames on both sides (a flash) is reported too. Motion
//! changes the same area in consecutive frames and is therefore not reported; neither are cuts
//! that change most of the picture at once.
use super::RgbaFrame;
use serde::Serialize;

/// Block edge in pixels of the compared frames.
const BLOCK: u32 = 8;
/// Mean difference per channel (0-255) of a block that counts as a sudden change.
const SUDDEN: f32 = 20.;
/// Mean difference per channel below which a block counts as standing still.
const STILL: f32 = 3.;
/// Share of the picture above which a change is a cut, not a pop.
const CUT: f32 = 0.3;

/// What changed abruptly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PopKind {
    /// Changed between two frames and then stayed: something appeared, vanished or swapped
    /// places with another element.
    Step,
    /// One frame differs from the frames before and after it.
    Flash,
}

/// A region that popped, in the pixels of the compared frames.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pop {
    /// The first frame (index in the sequence fed to [`PopDetector::push`]) that shows the
    /// change.
    pub frame: usize,
    pub kind: PopKind,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Largest mean difference per channel (0-255) of a block in the region.
    pub strength: f32,
}

/// Mean absolute difference per block between two frames of the same size.
fn block_differences(a: &RgbaFrame, b: &RgbaFrame, columns: usize, rows: usize) -> Vec<f32> {
    let mut sums = vec![0f32; columns * rows];
    let mut counts = vec![0u32; columns * rows];
    let width = a.width as usize;
    for (i, (pa, pb)) in a
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.pixels.as_chunks::<4>().0)
        .enumerate()
    {
        let (x, y) = (i % width, i / width);
        let block =
            (y / BLOCK as usize).min(rows - 1) * columns + (x / BLOCK as usize).min(columns - 1);
        let difference: u32 = (0..3).map(|c| u32::from(pa[c].abs_diff(pb[c]))).sum();
        sums[block] += difference as f32 / 3.;
        counts[block] += 1;
    }
    sums.iter()
        .zip(counts)
        .map(|(sum, count)| sum / count.max(1) as f32)
        .collect()
}

/// Feeds consecutive frames one at a time and collects the pops between them. Keeps only the
/// last three frames and comparisons, so it scans long videos in constant memory.
pub struct PopDetector {
    columns: usize,
    rows: usize,
    size: (u32, u32),
    frames: Vec<RgbaFrame>,
    /// `differences[i]` compares `frames[i]` with `frames[i + 1]`.
    differences: Vec<Vec<f32>>,
    pushed: usize,
    pops: Vec<Pop>,
}

impl PopDetector {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            columns: width.div_ceil(BLOCK).max(1) as usize,
            rows: height.div_ceil(BLOCK).max(1) as usize,
            size: (width, height),
            frames: Vec::new(),
            differences: Vec::new(),
            pushed: 0,
            pops: Vec::new(),
        }
    }

    /// Adds the next frame of the sequence. Frames must all have the size given to `new`.
    pub fn push(&mut self, frame: RgbaFrame) {
        debug_assert_eq!((frame.width, frame.height), self.size);
        if let Some(previous) = self.frames.last() {
            let difference = block_differences(previous, &frame, self.columns, self.rows);
            self.differences.push(difference);
        }
        self.frames.push(frame);
        self.pushed += 1;
        // `frames` holds the last four frames; `differences` the three comparisons between them.
        if self.frames.len() > 4 {
            self.frames.remove(0);
            self.differences.remove(0);
        }
        self.detect();
    }

    /// All pops found, in the order of their frames.
    pub fn finish(self) -> Vec<Pop> {
        self.pops
    }

    /// Largest value in the block's neighbourhood (itself and the eight blocks around it).
    fn around(&self, values: &[f32], block: usize) -> f32 {
        let (column, row) = (
            (block % self.columns) as isize,
            (block / self.columns) as isize,
        );
        let mut largest = 0f32;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (c, r) = (column + dx, row + dy);
                if c >= 0 && r >= 0 && (c as usize) < self.columns && (r as usize) < self.rows {
                    largest = largest.max(values[r as usize * self.columns + c as usize]);
                }
            }
        }
        largest
    }

    fn detect(&mut self) {
        let blocks = self.columns * self.rows;
        let n = self.differences.len();
        // A step between frames k and k + 1 is decided once the comparison after it is known:
        // differences [before, change, after].
        if n >= 3 {
            let (before, change, after) = (
                &self.differences[n - 3],
                &self.differences[n - 2],
                &self.differences[n - 1],
            );
            let flagged: Vec<(usize, f32)> = (0..blocks)
                .filter(|&b| {
                    change[b] >= SUDDEN
                        && self.around(before, b) < STILL
                        && self.around(after, b) < STILL
                })
                .map(|b| (b, change[b]))
                .collect();
            self.report(&flagged, PopKind::Step, self.pushed - 2);
        }
        // A flash: frames [k, k + 1, k + 2, k + 3] with k + 1 different from k and k + 2 while
        // k and k + 2 are alike, and everything around still before and after.
        if n >= 3 && self.frames.len() >= 4 {
            let (before, into, out_of) = (
                &self.differences[n - 3],
                &self.differences[n - 2],
                &self.differences[n - 1],
            );
            let returned = block_differences(
                &self.frames[self.frames.len() - 3],
                &self.frames[self.frames.len() - 1],
                self.columns,
                self.rows,
            );
            let flagged: Vec<(usize, f32)> = (0..blocks)
                .filter(|&b| {
                    into[b] >= SUDDEN
                        && out_of[b] >= SUDDEN
                        && returned[b] < STILL
                        && self.around(before, b) < STILL
                })
                .map(|b| (b, into[b]))
                .collect();
            self.report(&flagged, PopKind::Flash, self.pushed - 2);
        }
    }

    /// Groups flagged `(block, strength)` pairs into connected regions; a change of most of the
    /// picture is a cut.
    fn report(&mut self, flagged: &[(usize, f32)], kind: PopKind, frame: usize) {
        if flagged.is_empty() || flagged.len() as f32 > CUT * (self.columns * self.rows) as f32 {
            return;
        }
        let columns_count = self.columns;
        let mut unvisited: Vec<(usize, f32)> = flagged.to_vec();
        while let Some(start) = unvisited.pop() {
            let mut region = vec![start];
            let mut i = 0;
            while i < region.len() {
                let block = region[i].0;
                let (column, row) = (block % columns_count, block / columns_count);
                unvisited.retain(|&(other, strength)| {
                    let (c, r) = (other % columns_count, other / columns_count);
                    let touches = c.abs_diff(column) <= 1 && r.abs_diff(row) <= 1;
                    if touches {
                        region.push((other, strength));
                    }
                    !touches
                });
                i += 1;
            }
            let columns = region.iter().map(|(b, _)| b % columns_count);
            let rows = region.iter().map(|(b, _)| b / columns_count);
            let (left, right) = (
                columns.clone().min().unwrap_or(0),
                columns.max().unwrap_or(0),
            );
            let (top, bottom) = (rows.clone().min().unwrap_or(0), rows.max().unwrap_or(0));
            let x = left as u32 * BLOCK;
            let y = top as u32 * BLOCK;
            self.pops.push(Pop {
                frame,
                kind,
                x,
                y,
                width: ((right + 1) as u32 * BLOCK).min(self.size.0) - x,
                height: ((bottom + 1) as u32 * BLOCK).min(self.size.1) - y,
                strength: region
                    .iter()
                    .map(|&(_, strength)| strength)
                    .fold(0., f32::max),
            });
        }
        self.pops.sort_by_key(|pop| (pop.frame, pop.y, pop.x));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 64;
    const H: u32 = 48;

    /// A grey frame with a white square at `square` (x, y, size), if any.
    fn frame(square: Option<(u32, u32, u32)>) -> RgbaFrame {
        let mut pixels = vec![80u8; (W * H * 4) as usize];
        if let Some((sx, sy, size)) = square {
            for y in sy..(sy + size).min(H) {
                for x in sx..(sx + size).min(W) {
                    let i = ((y * W + x) * 4) as usize;
                    pixels[i..i + 3].copy_from_slice(&[255, 255, 255]);
                }
            }
        }
        RgbaFrame {
            width: W,
            height: H,
            pixels,
        }
    }

    fn scan(frames: Vec<RgbaFrame>) -> Vec<Pop> {
        let mut detector = PopDetector::new(W, H);
        for frame in frames {
            detector.push(frame);
        }
        detector.finish()
    }

    #[test]
    fn element_appearing_between_still_frames_is_a_step() {
        let pops = scan(vec![
            frame(None),
            frame(None),
            frame(Some((16, 16, 8))),
            frame(Some((16, 16, 8))),
            frame(Some((16, 16, 8))),
        ]);
        assert_eq!(pops.len(), 1, "{pops:?}");
        let pop = &pops[0];
        assert_eq!(pop.kind, PopKind::Step);
        assert_eq!(pop.frame, 2);
        assert_eq!((pop.x, pop.y, pop.width, pop.height), (16, 16, 8, 8));
    }

    #[test]
    fn single_frame_glitch_is_a_flash() {
        let pops = scan(vec![
            frame(None),
            frame(None),
            frame(Some((40, 8, 8))),
            frame(None),
            frame(None),
        ]);
        assert!(
            pops.iter()
                .any(|p| p.kind == PopKind::Flash && p.frame == 2 && p.x == 40),
            "{pops:?}"
        );
    }

    #[test]
    fn continuous_motion_is_not_a_pop() {
        let pops = scan((0..8).map(|i| frame(Some((4 + i * 6, 16, 8)))).collect());
        assert!(pops.is_empty(), "{pops:?}");
    }

    #[test]
    fn cut_of_the_whole_picture_is_not_a_pop() {
        let full = Some((0, 0, W.max(H)));
        let pops = scan(vec![
            frame(None),
            frame(None),
            frame(full),
            frame(full),
            frame(full),
        ]);
        assert!(pops.is_empty(), "{pops:?}");
    }

    #[test]
    fn still_video_has_no_pops() {
        assert_eq!(scan(vec![frame(None); 6]), Vec::new());
    }
}
