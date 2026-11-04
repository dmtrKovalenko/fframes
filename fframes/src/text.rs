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
/// The values from the struct could be passed directly to the `<text>` element attributes like:
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
            style: FontStyle::default(),
            stretch: FontStretch::default(),
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

/// Widths are tiny and a frame often measures every word.
const TEXT_WIDTH_CACHE_MIN: usize = 1024;

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
                    NonZeroUsize::new(size.max(TEXT_WIDTH_CACHE_MIN)).unwrap(),
                ))),
            })
        } else {
            None
        }
    }

    /// Forgets every measured width and wrapped text, for when a font changes.
    pub fn clear(&self) {
        self.breaks_cache.borrow_mut().clear();
        self.text_width_cache.borrow_mut().clear();
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
                {self.lines.iter().map(|line| {
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
                    }).collect::<Svgr>()}
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

/// Width of `text` in pixels as usvgr lays it out when rendering: shaped per bidi run, with
/// fallback fonts and the variation axes the renderer applies. The text is laid out twice, from
/// its start and from its end at `x = 0`: every glyph moves by exactly the advance width between
/// the two, so glyph offsets such as mark positioning cancel out.
pub fn measure_text_width(
    fontdb: &usvgr::fontdb::Database,
    font: &FontQuery,
    text: &str,
) -> Option<f32> {
    fn escape(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    fn texts<'t>(group: &'t usvgr::Group, found: &mut Vec<&'t usvgr::Text>) {
        for node in group.children() {
            match node {
                usvgr::Node::Text(text) => found.push(text),
                usvgr::Node::Group(group) => texts(group, found),
                _ => {}
            }
        }
    }
    fn first_glyph_x(text: &usvgr::Text) -> Option<f32> {
        let glyph = text.layouted().first()?.positioned_glyphs.first()?;
        Some(glyph.transform().tx)
    }

    if text.is_empty() {
        return Some(0.0);
    }
    let style = match font.style {
        FontStyle::Normal => "normal",
        FontStyle::Italic => "italic",
        FontStyle::Oblique => "oblique",
    };
    let text = format!(
        r#"xml:space="preserve" font-family="{}" font-size="{}" font-weight="{}" font-style="{style}" font-stretch="{}">{}</text>"#,
        escape(font.family),
        font.size,
        font.weight,
        font.stretch,
        escape(text),
    );
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><text text-anchor="start" {text}<text text-anchor="end" {text}</svg>"#
    );
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), fontdb).ok()?;
    let mut found = Vec::with_capacity(2);
    texts(tree.root(), &mut found);
    let [from_start, from_end] = found[..] else {
        return None;
    };
    Some(first_glyph_x(from_start)? - first_glyph_x(from_end)?)
}

/// The renderer's width when the font source can lay text out, per-character advances
/// otherwise.
fn text_width_px(
    text: &str,
    font: &FontQuery,
    font_source: &dyn FontSource,
    font_face: &dyn FontFace,
    font_variant: FontVariant,
) -> f32 {
    match text {
        "" | "\n" | "\r" => 0.0,
        text => font_source.measure_text(font, text).unwrap_or_else(|| {
            text.chars()
                .map(|char| char_width(char, font_face, font.size, font_variant))
                .sum::<usize>() as f32
        }),
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
    let max_width = max_width as f32;
    let width = |text: &str| {
        text_width_px(
            text,
            &font_query,
            font_source,
            font_face.as_ref(),
            font_variant,
        )
    };

    if width(value) <= max_width {
        return Some(Cow::Borrowed(value));
    }
    let marker = overflow.marker();
    if width(marker) > max_width {
        // Not even the marker fits.
        return Some(Cow::Borrowed(""));
    }

    // The kept prefix and the marker are measured together, as they render. Binary search over
    // the number of kept characters: measuring is a layout, so each probe counts.
    let ends: Vec<usize> = value
        .char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .collect();
    let fits = |end: usize| {
        let kept = &value[..end];
        if marker.is_empty() {
            width(kept) <= max_width
        } else {
            width(&format!("{}{marker}", kept.trim_end())) <= max_width
        }
    };
    let (mut low, mut high) = (0, ends.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        if fits(ends[mid - 1]) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    let kept = &value[..low.checked_sub(1).map_or(0, |i| ends[i])];
    if marker.is_empty() {
        return Some(Cow::Borrowed(kept));
    }

    // A marker right after a space reads as a typo ("and …"), so the kept
    // part ends on a visible character.
    Some(Cow::Owned(format!("{}{marker}", kept.trim_end())))
}

pub(crate) fn text_get_width_impl<'a>(
    font_query: FontQuery,
    text: &str,
    font_source: &'a (dyn FontSource<'a> + 'a),
) -> Option<usize> {
    let font_face = font_source.resolve_font(
        font_query.family,
        font_query.weight,
        font_query.style,
        font_query.stretch,
    )?;

    let font_variant = font_face.font_variant(font_query.size)?;
    Some(
        text_width_px(
            text,
            &font_query,
            font_source,
            font_face.as_ref(),
            font_variant,
        )
        .round() as usize,
    )
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

    let Some(font_face) =
        font_source.resolve_font(font.family, font.weight, font.style, font.stretch)
    else {
        crate::log!(
            "ERROR breaking lines: font is not resolved. Make sure that system fonts are not available for break_lines feature in editor."
        );
        return None;
    };

    let font_variant = font_face.font_variant(font.size)?;
    let max_width = width as f32;
    let measure =
        |text: &str| text_width_px(text, &font, font_source, font_face.as_ref(), font_variant);

    // Lines are measured joined, as they render, so spaces and kerning across words count.
    let mut structure: Vec<(Vec<String>, f32)> = vec![(vec![], 0.0)];
    let line_height_in_px = line_height * font.size as f32;

    for line in value.split('\n') {
        let mut is_first_word_in_line = true;

        for word in line.split_whitespace() {
            let (last_line, last_line_width) = structure.last_mut()?;
            let joined = if last_line.is_empty() {
                word.to_owned()
            } else {
                format!("{} {word}", last_line.join(" "))
            };
            let joined_width = measure(&joined);

            if joined_width > max_width && !last_line.is_empty() {
                structure.push((vec![word.to_owned()], measure(word)));
            } else {
                *last_line_width = joined_width;
                last_line.push(word.to_owned());
            }
            is_first_word_in_line = false;
        }

        if !is_first_word_in_line {
            structure.push((vec![], 0.0));
        }
    }

    if structure.last().is_some_and(|(words, _)| words.is_empty()) {
        structure.pop();
    }

    let lines = structure
        .into_iter()
        .enumerate()
        .map(|(index, (words, line_width))| {
            let line_width = line_width.round() as usize;
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

    text_get_width_impl(
        FontQuery {
            family: font_family,
            size: font_size,
            weight: font_weight,
            style: font_style,
            stretch: font_stretch,
        },
        text,
        ctx.font_source?,
    )
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
