//! Per-exposure vector contours, measured at the reference's native cadence.
//! Soft ink is a stack of translucent outlines, not an image or video sample.

use std::io::Read;

use fframes::{Frame, Svgr, Transform, animation::Easing};
use flate2::read::GzDecoder;

use crate::{flight::Letter, ink_capture};

const PIGMENTS: [&str; 11] = [
    "#0a0907", "#40220f", "#c91c14", "#fffbea", "#ff2414", "#ffd155", "#c91c14", "#c91c14",
    "#c91c14", "#c91c14", "#b91613",
];

#[derive(Debug)]
pub(super) struct VectorInk {
    frames: Vec<Svgr<'static>>,
    letter_marks: Vec<[Svgr<'static>; 4]>,
}

impl VectorInk {
    pub(super) fn new() -> Self {
        let mut frames: Vec<Vec<Svgr<'static>>> = (0..489).map(|_| Vec::new()).collect();
        let mut marks: Vec<[Vec<Svgr<'static>>; 4]> = (0..489)
            .map(|_| std::array::from_fn(|_| Vec::new()))
            .collect();
        // Decompress the vector text once; no source pixels are embedded.
        // Frame rendering only clones the prepared SVG tree.
        let mut source = String::new();
        for chunk in [
            include_bytes!("vector_ink/000.paths.gz").as_slice(),
            include_bytes!("vector_ink/100.paths.gz").as_slice(),
            include_bytes!("vector_ink/200.paths.gz").as_slice(),
            include_bytes!("vector_ink/300.paths.gz").as_slice(),
            include_bytes!("vector_ink/400.paths.gz").as_slice(),
        ] {
            GzDecoder::new(chunk)
                .read_to_string(&mut source)
                .expect("the generated vector ink archive must contain valid UTF-8 paths");
        }
        for line in source.lines() {
            let mut fields = line.splitn(4, '|');
            let Some(index) = fields.next().and_then(|value| value.parse::<usize>().ok()) else {
                continue;
            };
            let Some(pigment) = fields.next().and_then(|value| value.parse::<usize>().ok()) else {
                continue;
            };
            let Some(color) = PIGMENTS.get(pigment) else {
                continue;
            };
            let Some(opacity) = fields.next().and_then(|value| value.parse::<f32>().ok()) else {
                continue;
            };
            let (Some(path), Some(frame)) = (fields.next(), frames.get_mut(index)) else {
                continue;
            };
            let path = fframes::svgr!(<path d={path.to_owned()} fill={*color} fill-rule="evenodd" opacity={opacity}/>);
            if (6..10).contains(&pigment) {
                marks[index][pigment - 6].push(path);
            } else {
                frame.push(path);
            }
        }
        Self {
            frames: frames.into_iter().map(Svgr::from).collect(),
            letter_marks: marks
                .into_iter()
                .map(|frame| frame.map(Svgr::from))
                .collect(),
        }
    }

    pub(super) fn draw(&self, frame: &Frame) -> Svgr<'static> {
        let base = self
            .frames
            .get(frame.global_index)
            .cloned()
            .unwrap_or_default();
        let marks = self
            .letter_marks
            .get(frame.global_index)
            .cloned()
            .unwrap_or_default();
        fframes::svgr!(<g>{base}{marks.into_iter().collect::<Vec<_>>()}</g>)
    }

    /// Preserve source nib shapes and exposure timing through the seven brand
    /// flights and final camera pullback. `draw` remains the unmodified trace.
    pub(super) fn draw_signature(&self, frame: &Frame, letters: &[Letter; 7]) -> Svgr<'static> {
        let n = frame.global_index.min(488);
        let source =
            ink_capture::LETTERS[(n.saturating_sub(413)).min(ink_capture::LETTERS.len() - 1)];
        let marks = letters
            .iter()
            .enumerate()
            .map(|(i, letter)| {
                let [sx, sy] = source[i % 4];
                let [x, y] = letter.position;
                let shape = self.letter_marks[n][i % 4].clone();
                fframes::svgr!(<g transform={Transform::translate(x, y)}>
                    <g transform={frame.animate(fframes::timeline!(
                        at 2.541_667 => 2.666_667,
                        animate Transform::scale(1.) => Transform::scale(2.8), Easing::EaseIn,
                    ))}>
                        <g transform={Transform::translate(-sx, -sy)}>{shape}</g>
                    </g>
                </g>)
            })
            .collect::<Vec<_>>();
        // The reference's longest, strongest red gestures are its last twelve
        // frames. Keep them after assembly, attached to the same camera as the
        // wordmark; only the independent black gestures leave the composition.
        fframes::svgr!(<g>
            <g opacity={frame.animate(fframes::timeline!(
                at 2.541_667 => 2.708_333, animate 1.0_f32 => 0.0, Easing::EaseOut,
            ))}>{self.frames[n].clone()}</g>
            <g transform="translate(720 540)">
                <g transform={frame.animate(fframes::timeline!(
                    at 2.666_667 => 2.833_333,
                    animate Transform::scale(1.) => Transform::scale(1. / 2.8),
                    Easing::CubicBezier(0.12,0.92,0.20,1.),
                ))}>
                    <g transform="translate(-720 -540)">{marks}</g>
                </g>
            </g>
        </g>)
    }
}
