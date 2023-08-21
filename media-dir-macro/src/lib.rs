mod parser;
use crate::parser::IncludeMediaDirInput;
use fframes_media_loaders::PreloadedAudioData;
use proc_macro::TokenStream;
use proc_macro2::{Literal, Span};
use quote::{quote, ToTokens};
use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    fs::File,
    path::{Path, PathBuf},
    time::SystemTime,
};
use syn::{parse_macro_input, Ident};

/// Embed the contents of a directory in your crate.
#[proc_macro]
pub fn include_media_dir(input: TokenStream) -> TokenStream {
    let fframes_crate_ident = resolve_fframes_crate_ident();

    let IncludeMediaDirInput {
        visibility,
        ident,
        path,
    } = parse_macro_input!(input as parser::IncludeMediaDirInput);

    let media_files = read_media_files_dir(&path);
    let fields = media_files
        .iter()
        .map(|field| MediaFile::to_type_tokens(field, &fframes_crate_ident));

    let instantiate_fields = media_files
        .iter()
        .map(|field| MediaFile::to_instantiate_tokens(field, &fframes_crate_ident))
        .collect::<Vec<_>>();

    quote! {
        #[derive(Debug)]
        #visibility struct #ident {
            #(#fields)*
        }

        impl #fframes_crate_ident::StaticMediaProvider for #ident {
            fn prepare() -> #fframes_crate_ident::error::Result<Self> {
                Ok(Self {
                    #(#instantiate_fields)*
                })
            }
        }
    }
    .into()
}

fn resolve_fframes_crate_ident() -> Ident {
    match proc_macro_crate::crate_name("fframes")
        .expect("fframes crate must be present in Cargo.toml")
    {
        proc_macro_crate::FoundCrate::Itself => Ident::new("crate", Span::call_site()),
        proc_macro_crate::FoundCrate::Name(name) => Ident::new(&name, Span::call_site()),
    }
}

#[derive(Debug)]
enum MediaVariant {
    Audio,
    Image,
    Font,
    Subtitles,
}

impl MediaVariant {
    fn from_extension(file_name: &str, path: &Path) -> Option<Self> {
        match path
            .extension() ?
            .to_str()?
        {
            "mp3" => Some(MediaVariant::Audio),
            "ttf" | "ttc" | "otf" | "otc" => Some(MediaVariant::Font),
            "jpeg" | "png" | "jpg" => Some(MediaVariant::Image),
            "vtt" => Some(MediaVariant::Subtitles),
            _ if file_name.starts_with('.') => None,
            _ => panic!("Can not parse the media file {}. File type is not supported, please remove all the unsupported files from the static media folder", path.display()),
        }
    }
}

#[derive(Debug)]
struct MediaFile {
    path: PathBuf,
    ident: syn::Ident,
    variant: MediaVariant,
}

impl MediaFile {
    fn to_type_tokens(&self, fframes_crate_ident: &syn::Ident) -> proc_macro2::TokenStream {
        let ident = &self.ident;
        let type_identifier = match self.variant {
            MediaVariant::Audio => quote! { #fframes_crate_ident::AudioData<'static> },
            MediaVariant::Image => quote! { #fframes_crate_ident::media::ImageData },
            MediaVariant::Subtitles => quote! { #fframes_crate_ident::media::Subtitles<'static> },
            MediaVariant::Font => {
                quote! { #fframes_crate_ident::media::StaticFontFace<'static> }
            }
        };

        quote! {
            pub #ident: #type_identifier,
        }
    }

    fn to_instantiate_tokens(&self, fframes_crate_ident: &syn::Ident) -> proc_macro2::TokenStream {
        let ident = &self.ident;
        let bytes_tokens = &self.inline_file(fframes_crate_ident);

        quote! {
            #ident: #bytes_tokens,
        }
    }

    fn inline_file(&self, fframes_crate_ident: &syn::Ident) -> proc_macro2::TokenStream {
        match self.variant {
            MediaVariant::Audio => {
                let PreloadedAudioData {
                    sample_rate,
                    samples,
                } = fframes_media_loaders::decode_mp3(File::open(&self.path).unwrap()).unwrap();

                let bytes = bytemuck::cast_slice::<i16, u8>(&samples);
                let literal = Literal::byte_string(bytes);

                quote! {
                    #fframes_crate_ident::AudioData::Preloaded(
                        #fframes_crate_ident::media::PreloadedAudioData {
                            samples: std::borrow::Cow::Borrowed(
                                #fframes_crate_ident::bytemuck::cast_slice::<u8, i16>(#literal)
                            ),
                            sample_rate: #sample_rate,
                        }
                    )
                }
            }
            MediaVariant::Font => {
                let file_bytes = read_file_bytes_as_tokens(&self.path);
                quote! {
                    #fframes_crate_ident::media::StaticFontFace::from_bytes(#file_bytes)?
                }
            }
            MediaVariant::Subtitles => {
                let file_str = std::fs::read_to_string(&self.path).unwrap();
                let file_str_literal = Literal::string(&file_str);

                quote! {
                    #fframes_crate_ident::media::Subtitles::parse(
                        #file_str_literal
                    )?
                }
            }
            MediaVariant::Image => {
                let file_name = self.path.file_name().and_then(|f| f.to_str()).unwrap();
                let file_bytes = std::fs::read(&self.path).unwrap();

                let platform_specific_identifier = create_image_identifier_for_platform(
                    fframes_crate_ident,
                    file_name,
                    &file_bytes,
                );
                let platform_specific_identifier_wasm = create_image_identifier_for_platform_wasm(
                    fframes_crate_ident,
                    file_name,
                    &file_bytes,
                );

                quote! {
                    #fframes_crate_ident::media::ImageData {
                        filename: #file_name.to_owned(),
                        #[cfg(not(target_arch = "wasm32"))]
                        #platform_specific_identifier,
                        #[cfg(target_arch = "wasm32")]
                        #platform_specific_identifier_wasm
                    }
                }
            }
        }
    }
}

fn create_image_identifier_for_platform(
    fframes_crate_ident: &syn::Ident,
    file_name: &str,
    bytes: &[u8],
) -> impl ToTokens {
    let fframes_media_loaders::PreloadedImageData {
        height,
        width,
        mime,
        data,
    } = &fframes_media_loaders::decode_image(file_name, &bytes).unwrap();

    let bytes_literal = Literal::byte_string(data);

    quote! {
    image: std::sync::Arc::new(
        #fframes_crate_ident::usvgr::PreloadedImageData {
            data: std::borrow::Cow::Borrowed(
                #bytes_literal
            ),
            width: #width,
            height: #height,
            mime: #mime.to_owned(),
        })
    }
}

fn create_image_identifier_for_platform_wasm(
    _fframes_crate_ident: &syn::Ident,
    file_name: &str,
    bytes: &[u8],
) -> impl ToTokens {
    use base64::Engine;
    let encoded: String = base64::engine::general_purpose::STANDARD_NO_PAD.encode(bytes);
    let extension = file_name.split('.').last();

    let mime_type = match extension {
        Some("png") => "image/png",
        Some("jpg") => "image/jpeg",
        Some("jpeg") => "image/jpeg",
        _ => panic!("File {file_name} is not a valid image file"),
    };

    let base64_web_png = format!("data:{mime_type};base64,{encoded}");
    quote! {
        base64_data: std::borrow::Cow::Borrowed(#base64_web_png)
    }
}

fn read_media_files_dir(path: &Path) -> Vec<MediaFile> {
    let children = read_dir(path).unwrap_or_else(|e| {
        panic!(
            "Unable to read the entries in \"{}\": {}",
            path.display(),
            e
        )
    });

    let mut media_files = Vec::new();

    for child in children {
        if child.is_file() {
            let file_name = child
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or_else(|| {
                    panic!("Unable to get the file name from \"{}\"", child.display())
                });

            if let Some(variant) = MediaVariant::from_extension(file_name, &child) {
                media_files.push(MediaFile {
                    ident: syn::Ident::new(&to_valid_rust_identifier(file_name), Span::call_site()),
                    variant,
                    path: child,
                });
            }
        } else {
            panic!("\"{}\" is neither a file nor a directory", child.display());
        }
    }

    media_files
}

fn to_valid_rust_identifier(input: &str) -> String {
    // Remove invalid characters
    let cleaned = input
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();

    // Ensure the identifier starts with a letter or underscore
    let mut result = cleaned;
    if let Some(first_char) = result.chars().next() {
        if !first_char.is_alphabetic() && first_char != '_' {
            result.insert(0, '_');
        }
    }

    result
}

fn read_file_bytes_as_tokens(path: &Path) -> proc_macro2::TokenStream {
    let abs = path
        .canonicalize()
        .unwrap_or_else(|e| panic!("failed to resolve \"{}\": {}", path.display(), e));

    match abs.to_str() {
        Some(abs) => quote!(include_bytes!(#abs)),
        None => {
            let contents = read_file(path);
            let literal = Literal::byte_string(&contents);
            quote!(#literal)
        }
    }
}

fn metadata(path: &Path) -> Option<proc_macro2::TokenStream> {
    fn to_unix(t: SystemTime) -> u64 {
        t.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
    }

    if !cfg!(feature = "metadata") {
        return None;
    }

    let meta = path.metadata().ok()?;
    let accessed = meta.accessed().map(to_unix).ok()?;
    let created = meta.created().map(to_unix).ok()?;
    let modified = meta.modified().map(to_unix).ok()?;

    Some(quote! {
        include_dir::Metadata::new(
            std::time::Duration::from_secs(#accessed),
            std::time::Duration::from_secs(#created),
            std::time::Duration::from_secs(#modified),
        )
    })
}

/// Make sure that paths use the same separator regardless of whether the host
/// machine is Windows or Linux.
fn normalize_path(root: &Path, path: &Path) -> String {
    let stripped = path
        .strip_prefix(root)
        .expect("Should only ever be called using paths inside the root path");
    let as_string = stripped.to_string_lossy();

    as_string.replace('\\', "/")
}

fn read_dir(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    if !dir.is_dir() {
        panic!("\"{}\" is not a directory", dir.display());
    }

    track_path(dir);

    let mut paths = Vec::new();

    for entry in dir.read_dir()? {
        let entry = entry?;
        paths.push(entry.path());
    }

    paths.sort();

    Ok(paths)
}

fn read_file(path: &Path) -> Vec<u8> {
    track_path(path);
    std::fs::read(path).unwrap_or_else(|e| panic!("Unable to read \"{}\": {}", path.display(), e))
}

#[derive(Debug, PartialEq)]
struct UnableToParseVariable {
    rest: String,
}

impl Error for UnableToParseVariable {}

impl Display for UnableToParseVariable {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Unable to parse a variable from \"{}\"", self.rest)
    }
}

fn parse_identifier(text: &str) -> Option<(&str, &str)> {
    let mut calls = 0;

    let (head, tail) = take_while(text, |c| {
        calls += 1;

        match c {
            '_' => true,
            letter if letter.is_ascii_alphabetic() => true,
            digit if digit.is_ascii_digit() && calls > 1 => true,
            _ => false,
        }
    });

    if head.is_empty() {
        None
    } else {
        Some((head, tail))
    }
}

fn take_while(s: &str, mut predicate: impl FnMut(char) -> bool) -> (&str, &str) {
    let mut index = 0;

    for c in s.chars() {
        if predicate(c) {
            index += c.len_utf8();
        } else {
            break;
        }
    }

    s.split_at(index)
}

#[cfg(feature = "nightly")]
fn get_env(variable: &str) -> Option<String> {
    proc_macro::tracked_env::var(variable).ok()
}

#[cfg(not(feature = "nightly"))]
fn get_env(variable: &str) -> Option<String> {
    std::env::var(variable).ok()
}

fn track_path(_path: &Path) {
    #[cfg(feature = "nightly")]
    proc_macro::tracked_path::path(_path.to_string_lossy());
}
