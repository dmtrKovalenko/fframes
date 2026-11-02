//! Prepare caption scenes once; frame rendering only reads the prepared components.
use crate::{editorial::EditorialCaption, word_effects::WordEffects};
use fframes::captions::{CaptionCue, CaptionError, CaptionRect, CaptionTrack, Captions};
use fframes::{FFramesContext, Frame, Svgr};

enum Composition {
    Authored(Box<EditorialCaption>),
    Plain(Box<Captions>),
}

pub struct DirectedCaption {
    start: f64,
    end: f64,
    composition: Composition,
}

impl DirectedCaption {
    pub fn new(cue: CaptionCue) -> Result<Self, CaptionError> {
        let start = cue.start;
        let end = cue.end;
        let composition = if cue.metadata.contains_key("editorial") {
            Composition::Authored(Box::new(EditorialCaption::new(cue)?))
        } else {
            if !cue.metadata.is_empty() || cue.words.iter().any(|word| !word.metadata.is_empty()) {
                return Err(CaptionError::invalid(
                    "metadata",
                    "authored Moon captions require cue.metadata.editorial and word metadata matching WordStyle",
                ));
            }
            Composition::Plain(Box::new(
                Captions::new(
                    CaptionTrack::new([cue])?,
                    CaptionRect::new(160.0, 300.0, 1600.0, 600.0),
                )
                .clock(fframes::captions::CaptionClock::Global)
                .font("Barlow Condensed", 110)
                .font_weight(600)
                .outline("#080c15", 2.0)
                .active_fill("#ffd166"),
            ))
        };
        Ok(Self {
            start,
            end,
            composition,
        })
    }

    pub fn active_at(&self, seconds: f64) -> bool {
        self.start <= seconds && seconds < self.end
    }

    pub fn render<'a>(
        &'a self,
        frame: &mut Frame,
        ctx: &FFramesContext<'a, '_>,
        effects: &'a WordEffects,
        effect_override: Option<usize>,
    ) -> Svgr<'a> {
        match &self.composition {
            Composition::Authored(caption) => caption.render(frame, ctx, effects, effect_override),
            Composition::Plain(captions) => captions.render(frame, ctx),
        }
    }
}
