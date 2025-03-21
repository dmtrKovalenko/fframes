use fframes::{
    FontStretch, FontStyle,
    ttf_parser::{self, PlatformId},
};
use std::{borrow::Cow, collections::HashMap, sync::Arc};
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

#[derive(Debug, PartialEq, Eq, Hash, Clone, fframes::serde::Serialize)]
#[serde(crate = "fframes::serde")] // https://github.com/serde-rs/serde/issues/1465
pub struct FaceInfo {
    /// font name as bytes (we are in wasm so no utf8 strings parsing in runtime)
    name: String,
    name_bytes: Vec<u8>,
    stretch: fframes::FontStretch,
    weight: u16,
    style: fframes::FontStyle,
}

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct StaticFontFace {
    data: &'static [u8],
    filename: &'static str,
    info: FaceInfo,
}

#[wasm_bindgen]
impl StaticFontFace {
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(self.data)
    }

    #[wasm_bindgen(getter)]
    pub fn info(&self) -> JsValue {
        serde_wasm_bindgen::to_value(&self.info).unwrap()
    }

    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.filename.to_string()
    }
}

#[derive(Debug)]
/// The fontdb implementation for wasm, which is a hashmap of the font options used in css queries (name, stretch, weight and style) to the raw font data coming from array buffer.
pub struct WasmFontSource {
    pub static_fonts: Vec<StaticFontFace>,
    pub data: HashMap<FaceInfo, Cow<'static, [u8]>>,
}

impl WasmFontSource {
    pub fn new() -> Self {
        Self {
            static_fonts: vec![],
            data: HashMap::new(),
        }
    }

    pub fn insert_font(
        &mut self,
        data: Cow<'static, [u8]>,
        filename: Option<&'static str>,
    ) -> Option<FaceInfo> {
        let face = ttf_parser::Face::parse(&data, 0).ok()?;
        let name = parse_family_name(face.raw_face())?;

        let face_info = FaceInfo {
            name: name.to_string(),
            name_bytes: name.as_bytes().to_vec(),
            weight: face.weight().to_number(),
            stretch: face.width().into(),
            style: face.style().into(),
        };

        if let (Cow::Borrowed(borrowed_data), Some(filename)) = (&data, filename) {
            self.static_fonts.push(StaticFontFace {
                filename,
                data: borrowed_data,
                info: face_info.clone(),
            })
        }

        self.data.insert(face_info.clone(), data);
        Some(face_info)
    }
}

impl Default for WasmFontSource {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct WasmFontFace<'a> {
    pub name: String,
    pub face: ttf_parser::Face<'a>,
}

impl<'a> fframes::FontFace<'a> for WasmFontFace<'a> {
    fn is_monospaced(&self) -> Option<bool> {
        Some(self.face.is_monospaced())
    }

    fn resolve_char_width(&self, font_size: usize, char: char) -> Option<usize> {
        let glyph_id = self.face.glyph_index(char)?;

        Some(
            font_size * self.face.tables().hmtx?.advance(glyph_id)? as usize
                / self.face.units_per_em() as usize,
        )
    }
}

impl<'a> fframes::FontSource<'a> for WasmFontSource {
    fn resolve_font(
        &'a self,
        font_name: &str,
        font_weight: u16,
        font_style: FontStyle,
        font_stretch: FontStretch,
    ) -> Option<Box<dyn fframes::FontFace<'a> + 'a>> {
        let font = self.data.get(&FaceInfo {
            name_bytes: font_name.as_bytes().to_vec(),
            name: font_name.to_owned(),
            stretch: font_stretch,
            weight: font_weight,
            style: font_style,
        })?;

        let face = ttf_parser::Face::parse(font, 0).ok()?;

        Some(Box::new(WasmFontFace {
            face,
            name: font_name.to_owned(),
        }))
    }

    fn add_font(&mut self, _filename: String, _font_data: Arc<dyn AsRef<[u8]> + Sync + Send>) {
        unimplemented!(
            "Adding fonts for wasm font source must be done through WasmFontSource::insert_font api"
        )
    }
}

pub fn parse_family_name<'a>(raw_face: &'a ttf_parser::RawFace) -> Option<Cow<'a, str>> {
    const NAME_TAG: ttf_parser::Tag = ttf_parser::Tag::from_bytes(b"name");
    let name_data = raw_face.table(NAME_TAG)?;
    let name_table = ttf_parser::name::Table::parse(name_data)?;

    let mut typographic_family = None;
    let mut family = None;
    let mut wws_family = None;

    for name in name_table.names.into_iter() {
        let clean_name = match (name.platform_id, name.encoding_id) {
            // Unicode platform encodings
            (PlatformId::Unicode, _) |
            // Windows platform encodings
            // Encoding ID 1 = UTF-16BE
            (PlatformId::Windows, 1) |
            // Encoding ID 10 = UTF-16BE (full Unicode range)
            (PlatformId::Windows, 10) => decode_utf16be(name.name).unwrap_or_else(|| String::from_utf8_lossy(name.name)),
            // Macintosh platform encodings
            // Encoding ID 0 = Roman (usually ASCII or Mac Roman)
            (PlatformId::Macintosh, 0) |
            // Other Mac encodings may need specific handling
            // ISO platform encodings (rarely used)
            (PlatformId::Iso, _) => String::from_utf8_lossy(name.name),
            // Default fallback - try UTF-8 first then decode as utf16be
            _ => String::from_utf8(name.name.to_vec()).map(Cow::Owned).ok().or_else(|| decode_utf16be(name.name))?,
        };

        match name.name_id {
            ttf_parser::name_id::TYPOGRAPHIC_FAMILY => typographic_family = Some(clean_name),
            ttf_parser::name_id::FAMILY => family = Some(clean_name),
            ttf_parser::name_id::WWS_FAMILY => wws_family = Some(clean_name),
            _ => continue,
        }
    }

    typographic_family.or(family).or(wws_family)
}

fn decode_utf16be(data: &[u8]) -> Option<Cow<'_, str>> {
    // Make sure we have an even number of bytes
    if data.len() % 2 != 0 {
        return None;
    }

    // Convert bytes to UTF-16BE code units
    let utf16_units: Vec<u16> = data
        .chunks_exact(2)
        .map(|chunk| ((chunk[0] as u16) << 8) | (chunk[1] as u16))
        .collect();

    // Decode UTF-16BE to a Rust String
    match String::from_utf16(&utf16_units) {
        Ok(s) => Some(Cow::Owned(s)),
        Err(_) => None,
    }
}
