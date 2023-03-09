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
    pub line_height: f32,
    pub font_family: &'a str,
    pub font_size: usize,
    pub x: &'a str,
    pub y: &'a str,
    pub align: TextAlign,
    pub font_weight: u16,
    pub fill: &'a str,
    pub font_style: FontStyle,
    pub font_stretch: FontStretch,
    pub dominant_baseline: &'a str,
    pub text_anchor: &'a str,
}

impl BreakLinesOpts<'_> {
    pub(crate) fn hash_with_value(&self, value: &str) -> u64 {
        let mut s = DefaultHasher::new();
        value.hash(&mut s);
        self.hash(&mut s);

        s.finish()
    }

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
            ..
        } = self;

        svgr!(
          <text x={x} y={y} fill={fill} font-size={font_size} font-family={font_family} font-weight={font_weight} dominant-baseline={dominant_baseline} text-anchor={text_anchor}>
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
pub struct BreaksLruCache(pub(crate) Arc<Mutex<LruCache<u64, Option<Vec<WrappedTextLine>>>>>);

impl BreaksLruCache {
    pub fn new(size: usize) -> Option<Self> {
        if size > 0 {
            Some(Self(Arc::new(Mutex::new(lru::LruCache::new(
                std::num::NonZeroUsize::new(size).unwrap(),
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

impl WrappedTextLine {
    pub fn as_svgr(
        lines: &[Self],
        hash: u64,
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
         <text id={hash} x={x} y={y} fill={fill} font-size={font_size} font-family={font_family} font-weight={font_weight} dominant-baseline={dominant_baseline} text-anchor={text_anchor}>
           {
            lines.iter().map(|line| {
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
        let (last_line, last_line_width) = structure.last_mut().unwrap();

        if *last_line_width + space_width + word_width > width {
            structure.push((vec![word.to_owned()], word_width));
        } else {
            if *last_line_width != 0 {
                // last_line.push(' ');
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
