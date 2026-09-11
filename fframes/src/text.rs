use crate::{
    FFramesContext, FontFace, FontSource, FontStretch, FontStyle, FontVariant, Svgr, svgr,
};
use lru::LruCache;
use std::{
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
    pub(crate) breaks_cache: Rc<RefCell<LruCache<u64, Option<WrappedTextStructure>>>>,
    pub(crate) text_width_cache: Rc<RefCell<LruCache<u64, Option<usize>>>>,
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

fn calc_text_width(
    text: &str,
    font_face: &dyn FontFace,
    font_size: usize,
    font_variant: FontVariant,
) -> usize {
    match text {
        "" => 0,
        "\n" | "\r" => 0,
        text => match font_variant {
            FontVariant::Monospaced(mono_width) => text.len() * mono_width,
            FontVariant::Variable => text
                .chars()
                .filter_map(|char| font_face.resolve_char_width(font_size, char))
                .sum(),
        },
    }
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

    match font_variant {
        FontVariant::Monospaced(mono_width) => Some(text.len() * mono_width),
        FontVariant::Variable => {
            let total_width = text
                .chars()
                .filter_map(|char| font_face.resolve_char_width(font_size, char))
                .sum();
            Some(total_width)
        }
    }
}
