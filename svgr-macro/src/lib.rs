extern crate proc_macro2;

mod node;
mod nodes_to_format;
#[cfg(feature = "compile-time-svgtree")]
mod nodes_to_svgtree;
mod parser;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::Ident;

use syn::{
    parse::{ParseStream, Parser as _},
    Result,
};

use node::Node;
use parser::{Parser, ParserOptions};

mod punctuation {
    use syn::custom_punctuation;

    custom_punctuation!(Dash, -);
}

struct ParseOutput {
    nodes: Vec<Node>,
    animations: Vec<proc_macro2::TokenStream>,
}

#[cfg(feature = "compile-time-svgtree")]
fn create_svgr_ident(
    fframes_crate_ident: &Ident,
    nodes: Vec<Node>,
) -> syn::Result<proc_macro2::TokenStream> {
    let svg_tree = crate::nodes_to_svgtree::nodes_to_svgtree(&nodes, fframes_crate_ident)?;
    let (html_string, values) =
        crate::nodes_to_format::prepare_svg_nodes_for_format_statement(nodes, fframes_crate_ident);

    Ok(quote! {
        #fframes_crate_ident::Svgr {
            #[cfg(not(target_arch="wasm32"))]
            svg_tree: #svg_tree,
            #[cfg(target_arch="wasm32")]
            value: format!(#html_string, #(#values),*),
        }
    })
}

#[cfg(not(feature = "compile-time-svgtree"))]
fn create_svgr_ident(
    fframes_crate_ident: &Ident,
    nodes: Vec<Node>,
) -> syn::Result<proc_macro2::TokenStream> {
    let (html_string, values) =
        crate::nodes_to_format::prepare_svg_nodes_for_format_statement(nodes, &fframes_crate_ident);

    Ok(quote! {
        #fframes_crate_ident::Svgr {
             value: format!(#html_string, #(#values),*),
        }
    })
}

fn parse(tokens: proc_macro::TokenStream, fframes_crate_ident: &Ident) -> Result<ParseOutput> {
    let parser = move |input: ParseStream| {
        Parser::new(ParserOptions::default(), fframes_crate_ident).parse(input)
    };

    let (nodes, animations) = parser.parse(tokens)?;

    Ok(ParseOutput { nodes, animations })
}

#[proc_macro]
pub fn svgr(tokens: TokenStream) -> TokenStream {
    let fframes_crate_ident = match proc_macro_crate::crate_name("fframes")
        .expect("fframes crate must be present in Cargo.toml")
    {
        proc_macro_crate::FoundCrate::Itself => Ident::new("crate", Span::call_site()),
        proc_macro_crate::FoundCrate::Name(name) => Ident::new(&name, Span::call_site()),
    };

    let parse_result =
        parse(tokens, &fframes_crate_ident).and_then(|ParseOutput { nodes, animations }| {
            let svgr_ident = create_svgr_ident(&fframes_crate_ident, nodes)?;

            Ok(quote! {{
                 use #fframes_crate_ident::usvgr::svgtree::macro_prelude::*;

                 #fframes_crate_ident::lazy_static::lazy_static! { #(#animations)*
                 }

                 #[allow(unused_braces)]
                 #[allow(clippy::approx_constant)]
                 #svgr_ident
            }})
        });

    match parse_result {
        Ok(tokens) => tokens,
        Err(err) => err.to_compile_error(),
    }
    .into()
}
