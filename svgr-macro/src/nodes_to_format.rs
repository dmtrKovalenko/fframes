use crate::node::{Node, NodeType};
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};

pub(crate) fn prepare_svg_nodes_for_format_statement(
    nodes: Vec<Node>,
    fframes_crate_ident: &syn::Ident,
) -> (String, Vec<TokenStream>) {
    let mut out = String::new();
    let mut values: Vec<TokenStream> = vec![];

    for node in nodes {
        match node.node_type {
            NodeType::Element => {
                let name = node.name_as_string().unwrap();
                out.push_str(&format!("<{name}"));

                // attributes
                let (svg_string, attribute_values) =
                    prepare_svg_nodes_for_format_statement(node.attributes, fframes_crate_ident);
                out.push_str(&svg_string);
                values.extend(attribute_values);
                out.push('>');

                // children
                let (svg_string, children_values) =
                    prepare_svg_nodes_for_format_statement(node.children, fframes_crate_ident);

                out.push_str(&svg_string);
                values.extend(children_values);

                out.push_str(&format!("</{name}>"));
            }
            NodeType::Attribute => {
                out.push_str(&format!(" {}", node.name_as_string().unwrap()));
                if node.value.is_some() {
                    out.push_str(r#"="{}""#);
                    values.push(node.value.unwrap().into_token_stream());
                }
            }
            NodeType::Text => {
                out.push_str("{}");
                values.push(node.value.unwrap().into_token_stream());
            }
            NodeType::Block => {
                out.push_str("{}");
                let value = node.value.unwrap();
                let quote = quote! {
                    #fframes_crate_ident::Svgr::from(#value)
                };

                values.push(quote)
            }
        }
    }

    (out, values)
}
