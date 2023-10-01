use crate::{svgr, FontSource, FontStretch, FontStyle, Svgr};
use lru::LruCache;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
};

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy)]
pub struct BreakLinesOpts<'a> {
    pub width: usize,
    /// Similar to css `line-height` property where 1.0 is the line height of the selected font
    /// size. 1.1 is 10% larger than the font size which adds 10% of the font size as a margin
    /// between lines after the break.
    ///
    /// @default 1.1
    pub line_height: f32,
    /// The font family to use for the text.
    /// Remember that you can check the font family name in the editor media panel.
    pub font_family: &'a str,
    pub font_size: usize,
    pub x: &'a str,
    pub y: &'a str,
    pub align: TextAlign,
    pub font_weight: u16,
    pub fill: &'a str,
    /// Font style (normal, italic, oblique) to use. If the font is variadic, applies the style.
    /// If not - uses the font tha applies this style and name.
    pub font_style: FontStyle,
    pub font_stretch: FontStretch,
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
}

impl BreakLinesOpts<'_> {
    pub(crate) fn hash_with_value(&self, value: &str) -> u64 {
        let mut s = DefaultHasher::new();
        value.hash(&mut s);
        self.hash(&mut s);

        s.finish()
    }

    /// When using `frame.text_break_lines_structure` which returns the text structure
    /// use this function to create top level `text` element that applies all the options from the
    /// `BreakLinesOpts` to the `text` svg element.
    pub fn create_text_svgr(&self, children: Svgr) -> Svgr {
        let BreakLinesOpts {
            font_family,
            font_size,
            font_weight,
            x,
            y,
            fill,
            dominant_baseline,
            text_anchor,
            font_stretch,
            ..
        } = self;

        svgr!(
          <text x={x} y={y} fill={fill} font-size={font_size} font-family={font_family} font-weight={font_weight} font-stretch={font_stretch} dominant-baseline={dominant_baseline} text-anchor={text_anchor}>
             {children}
          </text>
        )
    }
}

impl std::hash::Hash for BreakLinesOpts<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.width.hash(state);
        self.line_height.to_bits().hash(state);
        self.font_size.hash(state);
        self.x.hash(state);
        self.y.hash(state);
        self.align.hash(state);
        self.font_weight.hash(state);
        self.fill.hash(state);
        self.font_family.hash(state);
        self.dominant_baseline.hash(state);
        self.text_anchor.hash(state);
    }
}

impl Default for BreakLinesOpts<'_> {
    fn default() -> Self {
        Self {
            width: 0,
            line_height: 1.1,
            font_family: Default::default(),
            font_size: 16,
            x: "",
            y: "",
            fill: "",
            align: TextAlign::Left,
            font_style: Default::default(),
            font_stretch: Default::default(),
            font_weight: 400,
            dominant_baseline: "auto",
            text_anchor: "start",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BreaksLruCache(pub(crate) Arc<Mutex<LruCache<u64, Option<WrappedTextStructure>>>>);

impl BreaksLruCache {
    pub fn new(size: usize) -> Option<Self> {
        if size > 0 {
            Some(Self(Arc::new(Mutex::new(lru::LruCache::new(
                std::num::NonZeroUsize::new(size)
                    .unwrap_or(std::num::NonZeroUsize::new(1).unwrap()),
            )))))
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
    hash: u64,
}

impl WrappedTextStructure {
    pub fn new(lines: Vec<WrappedTextLine>, hash: u64) -> Self {
        Self { lines, hash }
    }

    pub fn as_svgr(
        &self,
        BreakLinesOpts {
            x,
            y,
            fill,
            font_size,
            font_family,
            font_weight,
            dominant_baseline,
            text_anchor,
            ..
        }: &BreakLinesOpts,
    ) -> Svgr {
        svgr_macro::svgr!(
         <text id={self.hash} x={x} y={y} fill={fill} font-size={font_size} font-family={font_family} font-weight={font_weight} dominant-baseline={dominant_baseline} text-anchor={text_anchor}>
           {
            self.lines.iter().map(|line| {
                svgr_macro::svgr!(
                   <tspan x={x} y={y} dx={line.dx} dy={line.dy.to_string()}>
                    {line.words.join(" ")}
                   </tspan>
                )
            }).collect::<Vec<_>>()
           }
        </text>
        )
    }

    pub fn occupied_height(&self) -> usize {
        let lines = &self.lines;

        if lines.len() < 2 {
            return 0;
        }

        let line_height = lines[1].dy - lines[0].dy;
        (lines.len() - 1) * line_height
    }
}

pub(crate) fn text_wrap_impl<'a, 'b>(
    value: &'b str,
    font_source: &'a (dyn FontSource<'a> + 'a),
    BreakLinesOpts {
        width,
        align,
        font_family,
        font_size,
        font_weight,
        font_style,
        font_stretch,
        line_height,
        ..
    }: BreakLinesOpts,
) -> Option<Vec<WrappedTextLine>> {
    let font_face = font_source.resolve_font(font_family, font_weight, font_style, font_stretch)?;

    let font_variant = font_face.font_variant(font_size)?;
    let space_width = match font_variant {
        crate::FontVariant::Monospaced(mono_width) => mono_width,
        crate::FontVariant::Other => font_face.resolve_char_width(font_size, ' ')?,
    };

    let resolve_word_width = |word: &str| -> Option<usize> {
        let raw_width = match font_variant {
            crate::FontVariant::Monospaced(mono_width) => word.len() * mono_width,
            crate::FontVariant::Other => word
                .chars()
                .map_while(|char| font_face.resolve_char_width(font_size, char))
                .sum(),
        };

        Some(raw_width)
    };

    let mut structure = vec![(vec![], 0usize)];
    for word in value.split_whitespace() {
        let word_width = resolve_word_width(word)?;
        let (last_line, last_line_width) = structure.last_mut()?;

        if *last_line_width + space_width + word_width > width {
            structure.push((vec![word.to_owned()], word_width));
        } else {
            if *last_line_width != 0 {
                *last_line_width += space_width;
            }

            last_line.push(word.to_owned());

            *last_line_width += word_width;
        }
    }

    Some(
        structure
            .into_iter()
            .enumerate()
            .map(|(index, (words, line_width))| {
                let dx = match align {
                    TextAlign::Left => 0,
                    TextAlign::Center => (width - line_width) / 2,
                    TextAlign::Right => width - line_width,
                };

                WrappedTextLine {
                    words,
                    width: line_width,
                    dx,
                    dy: (index as f32 * line_height * font_size as f32) as usize,
                }
            })
            .collect(),
    )
}
