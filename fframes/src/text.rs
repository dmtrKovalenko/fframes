use crate::{
    FFramesContext, FontFace, FontSource, FontStretch, FontStyle, FontVariant, Svgr, svgr,
};
use lru::LruCache;
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    rc::Rc,
};
use usvgr::svgtree::SvgAttributeValue;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, Hash)]
/// Define a query that will resolve a font from the provided media source.
/// The values from the struct could be passed directly to the <text> element attributes like:
///
/// ```no_run
/// let font = FontQuery {
///    family: "Arial",
///    size: 16,
///    weight: 700,
///    ..Default::default()
/// }
///
/// svgr!(
///  <text
///     font-size={font.size}
///     height={font.height}
///     font-family={font.family}
///     font-style={font.style}
///     font-weight={font.weight}
///     font-stretch={font.stretch}
///  > "Hello world!" </text>
/// )
/// ```
pub struct FontQuery<'a> {
    pub family: &'a str,
    pub size: usize,
    pub weight: u16,
    /// Font style (normal, italic, oblique) to use. If the font is variadic, applies the style.
    /// If not - uses the will try to resolve font from the family that satisfies this style.
    pub style: FontStyle,
    pub stretch: FontStretch,
}

impl Default for FontQuery<'_> {
    fn default() -> Self {
        Self {
            weight: 500,
            size: 16,
            family: "Arial",
            style: Default::default(),
            stretch: Default::default(),
        }
    }
}

impl FontQuery<'_> {
    pub fn hash_with_value(&self, value: &str) -> u64 {
        let mut s = DefaultHasher::new();
        value.hash(&mut s);
        self.hash(&mut s);

        s.finish()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BreakLinesOpts<
    'a,
    TX: Into<SvgAttributeValue<'a>> + Hash + Default,
    TY: Into<SvgAttributeValue<'a>> + Hash + Default,
> {
    /// The font family to use for the text.
    pub font: FontQuery<'a>,
    pub width: usize,
    /// Similar to css `line-height` property where 1.0 is the line height of the selected font
    /// size. 1.1 is 10% larger than the font size which adds 10% of the font size as a margin
    /// between lines after the break.
    ///
    /// @default 1.1
    pub line_height: f32,
    /// The font family to use for the text.
    /// **Pro tip**: check resolved font family name in the editor media panel.
    pub x: TX,
    pub y: TY,
    pub align: TextAlign,
    pub fill: &'a str,
    /// The [dominant-baseline](https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/dominant-baseline) svg attribute specifies the dominant baseline,
    /// which is the baseline used to align the box's text and inline-level contents.
    /// It also indicates the default alignment baseline of any boxes
    /// participating in baseline alignment in the box's alignment context.
    ///
    /// **In short: defines how to align text vertically based to the `y` position.**
    pub dominant_baseline: &'a str,
    /// The [text-anchor](https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/text-anchor) attribute is used to align (start-, middle- or end-alignment) a string of pre-formatted text or auto-wrapped text where the wrapping area is determined from the inline-size property relative to a given point.
    /// **In short: defines how to align text horizontally based to the `x` position.**
    pub text_anchor: &'a str,
    pub opacity: f32,
}

impl<'a, TX, TY> BreakLinesOpts<'a, TX, TY>
where
    TX: Into<SvgAttributeValue<'a>> + Hash + Default,
    TY: Into<SvgAttributeValue<'a>> + Hash + Default,
{
    pub(crate) fn hash_with_value(&self, value: &str) -> u64 {
        let mut s = DefaultHasher::new();
        value.hash(&mut s);
        self.hash(&mut s);

        s.finish()
    }

    /// When using `frame.text_break_lines_structure` which returns the text structure
    /// use this function to create top level `text` element that applies all the options from the
    /// `BreakLinesOpts` to the `text` svg element.
    pub fn create_text_svgr(self, children: Svgr<'a>) -> Svgr<'a> {
        let BreakLinesOpts::<'a, TX, TY> {
            font,
            x,
            y,
            fill,
            dominant_baseline,
            text_anchor,
            ..
        } = self;

        svgr!(
          <text
            x={x.into()}
            y={y.into()}
            fill={fill}
            font-size={font.size}
            font-family={font.family}
            font-weight={font.weight}
            font-stretch={font.stretch.to_string()}
            dominant-baseline={dominant_baseline}
            text-anchor={text_anchor}
          >
             {children}
          </text>
        )
    }
}

impl<'a, TX, TY> std::hash::Hash for BreakLinesOpts<'a, TX, TY>
where
    TX: Into<SvgAttributeValue<'a>> + Hash + Default,
    TY: Into<SvgAttributeValue<'a>> + Hash + Default,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.width.hash(state);
        self.line_height.to_bits().hash(state);
        self.x.hash(state);
        self.y.hash(state);
        self.align.hash(state);
        self.fill.hash(state);
        self.dominant_baseline.hash(state);
        self.text_anchor.hash(state);
        self.font.hash(state);
    }
}

impl<'a, TX, TY> Default for BreakLinesOpts<'a, TX, TY>
where
    TX: Into<SvgAttributeValue<'a>> + Hash + Default,
    TY: Into<SvgAttributeValue<'a>> + Hash + Default,
{
    fn default() -> Self {
        Self {
            width: 0,
            line_height: 1.1,
            x: TX::default(),
            y: TY::default(),
            fill: "",
            font: FontQuery::default(),
            align: TextAlign::Left,
            dominant_baseline: "auto",
            text_anchor: "start",
            opacity: 1.0,
        }
    }
}

/// This is only fframes cache for text related processing, the actual
/// glyph aliasing, text outlining happens in the rendering backend and
/// can have its own cache
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct TextCache {
    pub(crate) breaks_cache: Rc<RefCell<LruCache<u64, WrappedTextStructure>>>,
    pub(crate) text_width_cache: Rc<RefCell<LruCache<u64, usize>>>,
}

impl TextCache {
    pub fn new(size: usize) -> Option<Self> {
        if size > 0 {
            Some(Self {
                breaks_cache: Rc::new(RefCell::new(LruCache::new(
                    NonZeroUsize::new(size).unwrap(),
                ))),
                text_width_cache: Rc::new(RefCell::new(LruCache::new(
                    NonZeroUsize::new(size).unwrap(),
                ))),
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct WrappedTextLine {
    pub words: Vec<String>,
    pub width: usize,
    pub dx: usize,
    pub dy: usize,
}

#[derive(Debug, Clone, Default)]
pub struct WrappedTextStructure {
    pub lines: Vec<WrappedTextLine>,
    pub(crate) line_height_in_px: f32,
    pub(crate) hash: u64,
}

impl WrappedTextStructure {
    pub fn as_svgr<
        'a,
        X: Into<SvgAttributeValue<'a>> + Hash + Default,
        Y: Into<SvgAttributeValue<'a>> + Hash + Default,
    >(
        &self,
        BreakLinesOpts {
            x,
            y,
            fill,
            font,
            dominant_baseline,
            text_anchor,
            opacity,
            ..
        }: BreakLinesOpts<'a, X, Y>,
    ) -> Svgr<'a> {
        let svgr_x: SvgAttributeValue = x.into();
        let svgr_y: SvgAttributeValue = y.into();

        svgr_macro::svgr! {
            <text
                id={self.hash}
                x={svgr_x.clone()}
                y={svgr_y.clone()}
                fill={fill}
                font-size={font.size}
                font-family={font.family}
                font-weight={font.weight}
                dominant-baseline={dominant_baseline}
                text-anchor={text_anchor}
                opacity={opacity}
            >
                {Svgr::from_iter(
                    self.lines.iter().map(|line| {
                        svgr_macro::svgr! {
                            <tspan
                                x={svgr_x.clone()}
                                y={svgr_y.clone()}
                                dx={line.dx}
                                dy={line.dy}
                            >
                                {line.words.join(" ")}
                            </tspan>
                        }
                    })
                )}
            </text>
        }
    }

    pub fn occupied_height(&self) -> usize {
        let lines = &self.lines;

        if lines.is_empty() {
            return 0;
        }

        if lines.len() == 1 {
            return self.line_height_in_px as usize;
        }

        lines.len() * self.line_height_in_px as usize
    }
}

/// How text that does not fit into its box is shortened, the equivalent of
/// the css `text-overflow` property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextOverflow<'a> {
    /// Drop the characters that do not fit; the result borrows from the input.
    Clip,
    /// Drop the characters that do not fit and end the text with `…`.
    #[default]
    Ellipsis,
    /// Like [`TextOverflow::Ellipsis`] with a custom marker, e.g. `"..."`.
    Marker(&'a str),
}

impl TextOverflow<'_> {
    fn marker(&self) -> &str {
        match self {
            TextOverflow::Clip => "",
            TextOverflow::Ellipsis => "…",
            TextOverflow::Marker(marker) => marker,
        }
    }
}

fn char_width(
    char: char,
    font_face: &dyn FontFace,
    font_size: usize,
    font_variant: FontVariant,
) -> usize {
    match font_variant {
        FontVariant::Monospaced(mono_width) => mono_width,
        FontVariant::Variable => font_face
            .resolve_char_width(font_size, char)
            .unwrap_or_default(),
    }
}

fn calc_text_width(
    text: &str,
    font_face: &dyn FontFace,
    font_size: usize,
    font_variant: FontVariant,
) -> usize {
    match text {
        "" => 0,
        "\n" | "\r" => 0,
        text => text
            .chars()
            .map(|char| char_width(char, font_face, font_size, font_variant))
            .sum(),
    }
}

/// Shortens `value` so it fits into `max_width` pixels, see
/// [`crate::Frame::text_fit`].
pub(crate) fn text_fit_impl<'a>(
    font_query: FontQuery,
    value: &'a str,
    max_width: usize,
    overflow: TextOverflow,
    font_source: &'a (dyn FontSource<'a> + 'a),
) -> Option<Cow<'a, str>> {
    let font_face = font_source.resolve_font(
        font_query.family,
        font_query.weight,
        font_query.style,
        font_query.stretch,
    )?;
    let font_variant = font_face.font_variant(font_query.size)?;
    let width =
        |text: &str| calc_text_width(text, font_face.as_ref(), font_query.size, font_variant);

    let marker = overflow.marker();
    let marker_width = width(marker);

    // One pass over the glyphs: `fits_end` is the end of the longest prefix
    // that leaves room for the marker, `total` the width of the whole text.
    let mut total = 0;
    let mut fits_end = 0;
    for (index, char) in value.char_indices() {
        total += char_width(char, font_face.as_ref(), font_query.size, font_variant);
        if total + marker_width <= max_width {
            fits_end = index + char.len_utf8();
        }
    }

    if total <= max_width {
        return Some(Cow::Borrowed(value));
    }
    if marker_width > max_width {
        // Not even the marker fits.
        return Some(Cow::Borrowed(""));
    }

    let kept = &value[..fits_end];
    if marker.is_empty() {
        return Some(Cow::Borrowed(kept));
    }

    // A marker right after a space reads as a typo ("and …"), so the kept
    // part ends on a visible character.
    Some(Cow::Owned(format!("{}{marker}", kept.trim_end())))
}

pub(crate) fn text_get_width_impl<'a>(
    font_query: FontQuery,
    text: &'a str,
    font_source: &'a (dyn FontSource<'a> + 'a),
) -> Option<usize> {
    let font_face = font_source.resolve_font(
        font_query.family,
        font_query.weight,
        font_query.style,
        font_query.stretch,
    )?;

    let font_variant = font_face.font_variant(font_query.size)?;
    Some(calc_text_width(
        text,
        font_face.as_ref(),
        font_query.size,
        font_variant,
    ))
}

pub(crate) fn text_wrap_impl<
    'a,
    'b,
    TX: Into<SvgAttributeValue<'a>> + Hash + Default,
    TY: Into<SvgAttributeValue<'a>> + Hash + Default,
>(
    hash: u64,
    value: &'b str,
    font_source: &'a (dyn FontSource<'a> + 'a),
    options: BreakLinesOpts<'a, TX, TY>,
) -> Option<WrappedTextStructure> {
    let BreakLinesOpts {
        width,
        align,
        font,
        line_height,
        ..
    } = options;

    let font_face = if let Some(font_face) =
        font_source.resolve_font(font.family, font.weight, font.style, font.stretch)
    {
        font_face
    } else {
        crate::log!(
            "ERROR breaking lines: font is not resolved. Make sure that system fonts are not available for break_lines feature in editor."
        );
        return None;
    };

    let font_variant = font_face.font_variant(font.size)?;
    let space_width = match font_variant {
        crate::FontVariant::Monospaced(mono_width) => mono_width,
        crate::FontVariant::Variable => font_face.resolve_char_width(font.size, ' ')?,
    };

    let mut structure: Vec<(Vec<String>, usize)> = vec![(vec![], 0usize)];
    let line_height_in_px = line_height * font.size as f32;

    for line in value.split('\n') {
        let mut is_first_word_in_line = true;

        for word in line.split_whitespace() {
            let word_width = calc_text_width(word, font_face.as_ref(), font.size, font_variant);
            let (last_line, last_line_width) = structure.last_mut()?;

            // The first word of a line is never preceded by a space.
            let separator_width = if *last_line_width == 0 {
                0
            } else {
                space_width
            };

            if *last_line_width + separator_width + word_width > width && !last_line.is_empty() {
                structure.push((vec![word.to_owned()], word_width));
            } else {
                *last_line_width += separator_width + word_width;
                last_line.push(word.to_owned());
            }
            is_first_word_in_line = false;
        }

        if !is_first_word_in_line {
            structure.push((vec![], 0usize));
        }
    }

    if let Some((last_words, last_width)) = structure.last()
        && last_words.is_empty()
        && *last_width == 0
    {
        structure.pop();
    }

    let lines = structure
        .into_iter()
        .enumerate()
        .map(|(index, (words, line_width))| {
            let dx = match align {
                TextAlign::Left => 0,
                TextAlign::Center => width.saturating_sub(line_width) / 2,
                TextAlign::Right => width.saturating_sub(line_width),
            };

            WrappedTextLine {
                words,
                width: line_width,
                dx,
                dy: (index as f32 * line_height_in_px) as usize,
            }
        })
        .collect();

    Some(WrappedTextStructure {
        lines,
        line_height_in_px,
        hash,
    })
}

#[derive(Debug, Clone)]
pub struct EstimateTextWidthOptions<'a> {
    pub font_family: &'a str,
    pub font_size: usize,
    pub font_weight: u16,
    pub font_style: FontStyle,
    pub font_stretch: FontStretch,
}

/// Estimates the width of the text in pixels.
/// Returns `None` if the font is not resolved.
///
/// **Important! font**
pub fn estimate_text_width<'a>(
    ctx: &FFramesContext<'a, '_>,
    text: &'a str,
    options: EstimateTextWidthOptions<'a>,
) -> Option<usize> {
    let EstimateTextWidthOptions {
        font_family,
        font_size,
        font_weight,
        font_style,
        font_stretch,
    } = options;

    let font_face =
        ctx.font_source?
            .resolve_font(font_family, font_weight, font_style, font_stretch)?;

    let font_variant = font_face.font_variant(font_size)?;
    Some(calc_text_width(
        text,
        font_face.as_ref(),
        font_size,
        font_variant,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Every glyph is `CHAR_WIDTH` pixels wide at any size, which keeps the
    /// expected values below readable.
    const CHAR_WIDTH: usize = 10;

    #[derive(Debug)]
    struct FixedWidthFace {
        monospaced: bool,
    }

    impl FontFace<'_> for FixedWidthFace {
        fn is_monospaced(&self) -> Option<bool> {
            Some(self.monospaced)
        }

        fn resolve_char_width(&self, _font_size: usize, _char: char) -> Option<usize> {
            Some(CHAR_WIDTH)
        }
    }

    #[derive(Debug)]
    struct FixedWidthFonts {
        monospaced: bool,
    }

    impl<'a> FontSource<'a> for FixedWidthFonts {
        fn add_font(&mut self, _: String, _: Arc<dyn AsRef<[u8]> + Sync + Send>) {}

        fn resolve_font(
            &'a self,
            _: &str,
            _: u16,
            _: FontStyle,
            _: FontStretch,
        ) -> Option<Box<dyn FontFace<'a> + 'a>> {
            Some(Box::new(FixedWidthFace {
                monospaced: self.monospaced,
            }))
        }
    }

    const VARIABLE: FixedWidthFonts = FixedWidthFonts { monospaced: false };
    const MONOSPACED: FixedWidthFonts = FixedWidthFonts { monospaced: true };

    fn fit<'a>(
        fonts: &'a FixedWidthFonts,
        value: &'a str,
        max_width: usize,
        overflow: TextOverflow,
    ) -> Cow<'a, str> {
        text_fit_impl(FontQuery::default(), value, max_width, overflow, fonts)
            .expect("the fixed width font always resolves")
    }

    #[test]
    fn text_that_fits_is_returned_untouched() {
        let fitted = fit(
            &VARIABLE,
            "Hello world",
            11 * CHAR_WIDTH,
            TextOverflow::Ellipsis,
        );
        assert!(matches!(fitted, Cow::Borrowed("Hello world")));
    }

    #[test]
    fn ellipsis_replaces_the_characters_that_do_not_fit() {
        // 9 cells: 8 characters + the ellipsis.
        assert_eq!(
            fit(
                &VARIABLE,
                "Hello world",
                9 * CHAR_WIDTH,
                TextOverflow::Ellipsis
            ),
            "Hello wo…"
        );
    }

    #[test]
    fn ellipsis_never_follows_whitespace() {
        // 7 cells would keep "Hello " and the trailing space is dropped.
        assert_eq!(
            fit(
                &VARIABLE,
                "Hello world",
                7 * CHAR_WIDTH,
                TextOverflow::Ellipsis
            ),
            "Hello…"
        );
    }

    #[test]
    fn clip_drops_the_overflow_without_a_marker() {
        let clipped = fit(&VARIABLE, "Hello world", 8 * CHAR_WIDTH, TextOverflow::Clip);
        assert!(matches!(clipped, Cow::Borrowed("Hello wo")));
        // Clip cuts exactly at the box edge, whitespace included.
        assert_eq!(
            fit(&VARIABLE, "Hello world", 6 * CHAR_WIDTH, TextOverflow::Clip),
            "Hello "
        );
    }

    #[test]
    fn custom_marker_width_is_accounted_for() {
        assert_eq!(
            fit(
                &VARIABLE,
                "Hello world",
                8 * CHAR_WIDTH,
                TextOverflow::Marker("...")
            ),
            "Hello..."
        );
    }

    #[test]
    fn marker_wider_than_the_box_yields_nothing() {
        assert_eq!(
            fit(
                &VARIABLE,
                "Hello world",
                2 * CHAR_WIDTH,
                TextOverflow::Marker("...")
            ),
            ""
        );
        assert_eq!(
            fit(&VARIABLE, "Hello world", CHAR_WIDTH, TextOverflow::Ellipsis),
            "…"
        );
    }

    #[test]
    fn monospaced_fonts_count_characters_not_bytes() {
        // "…" is three bytes but one cell.
        assert_eq!(
            fit(
                &MONOSPACED,
                "Ünïcödé text",
                6 * CHAR_WIDTH,
                TextOverflow::Ellipsis
            ),
            "Ünïcö…"
        );
        assert_eq!(
            text_get_width_impl(FontQuery::default(), "Ünïcödé", &MONOSPACED),
            Some(7 * CHAR_WIDTH)
        );
    }
}
