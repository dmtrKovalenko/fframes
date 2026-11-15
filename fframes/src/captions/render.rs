use super::{CaptionError, CaptionMetadata, CaptionTrack, WordState};
use crate::{FFramesContext, FontQuery, Frame, Svgr, diagnostics, svgr};
use parking_lot::Mutex;
use std::{
    cell::Cell,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    ops::Range,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicUsize, Ordering},
    },
};

/// A rectangle in composition pixels, with a top-left origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptionRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl CaptionRect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// The same timed data can be displayed in several ways without changing its layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CaptionMode {
    /// Hold the whole phrase; style each word according to its timing state.
    #[default]
    Highlight,
    /// Show only the active word, centered in the caption area. Gaps stay empty.
    Word,
    /// Reveal spoken and active words at fixed positions; upcoming words stay hidden.
    Reveal,
    /// Fill the active word left-to-right according to its elapsed duration.
    Karaoke,
}

/// Scene-local time by default, matching other frame animation APIs. At video level
/// scene and global time are equal. Use `Global` for a whole-video transcript.
/// `words_at` and explicit render-at methods always accept track seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CaptionClock {
    Global,
    #[default]
    Scene,
}

/// Per-word information for custom drawing. `rect` is an advance box, not glyph ink bounds.
#[derive(Debug, Clone)]
pub struct CaptionWordView<'a> {
    pub text: &'a str,
    /// Application-owned JSON metadata on the original timed word. Untimed
    /// phrase tokens expose an empty map, without inventing per-word metadata.
    pub metadata: &'a CaptionMetadata,
    /// Application-owned JSON metadata on the original cue, including for
    /// untimed phrase tokens. The caption renderer does not interpret its keys.
    pub cue_metadata: &'a CaptionMetadata,
    pub rect: CaptionRect,
    pub state: WordState,
    pub progress: f32,
    pub cue_index: usize,
    /// Index of this display token, including tokens in plain phrase captions.
    pub token_index: usize,
    /// `None` for untimed tokens of a plain phrase imported from VTT.
    pub word_index: Option<usize>,
    /// Track-clock seconds, or `None` for a plain phrase without word timing.
    pub start: Option<f64>,
    pub end: Option<f64>,
}

/// One word's drawing scope. Return any SVG from a caption rendering callback.
///
/// Callback drawing uses local coordinates: `(0, 0)` is the word's top-left advance
/// box. The component applies placement afterwards. Nothing is implicitly clipped,
/// so glows and decorations can extend outside the box. Use `clip` for a rectangular
/// boundary or `mask_text` for the glyph silhouette.
pub struct CaptionWordScope<'a, 'frame> {
    /// Word metadata and a local advance rectangle whose origin is `(0, 0)`.
    pub word: CaptionWordView<'a>,
    /// Original cue metadata, borrowed from the same loaded track as `word`.
    /// Read `word.metadata` for per-word effect or layout parameters.
    pub cue_metadata: &'a CaptionMetadata,
    /// Explicit composition placement, for inspection or advanced positioning.
    /// Drawing returned by the callback is translated here by the component.
    pub composition_rect: CaptionRect,
    /// The original frame; use it for animation or `Shader::draw` without borrowing
    /// the mutable frame passed to `render_with` again.
    pub frame: &'frame Frame,
    /// Caption-clock seconds (global, scene-local, or explicit render-at time).
    pub time: f64,
    captions: &'a Captions,
    prefix: String,
    next_definition: Cell<usize>,
}

impl<'a> CaptionWordScope<'a, '_> {
    pub fn width(&self) -> f64 {
        self.word.rect.width
    }
    pub fn height(&self) -> f64 {
        self.word.rect.height
    }
    pub fn is_active(&self) -> bool {
        self.word.state == WordState::Active
    }

    /// Seconds since this timed word began, clamped to zero. Plain phrases return `None`.
    pub fn elapsed(&self) -> Option<f64> {
        self.word.start.map(|start| (self.time - start).max(0.0))
    }

    /// An SVG-safe ID for caller-owned definitions. The same key gives the same ID
    /// within this scope; separate tokens, components and render calls cannot collide.
    /// IDs are internal drawing references, not persistent word identities.
    pub fn id(&self, key: &str) -> String {
        let mut hash = DefaultHasher::new();
        key.hash(&mut hash);
        format!("{}-user-{:x}", self.prefix, hash.finish())
    }

    fn definition_id(&self, kind: &str) -> String {
        let index = self.next_definition.get();
        self.next_definition.set(index + 1);
        format!("{}-internal-{kind}-{index}", self.prefix)
    }

    /// Restrict any local SVG to the word's advance rectangle.
    pub fn clip(&self, content: Svgr<'a>) -> Svgr<'a> {
        if self.width() <= 0.0 || self.height() <= 0.0 {
            return Svgr::empty();
        }
        let id = self.definition_id("clip");
        svgr!(
            <defs><clipPath id={id.clone()} clipPathUnits="userSpaceOnUse">
                <rect x="0" y="0" width={self.width()} height={self.height()} />
            </clipPath></defs>
            <g clip-path={format!("url(#{id})")}>{content}</g>
        )
    }

    /// Show any local SVG only through this word's glyphs, using the caption font.
    /// This can turn a gradient, shader image or animation into a text highlight.
    pub fn mask_text(&self, content: Svgr<'a>) -> Svgr<'a> {
        if self.width() <= 0.0 || self.height() <= 0.0 {
            return Svgr::empty();
        }
        let id = self.definition_id("mask");
        let mut local = self.word.clone();
        local.rect.x = 0.0;
        local.rect.y = 0.0;
        let glyphs = self.captions.text(&local, "white", false);
        // Leave room for glyph overhangs; the mask content is still the exact glyph
        // silhouette. An advance box is not a tight ink bounding box.
        let padding = self.captions.font_size as f64;
        svgr!(
            <defs><mask id={id.clone()} maskUnits="userSpaceOnUse" maskContentUnits="userSpaceOnUse"
                mask-type="alpha" x={-padding} y={-padding}
                width={self.width() + padding * 2.0} height={self.height() + padding * 2.0}>
                {glyphs}
            </mask></defs>
            <g mask={format!("url(#{id})")}>{content}</g>
        )
    }
}

#[derive(Debug)]
struct PreparedWord {
    token: PreparedToken,
    rect: CaptionRect,
}

#[derive(Debug)]
enum PreparedToken {
    Word(usize),
    Text(Range<usize>),
}

#[derive(Debug)]
struct PreparedCue {
    words: Vec<PreparedWord>,
    height: f64,
}

#[derive(Debug)]
struct PreparedTrack {
    identity: Arc<()>,
    cues: Vec<PreparedCue>,
    inner: CaptionRect,
}

#[derive(Debug)]
enum CaptionSource {
    Owned(CaptionTrack),
    Media(String),
}

/// An immutable caption component. Create once, render at any frame and in any order.
///
/// Font resolution and word layout are cached after their first successful evaluation.
/// Named media is resolved from the context and the cache follows loaded track revisions.
/// Missing fonts and overflow are reported through frame diagnostics by `render`; use
/// `try_render` or `words_at` when the caller wants an explicit error.
#[derive(Debug)]
pub struct Captions {
    source: CaptionSource,
    font: String,
    font_size: usize,
    font_weight: u16,
    bounds: CaptionRect,
    mode: CaptionMode,
    clock: CaptionClock,
    upcoming: String,
    active: String,
    spoken: String,
    outline: Option<(String, f64)>,
    background: Option<(String, f64)>,
    underline: Option<(String, f64)>,
    line_height: f64,
    layout: Mutex<Option<PreparedTrack>>,
    id: usize,
}

static NEXT_COMPONENT: AtomicUsize = AtomicUsize::new(0);
static NEXT_RENDER: AtomicUsize = AtomicUsize::new(0);
static EMPTY_METADATA: LazyLock<CaptionMetadata> = LazyLock::new(CaptionMetadata::new);

impl Captions {
    pub fn new(track: CaptionTrack, bounds: CaptionRect) -> Self {
        Self::with_source(CaptionSource::Owned(track), bounds)
    }

    /// Use a caption track loaded by the `FFrames` media provider, like images or audio.
    /// The same component follows a newly loaded revision under the same media name.
    pub fn from_media(name: impl Into<String>, bounds: CaptionRect) -> Self {
        Self::with_source(CaptionSource::Media(name.into()), bounds)
    }

    fn with_source(source: CaptionSource, bounds: CaptionRect) -> Self {
        Self {
            source,
            font: "DM Sans".into(),
            font_size: 64,
            font_weight: 600,
            bounds,
            mode: CaptionMode::default(),
            clock: CaptionClock::default(),
            upcoming: "#aeb4c0".into(),
            active: "#ffd166".into(),
            spoken: "#ffffff".into(),
            outline: Some(("#000000".into(), 3.0)),
            background: None,
            underline: None,
            line_height: 1.25,
            layout: Mutex::new(None),
            id: NEXT_COMPONENT.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Resolve the current track from its configured source.
    pub fn track<'a, 'ctx, 'media: 'a>(
        &'a self,
        ctx: &FFramesContext<'ctx, 'media>,
    ) -> Result<&'a CaptionTrack, CaptionError> {
        match &self.source {
            CaptionSource::Owned(track) => Ok(track),
            CaptionSource::Media(name) => ctx.get_subtitles(name).ok_or_else(|| {
                CaptionError::invalid(
                    format!("media.{name}"),
                    "caption track is not loaded; add it through the FFrames media provider",
                )
            }),
        }
    }

    pub fn font(mut self, family: impl Into<String>, size: usize) -> Self {
        self.font = family.into();
        self.font_size = size;
        self.layout.get_mut().take();
        self
    }
    pub fn font_weight(mut self, weight: u16) -> Self {
        self.font_weight = weight;
        self.layout.get_mut().take();
        self
    }
    /// Change the logical bounds supplied at construction. The text sits at the bottom of this
    /// area; overlapping cues stack vertically. Draft output scaling does not change it.
    pub fn bounds(mut self, bounds: CaptionRect) -> Self {
        self.bounds = bounds;
        self.layout.get_mut().take();
        self
    }
    pub fn line_height(mut self, multiplier: f64) -> Self {
        self.line_height = multiplier;
        self.layout.get_mut().take();
        self
    }
    pub fn mode(mut self, mode: CaptionMode) -> Self {
        self.mode = mode;
        self.layout.get_mut().take();
        self
    }
    pub fn clock(mut self, clock: CaptionClock) -> Self {
        self.clock = clock;
        self
    }
    pub fn active_fill(mut self, fill: impl Into<String>) -> Self {
        self.active = fill.into();
        self
    }
    pub fn upcoming_fill(mut self, fill: impl Into<String>) -> Self {
        self.upcoming = fill.into();
        self
    }
    pub fn spoken_fill(mut self, fill: impl Into<String>) -> Self {
        self.spoken = fill.into();
        self
    }
    pub fn outline(mut self, fill: impl Into<String>, width: f64) -> Self {
        self.outline = Some((fill.into(), width));
        self.layout.get_mut().take();
        self
    }
    pub fn active_background(mut self, fill: impl Into<String>, padding: f64) -> Self {
        self.background = Some((fill.into(), padding));
        self.layout.get_mut().take();
        self
    }
    pub fn underline(mut self, fill: impl Into<String>, width: f64) -> Self {
        self.underline = Some((fill.into(), width));
        self.layout.get_mut().take();
        self
    }

    fn font_query(&self) -> FontQuery<'_> {
        FontQuery {
            family: &self.font,
            size: self.font_size,
            weight: self.font_weight,
            ..Default::default()
        }
    }

    fn prepare(
        &self,
        track: &CaptionTrack,
        _frame: &mut Frame,
        ctx: &FFramesContext<'_, '_>,
    ) -> Result<PreparedTrack, CaptionError> {
        if self.font_size == 0 || !self.line_height.is_finite() || self.line_height < 1.0 {
            return Err(CaptionError::invalid(
                "layout.font",
                "font size must be positive and line_height must be finite and >= 1",
            ));
        }
        let mut inset: f64 = 0.0;
        for (name, setting) in [
            ("outline", &self.outline),
            ("background", &self.background),
            ("underline", &self.underline),
        ] {
            if let Some((_, size)) = setting {
                if !size.is_finite() || *size < 0.0 {
                    return Err(CaptionError::invalid(
                        format!("style.{name}"),
                        "size must be finite and nonnegative",
                    ));
                }
                inset = inset.max(*size);
            }
        }
        let bounds = self.bounds;
        if ![bounds.x, bounds.y, bounds.width, bounds.height]
            .into_iter()
            .all(f64::is_finite)
            || bounds.width <= inset * 2.0
            || bounds.height <= inset * 2.0
        {
            return Err(CaptionError::invalid(
                "layout.bounds",
                "bounds must be finite and have room for text and effects",
            ));
        }
        let inner = CaptionRect::new(
            bounds.x + inset,
            bounds.y + inset,
            bounds.width - inset * 2.0,
            bounds.height - inset * 2.0,
        );
        let font_source = ctx.font_source.ok_or_else(|| {
            CaptionError::invalid("layout.font", "caption layout needs a font source")
        })?;
        let measure = |text: &str| {
            crate::text_get_width_impl(self.font_query(), text, font_source)
                .map(|width| width as f64)
                .ok_or_else(|| {
                    CaptionError::invalid(
                        "layout.font",
                        format!(
                            "cannot measure font {:?}; load this font in the media provider",
                            self.font
                        ),
                    )
                })
        };
        let space = measure(" ")?;
        let line_height = self.font_size as f64 * self.line_height;
        let mut cues = Vec::with_capacity(track.cues().len());
        for (cue_index, cue) in track.cues().iter().enumerate() {
            let tokens: Vec<(&str, PreparedToken, bool, bool)> = if cue.words.is_empty() {
                cue.text
                    .lines()
                    .enumerate()
                    .flat_map(|(line_index, line)| {
                        line.split_whitespace()
                            .enumerate()
                            .map(move |(index, text)| {
                                let start = text.as_ptr() as usize - cue.text.as_ptr() as usize;
                                (
                                    text,
                                    PreparedToken::Text(start..start + text.len()),
                                    true,
                                    line_index > 0 && index == 0,
                                )
                            })
                    })
                    .collect()
            } else {
                cue.words
                    .iter()
                    .enumerate()
                    .map(|(i, word)| {
                        (
                            word.text.as_str(),
                            PreparedToken::Word(i),
                            word.space_before,
                            false,
                        )
                    })
                    .collect()
            };
            let mut lines: Vec<(Vec<PreparedWord>, f64)> = vec![(Vec::new(), 0.0)];
            for (text, token, space_before, break_before) in tokens {
                let width = measure(text)?;
                if width > inner.width {
                    return Err(CaptionError::invalid(
                        format!("cues[{cue_index}].layout"),
                        format!(
                            "word {text:?} exceeds caption width; increase width or reduce font size"
                        ),
                    ));
                }
                if break_before && !lines[lines.len() - 1].0.is_empty() {
                    lines.push((Vec::new(), 0.0));
                }
                let last_index = lines.len() - 1;
                let (line, line_width) = &mut lines[last_index];
                let separator = if line.is_empty() || !space_before {
                    0.0
                } else {
                    space
                };
                if !line.is_empty() && *line_width + separator + width > inner.width {
                    lines.push((
                        vec![PreparedWord {
                            token,
                            rect: CaptionRect::new(0.0, 0.0, width, line_height),
                        }],
                        width,
                    ));
                } else {
                    line.push(PreparedWord {
                        token,
                        rect: CaptionRect::new(*line_width + separator, 0.0, width, line_height),
                    });
                    *line_width += separator + width;
                }
            }
            let height = lines.len() as f64 * line_height;
            let displayed_height = if self.mode == CaptionMode::Word && !cue.words.is_empty() {
                line_height
            } else {
                height
            };
            if displayed_height > inner.height {
                return Err(CaptionError::invalid(
                    format!("cues[{cue_index}].layout"),
                    "wrapped phrase exceeds caption height; enlarge the area or reduce font size",
                ));
            }
            let words = lines
                .into_iter()
                .enumerate()
                .flat_map(|(line_index, (words, width))| {
                    words.into_iter().map(move |mut word| {
                        word.rect.x += (inner.width - width) / 2.0;
                        word.rect.y = line_index as f64 * line_height;
                        word
                    })
                })
                .collect();
            cues.push(PreparedCue { words, height });
        }
        Ok(PreparedTrack {
            identity: track.cache_identity().clone(),
            cues,
            inner,
        })
    }

    /// Word state and positioned advance boxes at explicit track seconds. The full phrase
    /// is laid out once, so changing active words cannot make other words move.
    pub fn words_at<'a, 'ctx, 'media: 'a>(
        &'a self,
        seconds: f64,
        frame: &mut Frame,
        ctx: &FFramesContext<'ctx, 'media>,
    ) -> Result<Vec<CaptionWordView<'a>>, CaptionError> {
        if !seconds.is_finite() {
            return Err(CaptionError::invalid("time", "seconds must be finite"));
        }
        let track = self.track(ctx)?;
        let active: Vec<_> = track.active_cues(seconds).collect();
        if active.is_empty() {
            return Ok(Vec::new());
        }
        let mut cached = self.layout.lock();
        if cached
            .as_ref()
            .is_none_or(|layout| !Arc::ptr_eq(&layout.identity, track.cache_identity()))
        {
            // Failed preparation leaves any previous revision intact and retries next time.
            // The identity token stays allocated with the cached layout, preventing reuse.
            let prepared = self.prepare(track, frame, ctx)?;
            *cached = Some(prepared);
        }
        let layout = cached
            .as_ref()
            .ok_or_else(|| CaptionError::invalid("layout", "layout preparation failed"))?;
        let height = |index: usize| {
            if self.mode == CaptionMode::Word && !track.cues()[index].words.is_empty() {
                self.font_size as f64 * self.line_height
            } else {
                layout.cues[index].height
            }
        };
        let gap = self.font_size as f64 * 0.3;
        let total = active.iter().map(|(i, _)| height(*i)).sum::<f64>()
            + active.len().saturating_sub(1) as f64 * gap;
        if total > layout.inner.height {
            return Err(CaptionError::invalid(
                "layout.overlap",
                "simultaneous cues exceed caption height; enlarge the area or use separate components",
            ));
        }
        let mut top = layout.inner.y + layout.inner.height - total;
        let mut views = Vec::new();
        for (cue_index, cue) in active {
            let single_word = self.mode == CaptionMode::Word && !cue.words.is_empty();
            for (token_index, word) in layout.cues[cue_index].words.iter().enumerate() {
                let (text, word_index) = match &word.token {
                    PreparedToken::Word(index) => (cue.words[*index].text.as_str(), Some(*index)),
                    PreparedToken::Text(range) => (&cue.text[range.clone()], None),
                };
                let (state, progress) = word_index.map_or((WordState::Spoken, 1.0), |index| {
                    (
                        cue.words[index].state_at(seconds),
                        cue.words[index].progress_at(seconds),
                    )
                });
                if single_word && state != WordState::Active {
                    continue;
                }
                let rect = if single_word {
                    CaptionRect::new(
                        layout.inner.x + (layout.inner.width - word.rect.width) / 2.0,
                        top,
                        word.rect.width,
                        word.rect.height,
                    )
                } else {
                    CaptionRect::new(
                        layout.inner.x + word.rect.x,
                        top + word.rect.y,
                        word.rect.width,
                        word.rect.height,
                    )
                };
                views.push(CaptionWordView {
                    text,
                    metadata: word_index
                        .map_or(&EMPTY_METADATA, |index| &cue.words[index].metadata),
                    cue_metadata: &cue.metadata,
                    rect,
                    state,
                    progress,
                    cue_index,
                    token_index,
                    word_index,
                    start: word_index.map(|index| cue.words[index].start),
                    end: word_index.map(|index| cue.words[index].end),
                });
            }
            top += height(cue_index) + gap;
        }
        Ok(views)
    }

    /// Render using the selected global or scene clock; errors become inspection findings.
    pub fn render<'a, 'ctx, 'media: 'a>(
        &'a self,
        frame: &mut Frame,
        ctx: &FFramesContext<'ctx, 'media>,
    ) -> Svgr<'a> {
        match self.try_render(frame, ctx) {
            Ok(svg) => svg,
            Err(error) => {
                diagnostics::report(diagnostics::Diagnostic::CaptionError {
                    message: error.to_string(),
                });
                Svgr::empty()
            }
        }
    }

    pub fn try_render<'a, 'ctx, 'media: 'a>(
        &'a self,
        frame: &mut Frame,
        ctx: &FFramesContext<'ctx, 'media>,
    ) -> Result<Svgr<'a>, CaptionError> {
        self.try_render_with(frame, ctx, |_, content| content)
    }

    /// Use explicit track seconds, for example after mapping from a source clock.
    pub fn try_render_at<'a, 'ctx, 'media: 'a>(
        &'a self,
        seconds: f64,
        frame: &mut Frame,
        ctx: &FFramesContext<'ctx, 'media>,
    ) -> Result<Svgr<'a>, CaptionError> {
        self.try_render_at_with(seconds, frame, ctx, |_, content| content)
    }

    /// Render at explicit track seconds; errors become inspection findings.
    pub fn render_at<'a, 'ctx, 'media: 'a>(
        &'a self,
        seconds: f64,
        frame: &mut Frame,
        ctx: &FFramesContext<'ctx, 'media>,
    ) -> Svgr<'a> {
        report_render_result(self.try_render_at(seconds, frame, ctx))
    }

    /// Customize every visible word at explicit track seconds.
    pub fn render_at_with<'a, 'ctx, 'media: 'a, F>(
        &'a self,
        seconds: f64,
        frame: &mut Frame,
        ctx: &FFramesContext<'ctx, 'media>,
        render_word: F,
    ) -> Svgr<'a>
    where
        F: for<'frame> FnMut(&CaptionWordScope<'a, 'frame>, Svgr<'a>) -> Svgr<'a>,
    {
        report_render_result(self.try_render_at_with(seconds, frame, ctx, render_word))
    }

    /// Customize active words at explicit track seconds. Other words keep their preset SVG.
    pub fn highlight_at_with<'a, 'ctx, 'media: 'a, F>(
        &'a self,
        seconds: f64,
        frame: &mut Frame,
        ctx: &FFramesContext<'ctx, 'media>,
        highlight: F,
    ) -> Svgr<'a>
    where
        F: for<'frame> FnMut(&CaptionWordScope<'a, 'frame>, Svgr<'a>) -> Svgr<'a>,
    {
        report_render_result(self.try_highlight_at_with(seconds, frame, ctx, highlight))
    }

    pub fn try_highlight_at_with<'a, 'ctx, 'media: 'a, F>(
        &'a self,
        seconds: f64,
        frame: &mut Frame,
        ctx: &FFramesContext<'ctx, 'media>,
        mut highlight: F,
    ) -> Result<Svgr<'a>, CaptionError>
    where
        F: for<'frame> FnMut(&CaptionWordScope<'a, 'frame>, Svgr<'a>) -> Svgr<'a>,
    {
        self.try_render_at_with(seconds, frame, ctx, |scope, content| {
            if scope.is_active() {
                highlight(scope, content)
            } else {
                content
            }
        })
    }

    /// Customize every visible word. The callback returns SVG in word-local coordinates.
    /// The second argument owns the preset SVG; return it or compose it without cloning.
    /// `Word` and `Reveal` modes
    /// filter invisible words before invoking the callback. The callback is never stored.
    pub fn render_with<'a, 'ctx, 'media: 'a, F>(
        &'a self,
        frame: &mut Frame,
        ctx: &FFramesContext<'ctx, 'media>,
        render_word: F,
    ) -> Svgr<'a>
    where
        F: for<'frame> FnMut(&CaptionWordScope<'a, 'frame>, Svgr<'a>) -> Svgr<'a>,
    {
        match self.try_render_with(frame, ctx, render_word) {
            Ok(svg) => svg,
            Err(error) => {
                diagnostics::report(diagnostics::Diagnostic::CaptionError {
                    message: error.to_string(),
                });
                Svgr::empty()
            }
        }
    }

    pub fn try_render_with<'a, 'ctx, 'media: 'a, F>(
        &'a self,
        frame: &mut Frame,
        ctx: &FFramesContext<'ctx, 'media>,
        render_word: F,
    ) -> Result<Svgr<'a>, CaptionError>
    where
        F: for<'frame> FnMut(&CaptionWordScope<'a, 'frame>, Svgr<'a>) -> Svgr<'a>,
    {
        if frame.fps == 0 {
            return Err(CaptionError::invalid("clock.fps", "FPS must be positive"));
        }
        let index = match self.clock {
            CaptionClock::Global => frame.global_index,
            CaptionClock::Scene => frame.index,
        };
        self.try_render_at_with(index as f64 / frame.fps as f64, frame, ctx, render_word)
    }

    /// Customize only active timed words. Other visible words retain their preset drawing.
    /// No highlight callback runs in a speech gap or for untimed phrase text.
    pub fn highlight_with<'a, 'ctx, 'media: 'a, F>(
        &'a self,
        frame: &mut Frame,
        ctx: &FFramesContext<'ctx, 'media>,
        highlight: F,
    ) -> Svgr<'a>
    where
        F: for<'frame> FnMut(&CaptionWordScope<'a, 'frame>, Svgr<'a>) -> Svgr<'a>,
    {
        match self.try_highlight_with(frame, ctx, highlight) {
            Ok(svg) => svg,
            Err(error) => {
                diagnostics::report(diagnostics::Diagnostic::CaptionError {
                    message: error.to_string(),
                });
                Svgr::empty()
            }
        }
    }

    pub fn try_highlight_with<'a, 'ctx, 'media: 'a, F>(
        &'a self,
        frame: &mut Frame,
        ctx: &FFramesContext<'ctx, 'media>,
        mut highlight: F,
    ) -> Result<Svgr<'a>, CaptionError>
    where
        F: for<'frame> FnMut(&CaptionWordScope<'a, 'frame>, Svgr<'a>) -> Svgr<'a>,
    {
        self.try_render_with(frame, ctx, |scope, content| {
            if scope.is_active() {
                highlight(scope, content)
            } else {
                content
            }
        })
    }

    /// Custom drawing at explicit caption-clock seconds, for mapped source timing.
    pub fn try_render_at_with<'a, 'ctx, 'media: 'a, F>(
        &'a self,
        seconds: f64,
        frame: &mut Frame,
        ctx: &FFramesContext<'ctx, 'media>,
        mut render_word: F,
    ) -> Result<Svgr<'a>, CaptionError>
    where
        F: for<'frame> FnMut(&CaptionWordScope<'a, 'frame>, Svgr<'a>) -> Svgr<'a>,
    {
        // words_at releases its layout lock before the callback can execute or re-enter.
        let words = self.words_at(seconds, frame, ctx)?;
        let invocation = NEXT_RENDER.fetch_add(1, Ordering::Relaxed);
        Ok(words
            .into_iter()
            .filter(|word| self.mode != CaptionMode::Reveal || word.state != WordState::Upcoming)
            .map(|word| {
                let prefix = format!(
                    "caption-{}-{invocation}-{}-{}",
                    self.id, word.cue_index, word.token_index
                );
                let mut local = word.clone();
                local.rect.x = 0.0;
                local.rect.y = 0.0;
                let content = self.draw_word(&local, format!("{prefix}-preset-karaoke"));
                let x = word.rect.x;
                let y = word.rect.y;
                let scope = CaptionWordScope {
                    cue_metadata: word.cue_metadata,
                    word: local,
                    composition_rect: word.rect,
                    frame,
                    time: seconds,
                    captions: self,
                    prefix,
                    next_definition: Cell::new(0),
                };
                let svg = render_word(&scope, content);
                svgr!(<g transform={format!("translate({x} {y})")}>{svg}</g>)
            })
            .collect())
    }

    fn text<'a>(&'a self, word: &CaptionWordView<'a>, fill: &'a str, outlined: bool) -> Svgr<'a> {
        let (stroke, width) = if outlined {
            self.outline
                .as_ref()
                .map_or(("none", 0.0), |(fill, width)| (fill.as_str(), *width))
        } else {
            ("none", 0.0)
        };
        svgr!(<text x={word.rect.x} y={word.rect.y + self.font_size as f64}
            font-family={self.font.as_str()} font-size={self.font_size} font-weight={self.font_weight}
            fill={fill} stroke={stroke} stroke-width={width} stroke-linejoin="round" paint-order="stroke fill"
            >{word.text}</text>)
    }

    fn draw_word<'a>(&'a self, word: &CaptionWordView<'a>, karaoke_id: String) -> Svgr<'a> {
        let fill = match word.state {
            WordState::Upcoming => self.upcoming.as_str(),
            WordState::Active => self.active.as_str(),
            WordState::Spoken => self.spoken.as_str(),
        };
        let mut nodes = Vec::new();
        if word.state == WordState::Active
            && word.rect.width > 0.0
            && let Some((fill, padding)) = &self.background
        {
            nodes.push(
                svgr!(<rect x={word.rect.x - padding} y={word.rect.y - padding}
                width={word.rect.width + padding * 2.0} height={word.rect.height + padding * 2.0}
                rx={padding} fill={fill.as_str()} />),
            );
        }
        if self.mode == CaptionMode::Karaoke && word.state == WordState::Active {
            nodes.push(self.text(word, &self.upcoming, true));
            if word.progress > 0.0 && word.rect.width > 0.0 {
                let id = karaoke_id;
                let text = self.text(word, &self.active, false);
                nodes.push(svgr!(
                <defs><clipPath id={id.clone()}><rect x={word.rect.x} y={word.rect.y - 4.0}
                        width={word.rect.width * f64::from(word.progress)} height={word.rect.height + 8.0} /></clipPath></defs>
                <g clip-path={format!("url(#{id})")}>{text}</g>
            ));
            }
        } else {
            nodes.push(self.text(word, fill, true));
        }
        if word.state == WordState::Active
            && word.rect.width > 0.0
            && let Some((fill, width)) = &self.underline
            && *width > 0.0
        {
            nodes.push(
                svgr!(<rect x={word.rect.x} y={word.rect.y + self.font_size as f64 + 2.0}
                width={word.rect.width} height={*width} fill={fill.as_str()} />),
            );
        }
        nodes.into_iter().collect()
    }
}

fn report_render_result(result: Result<Svgr<'_>, CaptionError>) -> Svgr<'_> {
    result.unwrap_or_else(|error| {
        diagnostics::report(diagnostics::Diagnostic::CaptionError {
            message: error.to_string(),
        });
        Svgr::empty()
    })
}
