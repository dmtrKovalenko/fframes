mod parser;
use crate::parser::IncludeMediaDirInput;
use proc_macro::TokenStream;
use proc_macro2::{Literal, Span};
use quote::{quote, ToTokens};
use std::{
    error::Error,
    path::{Path, PathBuf},
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

    let audio_handle_tokens = media_files
        .iter()
        .filter_map(|file| {
            file.to_manual_selection_match_variant(MediaVariant::Audio, &fframes_crate_ident)
        })
        .collect::<Vec<_>>();
    let images_handle_tokens = media_files
        .iter()
        .filter_map(|file| {
            file.to_manual_selection_match_variant(MediaVariant::Image, &fframes_crate_ident)
        })
        .collect::<Vec<_>>();
    let subtitles_handle_tokens = media_files
        .iter()
        .filter_map(|file| {
            file.to_manual_selection_match_variant(MediaVariant::Subtitles, &fframes_crate_ident)
        })
        .collect::<Vec<_>>();

    // These are the values we give to the renderer/editor to access all of the media dynamically.
    let populate_fonts_expressions = media_files
        .iter()
        .filter_map(
            |MediaFile {
                 variant,
                 ident,
                 filename,
                 ..
             }| {
                matches!(variant, MediaVariant::Font).then_some(quote! {
                    font_source.add_font(String::from(#filename), std::sync::Arc::new(self.#ident));
                })
            },
        )
        .collect::<Vec<_>>();
    let populate_images_expressions = media_files
        .iter()
        .filter_map(|MediaFile { variant, ident, filename, .. }| {
            matches!(variant, MediaVariant::Image).then_some(quote! {
                image_data.insert(String::from(#filename), std::sync::Arc::clone(&self.#ident.image));
            })
        })
        .collect::<Vec<_>>();

    // These are used mainly for editor and provides direct access to all the static media as
    // 'static borrow which significantly simplifies wasm code
    let audio_identifiers = media_files
        .iter()
        .filter_map(
            |MediaFile {
                 variant,
                 ident,
                 filename,
                 ..
             }| {
                matches!(variant, MediaVariant::Audio)
                    .then_some(quote! { ( &self.#ident, #filename )})
            },
        )
        .collect::<Vec<_>>();
    let font_identifiers = media_files
        .iter()
        .filter_map(
            |MediaFile {
                 variant,
                 ident,
                 filename,
                 ..
             }| {
                matches!(variant, MediaVariant::Font)
                    .then_some(quote! { ( &self.#ident, #filename )})
            },
        )
        .collect::<Vec<_>>();

    quote! {
        // This is a workaround to force include_bytes which is the way we inline bytes to force
        // the alignment of the plain &'static [u8] to match alignment of f32 which we need for audio
        //
        // More info here https://jack.wrenn.fyi/blog/include-transmute/
        #[repr(C)]
        struct FFramesForceAlignTo<Align, Bytes: ?Sized> {
            pub _align: [Align; 0],
            pub bytes: Bytes
        }

        #[derive(Debug)]
        #visibility struct #ident {
            #(#fields)*
        }

        impl #ident {
            // we have this only to avoid the requirement of importing the trait 
            pub fn new() -> #fframes_crate_ident::error::Result<Self> {
                Ok(Self {
                    #(#instantiate_fields)*
                })
            }
        }

        impl #fframes_crate_ident::StaticMediaProvider<'_> for #ident {
            fn prepare() -> #fframes_crate_ident::error::Result<Self> {
                Ok(Self {
                    #(#instantiate_fields)*
                })
            }

            fn get_all_audio_data(&self) -> Option<Vec<(&#fframes_crate_ident::AudioData, &str)>> {
                Some(vec![#(#audio_identifiers),*])
            }

            fn get_all_font_data(&self) -> Option<Vec<(&[u8], &str)>> {
                Some(vec![#(#font_identifiers),*])
            }
        }

        impl #fframes_crate_ident::MediaProvider<'_> for #ident {
            fn resolve_audio(&self, name: &str) -> Option<&#fframes_crate_ident::AudioData> {
                match name {
                    #(#audio_handle_tokens)*
                    _ => None
                }
            }

            fn resolve_image(&self, name: &str) -> Option<&#fframes_crate_ident::media::ImageData> {
                match name {
                    #(#images_handle_tokens)*
                    _ => None
                }
            }

            #[cfg(not(target_arch = "wasm32"))]
            fn resolve_subtitles(&self, name: &str) -> Option<&#fframes_crate_ident::media::Subtitles> {
                match name {
                    #(#subtitles_handle_tokens)*
                    _ => None
                }
            }

            #[cfg(target_arch = "wasm32")]
            fn resolve_subtitles(&self, name: &str) -> Option<&#fframes_crate_ident::media::Subtitles> {
                fframes::log!("Warning: subtitles dynamic resolution in editor mode is not supported, please use the direct field of the autogenerated inline media struct.");

                None
            }

            fn populate_font_source(&self, font_source: &mut dyn #fframes_crate_ident::FontSource) {
               #(#populate_fonts_expressions);*
            }

            #[cfg(not(target_arch = "wasm32"))]
            fn populate_image_source(&self, image_data: &mut std::collections::HashMap<String, std::sync::Arc<#fframes_crate_ident::media::PreloadedImageData>>) {
               #(#populate_images_expressions);*
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

#[derive(Debug, PartialEq)]
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
    filename: String,
    ident: syn::Ident,
    variant: MediaVariant,
}

impl MediaFile {
    fn to_type_tokens(&self, fframes_crate_ident: &syn::Ident) -> proc_macro2::TokenStream {
        let ident = &self.ident;
        let type_identifier = match self.variant {
            MediaVariant::Audio => quote! { #fframes_crate_ident::AudioData<'static> },
            MediaVariant::Image => quote! { #fframes_crate_ident::media::ImageData },
            // In wasm or dynamic media provider we load the Subtitles type, but in the static
            // compilation we expose direct Vtt struct that does not allocate.
            MediaVariant::Subtitles => quote! { #fframes_crate_ident::media::Vtt<'static> },
            MediaVariant::Font => {
                quote! { &'static [u8] }
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
                let bytes = std::fs::read(&self.path).unwrap();
                let fframes_media_loaders::PreloadedAudioData {
                    samples,
                    sample_rate,
                } = fframes_media_loaders::PreloadedAudioData::decode_buffer(
                    None,
                    &self.path.to_string_lossy(),
                    &bytes,
                )
                .unwrap();

                let bytes = bytemuck::cast_slice::<f32, u8>(&samples);
                let literal = Literal::byte_string(bytes);

                quote! {
                    #fframes_crate_ident::AudioData::Preloaded(
                        #fframes_crate_ident::media::PreloadedAudioData {
                            samples: {
                                static ALIGNED_LITERAL: &FFramesForceAlignTo<f32, [u8]> = &FFramesForceAlignTo {
                                    _align: [],
                                    bytes: *#literal
                                };

                                std::borrow::Cow::Borrowed(
                                    #fframes_crate_ident::bytemuck::cast_slice::<u8, f32>(&ALIGNED_LITERAL.bytes)
                                )
                            },
                            sample_rate: #sample_rate,
                        }
                    )
                }
            }
            // the font file is basically and adhoc byte stream that is parsed on read at runtime
            MediaVariant::Font => read_file_bytes_as_tokens(&self.path),
            MediaVariant::Subtitles => {
                let file_str = std::fs::read_to_string(&self.path).unwrap();
                let file_str_literal = Literal::string(&file_str);

                quote! {
                    #fframes_crate_ident::media::Vtt::parse(
                        #file_str_literal
                    ).map_err(#fframes_crate_ident::media::FFramesMediaError::VttError)?
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

    pub fn to_manual_selection_match_variant(
        &self,
        target_variant: MediaVariant,
        fframes_crate_ident: &syn::Ident,
    ) -> Option<impl ToTokens> {
        let MediaFile {
            variant,
            ident,
            filename,
            ..
        } = self;

        if variant == &target_variant && target_variant == MediaVariant::Font {
            return Some(quote! {
                #filename => Some(Box::new(self.#ident) as Box<dyn #fframes_crate_ident::FontFace>),
            });
        }

        (variant == &target_variant).then_some(quote! {
             #filename => Some(&self.#ident),
        })
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
        data,
        id,
    } = &fframes_media_loaders::decode_image(file_name, bytes).unwrap();

    let bytes_literal = Literal::byte_string(data);

    quote! {
    image: {
        // This is basically the u32 rgba images under the hood so we must align them correctly
        // they will be again casted via bytemuch to the u32
        static ALIGNED_LITERAL: &FFramesForceAlignTo<u32, [u8]> = &FFramesForceAlignTo {
            _align: [],
            bytes: *#bytes_literal
        };

        std::sync::Arc::new(
            #fframes_crate_ident::usvgr::PreloadedImageData {
                id: String::from(#id),
                data: std::borrow::Cow::Borrowed(
                    &ALIGNED_LITERAL.bytes
                ),
                width: #width,
                height: #height,
            })
        }
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
                    filename: file_name.to_owned(),
                    path: child,
                });
            }
        } else {
            // We do not process nested folders because it allows to nest media structure as
            // different static media provider per scene seamlessly
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

#[test]
fn verify_correct_algiment_of_the_file() {
    use bytemuck::cast_slice;

    let bytes = include_bytes!("../../examples/marketing/media/marketing.mp3");
    let initial_slize: &[f32] = &fframes_media_loaders::PreloadedAudioData::decode_buffer(
        Some(44100),
        "marketing.mp3",
        bytes,
    )
    .unwrap()
    .samples;

    let u8_slice: &[u8] = cast_slice::<f32, u8>(initial_slize);
    let f32_slice: &[f32] = cast_slice::<u8, f32>(u8_slice);

    assert_eq!(initial_slize, f32_slice);
}

fn track_path(_path: &Path) {
    #[cfg(feature = "nightly")]
    proc_macro::tracked_path::path(_path.to_string_lossy());
}
