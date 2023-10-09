#[derive(Debug)]
pub struct StaticFontFace<'a> {
    pub face: ttf_parser::Face<'a>,
}

impl<'a> StaticFontFace<'a> {
    pub fn from_bytes(bytes: &'a [u8]) -> crate::error::Result<StaticFontFace<'a>> {
        let face = ttf_parser::Face::parse(bytes, 0)?;

        Ok(Self { face })
    }
}
