use std::path::PathBuf;
use syn::{
    parse::{Parse, ParseStream},
    token::{Comma, Struct},
    LitStr, Result,
};

#[derive(Debug)]
pub struct IncludeMediaDirInput {
    pub path: PathBuf,
    pub ident: syn::Ident,
    pub visibility: syn::Visibility,
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

        Ok(Self {
            path,
            ident,
            visibility,
        })
    }
}
