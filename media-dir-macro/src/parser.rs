use std::path::PathBuf;
use syn::{
    LitStr, Result,
    parse::{Parse, ParseStream},
    token::{Comma, Struct},
};

#[derive(Debug)]
pub struct IncludeMediaDirInput {
    pub path: PathBuf,
    pub ident: syn::Ident,
    pub visibility: syn::Visibility,
    /// Keep both channels of stereo audio files instead of mixing them down to mono.
    pub stereo: bool,
}

impl Parse for IncludeMediaDirInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let visibility = input.parse::<syn::Visibility>()?;
        input.parse::<Struct>()?;
        let ident = input.parse::<syn::Ident>()?;
        input.parse::<Comma>()?;
        let path_literal = input.parse::<LitStr>()?;

        let path = std::fs::canonicalize(path_literal.value())
            .map_err(|err| syn::Error::new(path_literal.span(), format!("{err}\nMedia dir should be relative cargo project root (the project or workspace Cargo.toml directory)")))?;

        let mut stereo = false;
        if input.parse::<Option<Comma>>()?.is_some() && !input.is_empty() {
            let option = input.parse::<syn::Ident>()?;
            if option != "stereo" {
                return Err(syn::Error::new(
                    option.span(),
                    "unknown option, the only one is `stereo` (keep both channels of stereo audio)",
                ));
            }
            stereo = true;
            input.parse::<Option<Comma>>()?;
        }

        Ok(Self {
            path,
            ident,
            visibility,
            stereo,
        })
    }
}
