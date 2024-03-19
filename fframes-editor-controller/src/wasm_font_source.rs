use fframes::{
    ttf_parser::{self},
    FontStretch, FontStyle,
};
use std::{borrow::Cow, collections::HashMap, sync::Arc};
use wasm_bindgen::{prelude::wasm_bindgen, JsValue};

#[derive(Debug, PartialEq, Eq, Hash, Clone, fframes::serde::Serialize)]
#[serde(crate = "fframes::serde")] // https://github.com/serde-rs/serde/issues/1465
pub struct FaceInfo {
    /// font name as bytes (we are in wasm so no utf8 strings parsing in runtime)
    name: String,
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
        let name_bytes = parse_family_name(face.raw_face())?;
        let name = String::from_utf8_lossy(&name_bytes).to_string();

        let face_info = FaceInfo {
            name,
            weight: face.weight().to_number(),
            stretch: face.width().into(),
            style: face.style().into(),
        };

        match (&data, filename) {
            (Cow::Borrowed(borrowed_data), Some(filename)) => {
                self.static_fonts.push(StaticFontFace {
                    filename,
                    data: borrowed_data,
                    info: face_info.clone(),
                })
            }
            _ => (),
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
    ) -> Option<Box<dyn fframes::FontFace + 'a>> {
        let font = self.data.get(&FaceInfo {
            name: font_name.to_string(),
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
        unimplemented!("Adding fonts for wasm font source must be done through WasmFontSource::insert_font api")
    }
}

pub fn parse_family_name(raw_face: &ttf_parser::RawFace) -> Option<Vec<u8>> {
    const NAME_TAG: ttf_parser::Tag = ttf_parser::Tag::from_bytes(b"name");
    let name_data = raw_face.table(NAME_TAG)?;
    let name_table = ttf_parser::name::Table::parse(name_data)?;

    name_table.names.into_iter().find_map(|name| {
        (name.name_id == ttf_parser::name_id::FAMILY).then_some(
            name.name
                .iter()
                .filter_map(|b| if *b != 0 { Some(*b) } else { None })
                .collect(),
        )
    })
}
