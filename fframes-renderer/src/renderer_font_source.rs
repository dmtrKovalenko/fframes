use fframes::{self, FontFace, FontStretch, FontStyle};
use std::sync::Arc;
use usvgr_text_layout::fontdb::{self, Family, Query, Weight};

pub(crate) struct RendererFont<'a> {
    pub(crate) index: u32,
    pub(crate) data: Arc<dyn AsRef<[u8]> + Send + Sync + 'a>,
}

impl std::fmt::Debug for RendererFont<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RendererFont")
            .field("index", &self.index)
            .finish()
    }
}

impl<'a> fframes::FontFace<'a> for RendererFont<'a> {
    fn is_monospaced(&self) -> Option<bool> {
        let face =
            fframes::ttf_parser::Face::parse(self.data.as_ref().as_ref(), self.index).ok()?;
        Some(face.is_monospaced())
    }

    fn resolve_char_width(&self, font_size: usize, char: char) -> Option<usize> {
        let face =
            fframes::ttf_parser::Face::parse(self.data.as_ref().as_ref(), self.index).ok()?;
        let glyph_id = face.glyph_index(char)?;

        Some(
            font_size * face.tables().hmtx?.advance(glyph_id)? as usize
                / face.units_per_em() as usize,
        )
    }
}

#[derive(Debug)]
pub(crate) struct RendererFontSource<'a> {
    pub(crate) fontdb: &'a usvgr_text_layout::fontdb::Database,
}

impl<'a> fframes::FontSource<'a> for RendererFontSource<'a> {
    fn resolve_font(
        &'a self,
        font_name: &str,
        font_weight: u16,
        font_style: FontStyle,
        font_stretch: FontStretch,
    ) -> Option<Box<dyn FontFace + 'a>> {
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
        let data_ref = match source {
            fontdb::Source::Binary(data) => data.clone(),
            fontdb::Source::File(file) => Arc::new(std::fs::read(file).ok()?),
            fontdb::Source::SharedFile(_, data) => data.clone(),
        };

        Some(Box::new(RendererFont {
            index,
            data: data_ref,
        }))
    }
}
