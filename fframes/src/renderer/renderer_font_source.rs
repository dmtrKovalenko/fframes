use crate::usvgr::fontdb::{self, Family, Query, Weight};
use crate::{FontFace, FontStretch, FontStyle};
use std::path::Path;
use std::sync::Arc;

pub(crate) struct RendererFont<'a> {
    pub(crate) index: u32,
    // We can not use the ttf_parser::Face directly because it can be a file
    // which in theory not a big deal because "parse" here mostly not doing any data transformations
    pub(crate) data: Arc<dyn AsRef<[u8]> + Send + Sync + 'a>,
}

impl std::fmt::Debug for RendererFont<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RendererFont")
            .field("index", &self.index)
            .finish()
    }
}

impl<'a> crate::FontFace<'a> for RendererFont<'a> {
    fn is_monospaced(&self) -> Option<bool> {
        let face = crate::ttf_parser::Face::parse(self.data.as_ref().as_ref(), self.index).ok()?;
        Some(face.is_monospaced())
    }

    fn resolve_char_width(&self, font_size: usize, char: char) -> Option<usize> {
        let face = crate::ttf_parser::Face::parse(self.data.as_ref().as_ref(), self.index).ok()?;
        let glyph_id = face.glyph_index(char)?;

        Some(
            font_size * face.tables().hmtx?.advance(glyph_id)? as usize
                / face.units_per_em() as usize,
        )
    }
}

#[derive(Debug)]
pub struct RendererFontSource {
    pub(crate) fontdb: fontdb::Database,
}

impl RendererFontSource {
    pub fn as_db_ref(&self) -> &fontdb::Database {
        &self.fontdb
    }

    /// Read the fonts from the folder
    pub fn read_folder(&mut self, path: impl AsRef<Path>) {
        self.fontdb.load_fonts_dir(path);
    }

    /// Loads the fonts installed on the system (see `RenderOptions::load_system_fonts`).
    pub fn load_system_fonts(&mut self) {
        self.fontdb.load_system_fonts();
    }
}

impl<'a> crate::FontSource<'a> for RendererFontSource {
    fn resolve_font(
        &'a self,
        font_name: &str,
        font_weight: u16,
        font_style: FontStyle,
        font_stretch: FontStretch,
    ) -> Option<Box<dyn FontFace<'a> + 'a>> {
        let font_id = self.fontdb.query(&Query {
            families: &[Family::Name(font_name)],
            weight: Weight(font_weight),
            style: match font_style {
                FontStyle::Normal => fontdb::Style::Normal,
                FontStyle::Italic => fontdb::Style::Italic,
                FontStyle::Oblique => fontdb::Style::Oblique,
            },
            stretch: match font_stretch {
                FontStretch::ExtraCondensed => fontdb::Stretch::ExtraCondensed,
                FontStretch::UltraCondensed => fontdb::Stretch::UltraCondensed,
                FontStretch::Condensed => fontdb::Stretch::Condensed,
                FontStretch::SemiCondensed => fontdb::Stretch::SemiCondensed,
                FontStretch::Normal => fontdb::Stretch::Normal,
                FontStretch::SemiExpanded => fontdb::Stretch::SemiExpanded,
                FontStretch::Expanded => fontdb::Stretch::Expanded,
                FontStretch::ExtraExpanded => fontdb::Stretch::ExtraExpanded,
                FontStretch::UltraExpanded => fontdb::Stretch::UltraExpanded,
            },
        })?;

        let (source, index) = self.fontdb.face_source(font_id)?;
        let font_data = match source {
            fontdb::Source::Binary(data) | fontdb::Source::SharedFile(_, data) => data.clone(),
            fontdb::Source::File(path) => Arc::new(std::fs::read(path).ok()?),
        };

        Some(Box::new(RendererFont {
            index,
            data: font_data,
        }))
    }

    fn measure_text(&self, font: &crate::FontQuery, text: &str) -> Option<f32> {
        crate::measure_text_width(&self.fontdb, font, text)
    }

    fn add_font(&mut self, _filename: String, font_data: Arc<dyn AsRef<[u8]> + Sync + Send>) {
        self.fontdb
            .load_font_source(fontdb::Source::Binary(font_data.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{text_fit_impl, text_wrap_impl};
    use crate::{BreakLinesOpts, FontQuery, FontSource, TextOverflow};

    const INTER: &[u8] = include_bytes!("../../../examples/arm/media/Inter_18pt-ExtraBold.ttf");
    const SOFIA: &[u8] = include_bytes!(
        "../../../examples/teej-podcast/dynamic_media/SofiaSansSemiCondensed-VariableFont_wght.ttf"
    );
    // Noto Sans Arabic (OFL) cut down to the glyphs of "سلام".
    const ARABIC: &[u8] =
        include_bytes!("../../../examples/text-width/media/NotoSansArabic-subset.ttf");

    fn fonts(files: &[&[u8]]) -> RendererFontSource {
        let mut fontdb = fontdb::Database::new();
        for data in files {
            fontdb.load_font_data(data.to_vec());
        }
        RendererFontSource { fontdb }
    }

    fn font(family: &str, size: usize, weight: u16) -> FontQuery<'_> {
        FontQuery {
            family,
            size,
            weight,
            ..Default::default()
        }
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.5,
            "measured {actual}, expected {expected}"
        );
    }

    #[test]
    fn text_fit_keeps_kerned_text_that_fits() {
        let fonts = fonts(&[INTER]);
        let text = "AVAVAVAV";
        assert_near(
            fonts
                .measure_text(&font("Inter 18pt", 100, 800), text)
                .unwrap(),
            546.0,
        );
        let fitted = text_fit_impl(
            font("Inter 18pt", 100, 800),
            text,
            550,
            TextOverflow::Ellipsis,
            &fonts,
        );
        assert_eq!(fitted.as_deref(), Some(text));
    }

    #[test]
    fn text_fit_truncates_text_wider_than_its_box() {
        let fonts = fonts(&[INTER]);
        let query = font("Inter 18pt", 46, 800);
        let text = "The Byzantine Greeks call Eastern Rome";
        assert!(fonts.measure_text(&query, text).unwrap() > 885.0);
        let fitted = text_fit_impl(query, text, 885, TextOverflow::Ellipsis, &fonts).unwrap();
        assert!(fitted.ends_with('…'), "{fitted}");
        assert!(fonts.measure_text(&query, &fitted).unwrap() <= 885.0);
    }

    #[test]
    fn wrapped_lines_are_measured_joined() {
        let fonts = fonts(&[INTER]);
        let query = font("Inter 18pt", 16, 800);
        let wrapped = text_wrap_impl(
            0,
            "a a a a a a a a a a",
            &fonts,
            BreakLinesOpts::<usize, usize> {
                font: query,
                width: 117,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(wrapped.lines.len() > 1);
        for line in &wrapped.lines {
            let rendered = fonts.measure_text(&query, &line.words.join(" ")).unwrap();
            assert!(rendered <= 117.0, "{:?} renders {rendered} px", line.words);
            assert_eq!(line.width, rendered.round() as usize);
        }
    }

    #[test]
    fn measuring_follows_the_weight_axis() {
        let fonts = fonts(&[SOFIA]);
        let family = "Sofia Sans Semi Condensed";
        assert_near(
            fonts
                .measure_text(&font(family, 40, 100), "Hamburgefonts")
                .unwrap(),
            247.9,
        );
        assert_near(
            fonts
                .measure_text(&font(family, 40, 900), "Hamburgefonts")
                .unwrap(),
            266.5,
        );
    }

    #[test]
    fn missing_glyphs_are_measured_in_the_fallback_font() {
        let fonts = fonts(&[INTER, ARABIC]);
        let fallback = fonts
            .measure_text(&font("Inter 18pt", 40, 800), "سلام")
            .unwrap();
        let direct = fonts
            .measure_text(&font("Noto Sans Arabic", 40, 400), "سلام")
            .unwrap();
        assert_near(fallback, direct);
    }

    #[test]
    fn mixed_direction_text_is_measured_per_run() {
        let fonts = fonts(&[INTER, ARABIC]);
        let query = font("Inter 18pt", 40, 800);
        let latin = fonts.measure_text(&query, "AV ").unwrap();
        let arabic = fonts.measure_text(&query, "سلام").unwrap();
        assert_near(
            fonts.measure_text(&query, "AV سلام").unwrap(),
            latin + arabic,
        );
    }
}
