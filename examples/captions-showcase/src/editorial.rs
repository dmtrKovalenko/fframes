//! Persistent phrases, optical materials and glyph motion directed by subtitle metadata.
//! Font geometry comes from Captions; application metadata is lazily decoded and cached there.
use std::sync::OnceLock;

use fframes::{
    EstimateTextWidthOptions, FFramesContext, FontStretch, FontStyle, Frame, Svgr,
    animation::{AnimationRuntime, Easing},
    captions::{
        CaptionCue, CaptionError, CaptionMode, CaptionRect, CaptionTrack, CaptionWordScope,
        Captions, WordState,
    },
    estimate_text_width, svgr,
};
use serde::Deserialize;

#[path = "choreography.rs"]
mod choreography;
pub use choreography::{Echo, GlyphMotion, Replica};

#[path = "cue_graphics.rs"]
mod cue_graphics;

#[path = "word_plane.rs"]
mod word_plane;
pub use word_plane::PlaneMotion;

use crate::{
    annotations::{self, Accent, AccentKind},
    word_effects::{
        GlassNebulaTuning, GooTuning, IrradianceTuning, LiquidTuning, MaterialControls,
        ReliefTuning, WordDirection, WordEffects,
    },
};

fn scene_enter() -> f64 {
    0.16
}
fn scene_exit() -> f64 {
    0.15
}
fn word_enter() -> f64 {
    0.18
}
fn family() -> String {
    "Barlow Condensed".into()
}
fn weight() -> u16 {
    600
}
fn paper() -> String {
    "#f3eee3".into()
}
fn ink() -> String {
    "#080c15".into()
}
fn outline_width() -> f64 {
    1.2
}

/// Cue-level application data. Extra top-level keys remain available to other consumers.
#[derive(Debug, Clone, Deserialize)]
pub struct CueStyle {
    pub editorial: SceneStyle,
    /// Optional composition endpoint. Its containing cue must be the final cue;
    /// that cue's end supplies the duration, without another copy of the clock.
    #[serde(default)]
    pub output: Option<OutputFades>,
}

/// Fades applied at an explicitly authored composition end. Omitted fades mean
/// a cut; tracks without an output marker retain the application's old policy.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OutputFades {
    pub picture_fade_seconds: f64,
    pub voice_fade_seconds: f64,
    pub music_fade_seconds: f64,
}

/// Validated output settings resolved once from the loaded caption metadata.
#[derive(Debug, Clone, Copy)]
pub struct OutputPolicy {
    pub end_seconds: f64,
    pub picture_fade_seconds: f64,
    pub voice_fade_seconds: f64,
    pub music_fade_seconds: f64,
}

impl OutputFades {
    fn resolve(self, end_seconds: f64, path: &str) -> Result<OutputPolicy, CaptionError> {
        for (name, value) in [
            ("picture_fade_seconds", self.picture_fade_seconds),
            ("voice_fade_seconds", self.voice_fade_seconds),
            ("music_fade_seconds", self.music_fade_seconds),
        ] {
            range(value, 0.0, end_seconds, &format!("{path}.{name}"))?;
        }
        Ok(OutputPolicy {
            end_seconds,
            picture_fade_seconds: self.picture_fade_seconds,
            voice_fade_seconds: self.voice_fade_seconds,
            music_fade_seconds: self.music_fade_seconds,
        })
    }
}

/// Resolve an optional output marker through the same cached cue metadata used
/// for drawing. No file is reopened and the subtitle JSON is never reparsed.
pub fn output_policy(track: &CaptionTrack) -> Result<Option<OutputPolicy>, CaptionError> {
    let mut selected = None;
    for (index, cue) in track.cues().iter().enumerate() {
        if cue
            .metadata
            .get("output")
            .is_none_or(serde_json::Value::is_null)
        {
            continue;
        }
        let path = format!("cues[{index}].metadata.output");
        if selected.is_some() {
            return Err(CaptionError::invalid(
                path,
                "only one cue may define output settings",
            ));
        }
        let fades = cue
            .metadata
            .typed::<CueStyle>()
            .and_then(|style| style.output)
            .ok_or_else(|| {
                CaptionError::invalid(
                    &path,
                    "provide CueStyle metadata with valid output fade settings",
                )
            })?;
        selected = Some((index, fades));
    }
    let Some((index, fades)) = selected else {
        return Ok(None);
    };
    let path = format!("cues[{index}].metadata.output");
    if index + 1 != track.cues().len() {
        return Err(CaptionError::invalid(
            path,
            "output settings must belong to the final cue",
        ));
    }
    let end_seconds = track.cues()[index].end;
    if track.cues().iter().any(|cue| cue.end > end_seconds) {
        return Err(CaptionError::invalid(
            path,
            "the output end must not truncate an overlapping cue",
        ));
    }
    fades.resolve(end_seconds, &path).map(Some)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneStyle {
    #[serde(default = "scene_enter")]
    pub enter_seconds: f64,
    #[serde(default = "scene_exit")]
    pub exit_seconds: f64,
    #[serde(default)]
    pub graphics: Vec<cue_graphics::Graphic>,
}

/// Word-level application data, decoded lazily once per metadata object.
#[derive(Debug, Clone, Deserialize)]
pub struct WordStyle {
    pub layout: WordLayout,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub motion: Motion,
    #[serde(default)]
    pub accents: Vec<Accent>,
    #[serde(default)]
    pub material: Option<Material>,
    #[serde(default)]
    pub replicas: Vec<Replica>,
    #[serde(default)]
    pub plane: Option<PlaneMotion>,
}

/// Application-level material names. The subtitle parser does not interpret them.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MaterialEffect {
    Plasma,
    Nebula,
    Diffraction,
    Etch,
    Interference,
    Discharge,
    Shear,
    Mercury,
    Fracture,
    ScanGlass,
    Engraving,
    Wavefront,
    Quartz,
    Crystal,
    ThinFilm,
    Horizon,
    Astral,
    Combustion,
    Fluid,
    Raster,
    Contour,
    Prism,
    Lattice,
    Flux,
    Stress,
    Lamella,
    Photonic,
    LiquidMetal,
    Irradiance,
    Goo,
    GlassNebula,
}

impl MaterialEffect {
    pub fn index(self) -> usize {
        self as usize
    }
}

fn material_variation() -> f32 {
    0.5
}
fn material_hero() -> bool {
    true
}
fn material_exit() -> f64 {
    0.18
}
fn material_rest() -> f64 {
    0.25
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub effect: MaterialEffect,
    /// Optional art controls for the geometric lamella/photonic materials.
    #[serde(default)]
    pub relief: Option<ReliefTuning>,
    /// Optional controls ported from shaders.com's `LiquidMetal` material.
    #[serde(default)]
    pub liquid: Option<LiquidTuning>,
    /// Optional controls for shaders.com's edge-light transport material.
    #[serde(default)]
    pub irradiance: Option<IrradianceTuning>,
    /// Optional controls for shaders.com's thick translucent Goo material.
    #[serde(default)]
    pub goo: Option<GooTuning>,
    /// Optional controls for shaders.com's glass-enclosed Nebula material.
    #[serde(default)]
    pub glass_nebula: Option<GlassNebulaTuning>,
    #[serde(default = "material_variation")]
    pub variation: f32,
    #[serde(default = "material_hero")]
    pub hero: bool,
    #[serde(default)]
    pub warm: bool,
    /// Absolute visual release time; omitted means the end of the cue.
    #[serde(default)]
    pub hold_until: Option<f64>,
    #[serde(default = "material_exit")]
    pub exit_seconds: f64,
    /// Return the highlighted word to ordinary text while its phrase stays visible.
    #[serde(default)]
    pub rest_at: Option<f64>,
    #[serde(default = "material_rest")]
    pub rest_seconds: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordLayout {
    pub font_size: usize,
    pub center_x: f64,
    pub baseline: f64,
    #[serde(default)]
    pub angle: f64,
    #[serde(default = "family")]
    pub font_family: String,
    #[serde(default = "weight")]
    pub font_weight: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    pub fill: String,
    pub outline: String,
    pub outline_width: f64,
    pub outline_before: bool,
    pub shadow_blur: f64,
    pub shadow_opacity: f64,
    pub background: Option<String>,
    pub background_padding: f64,
    pub background_radius: f64,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            fill: paper(),
            outline: ink(),
            outline_width: outline_width(),
            outline_before: false,
            shadow_blur: 4.0,
            shadow_opacity: 0.8,
            background: None,
            background_padding: 12.0,
            background_radius: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Reveal {
    Cue,
    #[default]
    Word,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Entrance {
    None,
    #[default]
    Rise,
    Stamp,
    Wipe,
    SlideLeft,
    SlideRight,
    Aperture,
    Arc,
    GlyphHop,
    GlyphAssemble,
    Tracking,
    Flip,
    Choreograph,
}

impl Entrance {
    fn uses_glyphs(self) -> bool {
        matches!(
            self,
            Self::GlyphHop | Self::GlyphAssemble | Self::Tracking | Self::Flip | Self::Choreograph
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Motion {
    pub reveal: Reveal,
    pub entrance: Entrance,
    pub enter_seconds: f64,
    /// Delay between adjacent glyphs, independent of spoken word timing.
    pub stagger_seconds: f64,
    /// Maximum displacement in pixels, or total spread for tracking.
    pub amplitude: f64,
    /// Optional absolute visual start, without changing the spoken timestamp.
    pub start: Option<f64>,
    /// Reverse the order of the character stagger without reversing text.
    pub reverse: bool,
    /// Explicit character paths relative to the measured resting positions.
    pub choreography: Vec<GlyphMotion>,
    /// Filled samples of the same authored paths at future instants.
    pub echoes: Vec<Echo>,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            reveal: Reveal::Word,
            entrance: Entrance::Rise,
            enter_seconds: word_enter(),
            stagger_seconds: 0.025,
            amplitude: 32.0,
            start: None,
            reverse: false,
            choreography: Vec::new(),
            echoes: Vec::new(),
        }
    }
}

struct EditorialWord {
    captions: Captions,
    starts: f64,
    entrance: AnimationRuntime,
    glyphs: Vec<String>,
    glyph_layout: OnceLock<Vec<GlyphPlacement>>,
    plane_clearance: OnceLock<f64>,
    choreography: choreography::Choreography,
    plane: word_plane::PlaneRenderer,
}

struct GlyphPlacement {
    x: f64,
    width: f64,
}

pub struct EditorialCaption {
    cue: CaptionCue,
    scene: SceneStyle,
    words: Vec<EditorialWord>,
    exit: AnimationRuntime,
    graphics: cue_graphics::PreparedGraphics,
}

fn range(value: f64, min: f64, max: f64, path: &str) -> Result<(), CaptionError> {
    if !value.is_finite() || !(min..=max).contains(&value) {
        return Err(CaptionError::invalid(
            path,
            format!("must be finite and between {min} and {max}"),
        ));
    }
    Ok(())
}

impl EditorialCaption {
    pub fn new(cue: CaptionCue) -> Result<Self, CaptionError> {
        let path = format!("cue[{}]", cue.id.as_deref().unwrap_or("unnamed"));
        let scene = cue
            .metadata
            .typed::<CueStyle>()
            .ok_or_else(|| {
                CaptionError::invalid(
                    format!("{path}.metadata.editorial"),
                    "provide CueStyle metadata with valid editorial settings",
                )
            })?
            .editorial
            .clone();
        range(
            scene.enter_seconds,
            0.01,
            2.0,
            &format!("{path}.metadata.editorial.enter_seconds"),
        )?;
        range(
            scene.exit_seconds,
            0.01,
            2.0,
            &format!("{path}.metadata.editorial.exit_seconds"),
        )?;
        if cue.words.is_empty() {
            return Err(CaptionError::invalid(
                format!("{path}.words"),
                "editorial composition requires timed words",
            ));
        }
        let mut words = Vec::with_capacity(cue.words.len());
        for (index, word) in cue.words.iter().enumerate() {
            let path = format!("{path}.words[{index}].metadata");
            let style = word.metadata.typed::<WordStyle>().ok_or_else(|| {
                CaptionError::invalid(&path,
                    "provide WordStyle metadata with layout, appearance, motion and accents with valid types")
            })?;
            if !(16..=700).contains(&style.layout.font_size) {
                return Err(CaptionError::invalid(
                    format!("{path}.layout.font_size"),
                    "must be between 16 and 700 pixels",
                ));
            }
            range(
                style.layout.center_x,
                0.0,
                1920.0,
                &format!("{path}.layout.center_x"),
            )?;
            range(
                style.layout.baseline,
                0.0,
                1080.0,
                &format!("{path}.layout.baseline"),
            )?;
            range(
                style.layout.angle,
                -15.0,
                15.0,
                &format!("{path}.layout.angle"),
            )?;
            if style.layout.font_family.trim().is_empty()
                || !(100..=900).contains(&style.layout.font_weight)
            {
                return Err(CaptionError::invalid(
                    format!("{path}.layout"),
                    "font family must be nonempty and weight must be between 100 and 900",
                ));
            }
            range(
                style.motion.enter_seconds,
                0.01,
                2.0,
                &format!("{path}.motion.enter_seconds"),
            )?;
            range(
                style.motion.stagger_seconds,
                0.0,
                0.2,
                &format!("{path}.motion.stagger_seconds"),
            )?;
            range(
                style.motion.amplitude,
                0.0,
                400.0,
                &format!("{path}.motion.amplitude"),
            )?;
            if let Some(start) = style.motion.start {
                range(start, cue.start, cue.end, &format!("{path}.motion.start"))?;
            }
            if !style.motion.choreography.is_empty()
                && style.motion.entrance != Entrance::Choreograph
            {
                return Err(CaptionError::invalid(
                    format!("{path}.motion.choreography"),
                    "explicit glyph paths require entrance: choreograph",
                ));
            }
            if style.motion.entrance == Entrance::Choreograph
                && style.motion.choreography.is_empty()
            {
                return Err(CaptionError::invalid(
                    format!("{path}.motion.choreography"),
                    "entrance: choreograph needs at least one explicit character path",
                ));
            }
            let choreography = choreography::Choreography::prepare(
                &style.motion.choreography,
                word.text.chars().count(),
                &format!("{path}.motion.choreography"),
            )?;
            choreography::validate_echoes(&style.motion.echoes, &format!("{path}.motion.echoes"))?;
            if !style.motion.echoes.is_empty() && style.motion.entrance != Entrance::Choreograph {
                return Err(CaptionError::invalid(
                    format!("{path}.motion.echoes"),
                    "future echoes require explicit glyph choreography",
                ));
            }
            choreography::validate_replicas(
                &style.replicas,
                cue.start,
                cue.end,
                &format!("{path}.replicas"),
            )?;
            if let Some(plane) = &style.plane {
                plane.validate(cue.start, cue.end, &format!("{path}.plane"))?;
                if style.material.is_some() || style.motion.entrance.uses_glyphs() {
                    return Err(CaptionError::invalid(
                        format!("{path}.plane"),
                        "projective planes require a plain whole-word caption; material shaders and per-glyph entrances are separate techniques",
                    ));
                }
                if style.appearance.outline_before {
                    return Err(CaptionError::invalid(
                        format!("{path}.appearance.outline_before"),
                        "a projective plane captures one stable glyph appearance; use a filled caption throughout its cue",
                    ));
                }
            }
            if let Some(material) = &style.material {
                if let Some(relief) = material.relief {
                    if !matches!(
                        material.effect,
                        MaterialEffect::Lamella | MaterialEffect::Photonic
                    ) {
                        return Err(CaptionError::invalid(
                            format!("{path}.material.relief"),
                            "relief controls require lamella or photonic",
                        ));
                    }
                    relief.validate().map_err(|message| {
                        CaptionError::invalid(format!("{path}.material.relief"), message)
                    })?;
                }
                if let Some(liquid) = material.liquid {
                    if !matches!(material.effect, MaterialEffect::LiquidMetal) {
                        return Err(CaptionError::invalid(
                            format!("{path}.material.liquid"),
                            "liquid controls require liquid-metal",
                        ));
                    }
                    liquid.validate().map_err(|message| {
                        CaptionError::invalid(format!("{path}.material.liquid"), message)
                    })?;
                }
                if let Some(tuning) = material.irradiance {
                    if !matches!(material.effect, MaterialEffect::Irradiance) {
                        return Err(CaptionError::invalid(
                            format!("{path}.material.irradiance"),
                            "irradiance controls require irradiance",
                        ));
                    }
                    tuning.validate().map_err(|message| {
                        CaptionError::invalid(format!("{path}.material.irradiance"), message)
                    })?;
                }
                if let Some(tuning) = material.goo {
                    if !matches!(material.effect, MaterialEffect::Goo) {
                        return Err(CaptionError::invalid(
                            format!("{path}.material.goo"),
                            "goo controls require goo",
                        ));
                    }
                    tuning.validate().map_err(|message| {
                        CaptionError::invalid(format!("{path}.material.goo"), message)
                    })?;
                }
                if let Some(tuning) = material.glass_nebula {
                    if !matches!(material.effect, MaterialEffect::GlassNebula) {
                        return Err(CaptionError::invalid(
                            format!("{path}.material.glass_nebula"),
                            "glass_nebula controls require glass-nebula",
                        ));
                    }
                    tuning.validate().map_err(|message| {
                        CaptionError::invalid(format!("{path}.material.glass_nebula"), message)
                    })?;
                }
                range(
                    f64::from(material.variation),
                    0.0,
                    1.0,
                    &format!("{path}.material.variation"),
                )?;
                range(
                    material.exit_seconds,
                    0.01,
                    2.0,
                    &format!("{path}.material.exit_seconds"),
                )?;
                range(
                    material.rest_seconds,
                    0.01,
                    2.0,
                    &format!("{path}.material.rest_seconds"),
                )?;
                if let Some(rest_at) = material.rest_at {
                    range(
                        rest_at,
                        word.end,
                        cue.end,
                        &format!("{path}.material.rest_at"),
                    )?;
                }
                if let Some(until) = material.hold_until {
                    range(
                        until,
                        word.end,
                        cue.end,
                        &format!("{path}.material.hold_until"),
                    )?;
                }
                if style.motion.entrance.uses_glyphs() {
                    return Err(CaptionError::invalid(
                        format!("{path}.motion.entrance"),
                        "this example's glyph entrances require plain text; use rise, stamp, wipe, slide-left or slide-right with a material",
                    ));
                }
            }
            range(
                style.appearance.outline_width,
                0.0,
                20.0,
                &format!("{path}.appearance.outline_width"),
            )?;
            range(
                style.appearance.shadow_blur,
                0.0,
                20.0,
                &format!("{path}.appearance.shadow_blur"),
            )?;
            range(
                style.appearance.shadow_opacity,
                0.0,
                1.0,
                &format!("{path}.appearance.shadow_opacity"),
            )?;
            range(
                style.appearance.background_padding,
                0.0,
                100.0,
                &format!("{path}.appearance.background_padding"),
            )?;
            range(
                style.appearance.background_radius,
                0.0,
                100.0,
                &format!("{path}.appearance.background_radius"),
            )?;
            for (index, accent) in style.accents.iter().enumerate() {
                let accent_path = format!("{path}.accents[{index}]");
                range(
                    accent.start,
                    cue.start,
                    cue.end,
                    &format!("{accent_path}.start"),
                )?;
                range(
                    accent.duration,
                    0.01,
                    3.0,
                    &format!("{accent_path}.duration"),
                )?;
                range(accent.width, 0.1, 30.0, &format!("{accent_path}.width"))?;
                range(accent.padding, 0.0, 90.0, &format!("{accent_path}.padding"))?;
            }
            let starts = style.motion.start.unwrap_or(match style.motion.reveal {
                Reveal::Cue => cue.start,
                Reveal::Word => word.start,
            });
            let enter_seconds = if style.motion.reveal == Reveal::Cue
                && style.motion.start.is_none()
                && !style.motion.entrance.uses_glyphs()
            {
                scene.enter_seconds
            } else {
                style.motion.enter_seconds
            };
            let word_cue = CaptionCue::words(cue.start, cue.end, [word.clone()])
                .with_metadata(cue.metadata.clone());
            let captions = Captions::new(
                CaptionTrack::new([word_cue])?,
                CaptionRect::new(30.0, 0.0, 1860.0, 1080.0),
            )
            .clock(fframes::captions::CaptionClock::Global)
            .font(style.layout.font_family.clone(), style.layout.font_size)
            .font_weight(style.layout.font_weight)
            .line_height(1.0)
            .mode(CaptionMode::Highlight)
            .outline(
                style.appearance.outline.clone(),
                style.appearance.outline_width,
            )
            .upcoming_fill(if style.appearance.outline_before {
                "none".to_owned()
            } else {
                style.appearance.fill.clone()
            })
            .active_fill(style.appearance.fill.clone())
            .spoken_fill(style.appearance.fill.clone());
            words.push(EditorialWord {
                captions,
                starts,
                entrance: AnimationRuntime::new(enter_seconds as f32, &Easing::EaseOut),
                glyphs: word.text.chars().map(|glyph| glyph.to_string()).collect(),
                glyph_layout: OnceLock::new(),
                plane_clearance: OnceLock::new(),
                choreography,
                plane: word_plane::PlaneRenderer::new(),
            });
        }
        let exit = AnimationRuntime::new(scene.exit_seconds as f32, &Easing::EaseIn);
        let graphics = cue_graphics::PreparedGraphics::new(&scene.graphics, cue.start, cue.end)?;
        Ok(Self {
            cue,
            scene,
            words,
            exit,
            graphics,
        })
    }

    pub fn render<'a>(
        &'a self,
        frame: &mut Frame,
        ctx: &FFramesContext<'a, '_>,
        effects: &'a WordEffects,
        effect_override: Option<usize>,
    ) -> Svgr<'a> {
        let time = frame.global_index as f64 / frame.fps as f64;
        let opacity = 1.0 - ease(&self.exit, time - (self.cue.end - self.scene.exit_seconds));
        let words: Svgr<'a> = self.words.iter().filter(|word| time >= word.starts).map(|word| {
            let progress = ease(&word.entrance, time - word.starts);
            word.captions.render_with(frame, ctx, |scope, text| {
                // Constructor validation initializes this shared cache once. Rendering
                // only borrows it, including when frames are evaluated out of order.
                let Some(style) = scope.word.metadata.typed::<WordStyle>() else { return text; };
                let width = scope.width();
                let height = scope.height();
                let (shift, rise, scale_x, scale_y) = match style.motion.entrance {
                    Entrance::Rise => (0.0, 16.0 * (1.0 - progress), 1.0, 1.0),
                    Entrance::Stamp => (0.0, 0.0, 1.0 + 0.08 * (1.0 - progress), 1.0 + 0.08 * (1.0 - progress)),
                    Entrance::SlideLeft => (-style.motion.amplitude * (1.0 - progress), 0.0, 1.0, 1.0),
                    Entrance::SlideRight => (style.motion.amplitude * (1.0 - progress), 0.0, 1.0, 1.0),
                    Entrance::Aperture => (0.0, 0.0, 1.0 - 0.12 * (1.0 - progress), 1.0 + 0.08 * (1.0 - progress)),
                    Entrance::Arc => {
                        let linear = ((time - word.starts) / f64::from(word.entrance.get_duration())).clamp(0.0, 1.0);
                        (-style.motion.amplitude * (1.0 - progress),
                            -style.motion.amplitude * (std::f64::consts::PI * linear).sin(), 1.0, 1.0)
                    }
                    _ => (0.0, 0.0, 1.0, 1.0),
                };
                let dx = style.layout.center_x - width * 0.5 - scope.composition_rect.x + shift;
                let dy = style.layout.baseline - style.layout.font_size as f64 + rise - scope.composition_rect.y;
                let cx = width * 0.5;
                let cy = height * 0.5;
                let underlay = style.accents.iter().filter(|accent| accent.kind == AccentKind::Marker)
                    .map(|accent| annotations::render(scope, accent, time)).collect::<Vec<_>>();
                let overlay = style.accents.iter().filter(|accent| accent.kind != AccentKind::Marker)
                    .map(|accent| annotations::render(scope, accent, time)).collect::<Vec<_>>();
                let replicas = choreography::replicas(scope, &style.replicas, time);
                let background = style.appearance.background.as_ref().map(|fill| {
                    let padding = style.appearance.background_padding;
                    svgr!(<rect x={-padding} y={-padding}
                        width={width + 2.0 * padding} height={height + 2.0 * padding}
                        rx={style.appearance.background_radius} fill={fill.as_str()} />)
                }).unwrap_or_default();
                let wipe_id = (style.motion.entrance == Entrance::Wipe && progress < 1.0)
                    .then(|| scope.id("editorial-wipe"));
                let shadow_id = (style.appearance.shadow_blur > 0.0 && style.appearance.shadow_opacity > 0.0)
                    .then(|| scope.id("editorial-shadow"));
                // Optical effects reserve 72px outside the glyph texture. Include
                // that region plus three shadow deviations so flames and stars
                // are not clipped by an otherwise ordinary word shadow filter.
                let effect_padding = if style.material.is_some() || effect_override.is_some() { 72.0 } else { 0.0 };
                let motion_padding = word.choreography.shadow_clearance(time - word.starts, style.layout.font_size as f64);
                let plane_padding = style.plane.as_ref().map_or(0.0, |plane| {
                    *word.plane_clearance.get_or_init(|| plane.shadow_clearance(width, height))
                });
                let shadow_padding = (effect_padding + motion_padding + plane_padding + 3.0 * style.appearance.shadow_blur
                    + style.appearance.outline_width + 4.0).max(40.0);
                let material = effect_override.map(|effect| (effect, WordDirection {
                    hold_until: Some(self.cue.end), exit_seconds: self.scene.exit_seconds,
                    hero: true, warm: false, variation: 0.5,
                })).or_else(|| style.material.as_ref().map(|material| (
                    material.effect.index(), WordDirection {
                        hold_until: Some(material.hold_until.unwrap_or(self.cue.end)),
                        exit_seconds: material.exit_seconds, hero: material.hero,
                        warm: material.warm, variation: material.variation,
                    }
                )));
                let glyphs = (material.is_none() && style.motion.entrance.uses_glyphs())
                    .then(|| word.render_glyphs(scope, ctx, style, time))
                    .flatten();
                let rest = if effect_override.is_none() {
                    style.material.as_ref().and_then(|material| material.rest_at.map(|at|
                        ((time - at) / material.rest_seconds).clamp(0.0, 1.0)
                    )).unwrap_or(0.0)
                } else { 0.0 };
                let content = if effect_override.is_none() && let Some(plane) = &style.plane {
                    word.plane.render(scope, &text, ctx, plane, time, effects.shaders_enabled())
                } else if rest < 1.0 && scope.word.state != WordState::Upcoming
                    && let Some((effect, direction)) = material {
                    let plain = text.clone();
                    let relief = style.material.as_ref().and_then(|material| material.relief).unwrap_or_default();
                    let liquid = style.material.as_ref().and_then(|material| material.liquid).unwrap_or_default();
                    let irradiance = style.material.as_ref().and_then(|material| material.irradiance).unwrap_or_default();
                    let goo = style.material.as_ref().and_then(|material| material.goo).unwrap_or_default();
                    let glass_nebula = style.material.as_ref().and_then(|material| material.glass_nebula).unwrap_or_default();
                    let highlighted = effects.render_directed_with_controls(scope, text, ctx, effect, direction, &MaterialControls { relief, liquid, irradiance, goo, glass_nebula });
                    if rest > 0.0 {
                        let blend = rest * rest * (3.0 - 2.0 * rest);
                        svgr!(<g>
                            <g opacity={1.0 - blend}>{highlighted}</g>
                            <g opacity={blend}>{plain}</g>
                        </g>)
                    } else { highlighted }
                } else {
                    // A whole phrase may enter before it is spoken. Keep those words
                    // readable; the material starts on their original speech time.
                    glyphs.unwrap_or(text)
                };
                let content = if let Some(id) = &shadow_id {
                    svgr!(<g>
                        <defs><filter id={id.clone()} filterUnits="userSpaceOnUse"
                            primitiveUnits="userSpaceOnUse" x={-shadow_padding} y={-shadow_padding}
                            width={width + 2.0 * shadow_padding} height={height + 2.0 * shadow_padding}>
                            <feDropShadow dx="0" dy="3" stdDeviation={style.appearance.shadow_blur}
                                flood-color="#000000" flood-opacity={style.appearance.shadow_opacity} />
                        </filter></defs>
                        <g filter={format!("url(#{id})")}>{content}</g>
                    </g>)
                } else { content };
                let local = if let Some(id) = wipe_id {
                    let (pad_x, pad_y) = if shadow_id.is_some() { (shadow_padding, shadow_padding) } else { (16.0, 30.0) };
                    let revealed = (width + 2.0 * pad_x) * (0.12 + 0.88 * progress);
                    svgr!(<g>
                        <defs><clipPath id={id.clone()} clipPathUnits="userSpaceOnUse">
                            <rect x={-pad_x} y={-pad_y} width={revealed.max(1.0)} height={height+2.0*pad_y} />
                        </clipPath></defs>
                        <g clip-path={format!("url(#{id})")}>{background}{replicas}{underlay}{content}{overlay}</g>
                    </g>)
                } else {
                    svgr!(<g>{background}{replicas}{underlay}{content}{overlay}</g>)
                };
                svgr!(<g opacity={opacity} transform={format!(
                    "translate({dx} {dy}) translate({cx} {cy}) rotate({}) scale({scale_x} {scale_y}) translate({} {})",
                    style.layout.angle, -cx, -cy
                )}>{local}</g>)
            })
        }).collect();
        if self.scene.graphics.is_empty() {
            return words;
        }
        let under = self.graphics.render(cue_graphics::Layer::Under, time);
        let over = self.graphics.render(cue_graphics::Layer::Over, time);
        svgr!(<g><g opacity={opacity}>{under}</g>{words}<g opacity={opacity}>{over}</g></g>)
    }
}

fn ease(runtime: &AnimationRuntime, elapsed: f64) -> f64 {
    if elapsed <= 0.0 {
        0.0
    } else if elapsed >= f64::from(runtime.get_duration()) {
        1.0
    } else {
        f64::from(runtime.solve(&(elapsed as f32)))
    }
}

impl EditorialWord {
    fn measured_glyphs<'a>(
        &'a self,
        ctx: &FFramesContext<'a, '_>,
        style: &'a WordStyle,
    ) -> Option<&'a [GlyphPlacement]> {
        if let Some(layout) = self.glyph_layout.get() {
            return Some(layout);
        }
        // Use exactly the caption engine's font lookup and advances. Missing fonts
        // are not cached: the editor may provide them on a later frame.
        let mut x = 0.0;
        let mut layout = Vec::with_capacity(self.glyphs.len());
        for glyph in &self.glyphs {
            let width = estimate_text_width(
                ctx,
                glyph,
                EstimateTextWidthOptions {
                    font_family: &style.layout.font_family,
                    font_size: style.layout.font_size,
                    font_weight: style.layout.font_weight,
                    font_style: FontStyle::default(),
                    font_stretch: FontStretch::default(),
                },
            )? as f64;
            layout.push(GlyphPlacement { x, width });
            x += width;
        }
        // Concurrent render workers can compute the same small layout on first use;
        // all subsequent frames share the first successful immutable result.
        let _ = self.glyph_layout.set(layout);
        self.glyph_layout.get().map(Vec::as_slice)
    }

    fn render_glyphs<'a>(
        &'a self,
        scope: &CaptionWordScope<'a, '_>,
        ctx: &FFramesContext<'a, '_>,
        style: &'a WordStyle,
        time: f64,
    ) -> Option<Svgr<'a>> {
        let duration = f64::from(self.entrance.get_duration());
        let elapsed = time - self.starts;
        // Keep the same measured glyphs through the hold. Switching back to a
        // shaped whole-word node here can change kerning on the settling frame.
        let layout = self.measured_glyphs(ctx, style)?;
        let fill = if scope.word.state == WordState::Upcoming && style.appearance.outline_before {
            "none"
        } else {
            style.appearance.fill.as_str()
        };
        let foreground: Svgr<'a> = self.glyphs.iter().zip(layout).enumerate().map(|(index, (glyph, placement))| {
            let order = if style.motion.reverse { self.glyphs.len() - 1 - index } else { index };
            let local = elapsed - order as f64 * style.motion.stagger_seconds;
            let progress = (local / duration).clamp(0.0, 1.0);
            let eased = ease(&self.entrance, local);
            let remaining = 1.0 - eased;
            let direction = if index % 2 == 0 { -1.0 } else { 1.0 };
            let amplitude = style.motion.amplitude;
            let mut x = 0.0;
            let mut y = 0.0;
            let mut scale_y = 1.0;
            let mut scale_x = 1.0;
            let mut skew = 0.0;
            let mut rotation = 0.0;
            let mut opacity = 1.0;
            match style.motion.entrance {
                Entrance::GlyphHop => {
                    let arc = (std::f64::consts::PI * progress).sin();
                    y = -amplitude * arc;
                    rotation = direction * 2.0 * arc;
                }
                Entrance::GlyphAssemble => {
                    x = direction * amplitude * 0.35 * remaining;
                    y = direction * amplitude * remaining;
                    rotation = direction * 4.0 * remaining;
                    opacity = 0.55 + 0.45 * eased;
                }
                Entrance::Tracking => {
                    let position = if self.glyphs.len() > 1 {
                        index as f64 / (self.glyphs.len() - 1) as f64 - 0.5
                    } else { 0.0 };
                    x = position * amplitude * remaining;
                }
                Entrance::Flip => {
                    scale_y = 0.22 + 0.78 * eased;
                    skew = direction * 12.0 * remaining;
                }
                Entrance::Choreograph => {
                    let pose = self.choreography.pose(index, elapsed);
                    x = pose.x;
                    y = pose.y;
                    scale_x = pose.scale * pose.scale_x;
                    scale_y = pose.scale * pose.scale_y;
                    rotation = pose.rotate;
                    opacity = pose.opacity;
                }
                _ => {}
            }
            let cx = placement.x + placement.width * 0.5;
            let cy = style.layout.font_size as f64 * 0.65;
            svgr!(<g opacity={opacity} transform={format!(
                "translate({x} {y}) translate({cx} {cy}) rotate({rotation}) skewX({skew}) scale({scale_x} {scale_y}) translate({} {})",
                -cx, -cy
            )}>
                <text x={placement.x} y={style.layout.font_size}
                    font-family={style.layout.font_family.as_str()} font-size={style.layout.font_size}
                    font-weight={style.layout.font_weight} fill={fill}
                    stroke={style.appearance.outline.as_str()} stroke-width={style.appearance.outline_width}
                    stroke-linejoin="round" paint-order="stroke fill">{glyph.as_str()}</text>
            </g>)
        }).collect();
        if style.motion.echoes.is_empty() {
            return Some(foreground);
        }
        let echoes: Vec<_> = style.motion.echoes.iter().rev().flat_map(|echo| {
            self.glyphs.iter().zip(layout).enumerate().filter_map(move |(index, (glyph, placement))| {
                let pose = self.choreography.echo_pose(index, elapsed, echo)?;
                let cx = placement.x + placement.width * 0.5;
                let cy = style.layout.font_size as f64 * 0.65;
                let sx = pose.scale * pose.scale_x;
                let sy = pose.scale * pose.scale_y;
                Some(svgr!(<g opacity={pose.opacity} transform={format!(
                    "translate({} {}) translate({cx} {cy}) rotate({}) scale({sx} {sy}) translate({} {})",
                    pose.x, pose.y, pose.rotate, -cx, -cy
                )}>
                    <text x={placement.x} y={style.layout.font_size}
                        font-family={style.layout.font_family.as_str()} font-size={style.layout.font_size}
                        font-weight={style.layout.font_weight} fill={echo.fill.as_str()}>{glyph.as_str()}</text>
                </g>))
            })
        }).collect();
        Some(svgr!(<g>{echoes}{foreground}</g>))
    }
}
