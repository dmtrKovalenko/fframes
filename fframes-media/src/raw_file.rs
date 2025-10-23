use std::borrow::Cow;
use std::path::PathBuf;
use std::{fs, io};

#[derive(Debug)]
pub enum RawMediaFile {
    Stream(PathBuf),
    Data(Vec<u8>),
}

impl RawMediaFile {
    pub fn read_bytes(&self) -> io::Result<Cow<'_, [u8]>> {
        match self {
            RawMediaFile::Stream(path) => Ok(fs::read(path)?.into()),
            RawMediaFile::Data(data) => Ok(Cow::Borrowed(data)),
        }
    }
}
