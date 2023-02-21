use crate::node::{Node, NodeType};
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{ExprBlock};

use usvgr::svgtree::{
    self, attributes_list, parse::SVG_NS, AId, AttributeValue, EId,
    NestedNodeKind,
};

#[derive(Debug)]
enum MaybeParsedValue<T: ToTokens> {
    Value(T),
    Expression(TokenStream),
}

impl<T: ToTokens> ToTokens for MaybeParsedValue<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            MaybeParsedValue::Value(value) => value.to_tokens(tokens),
            MaybeParsedValue::Expression(expr) => expr.to_tokens(tokens),
        }
    }
}

pub(crate) trait CompileTimeValue {
    fn resolve_str(&self) -> Option<String>;
    fn resolve_block(&self) -> Option<ExprBlock>;
}

impl CompileTimeValue for Node {
    fn resolve_str(&self) -> Option<String> {
        self.value_as_string()
    }

    fn resolve_block(&self) -> Option<ExprBlock> {
        self.value_as_block()
    }
}

fn maybe_value<T: ToTokens>(
    value: &impl CompileTimeValue,
    create_expression: impl FnOnce(ExprBlock) -> TokenStream,
    get_value: impl FnOnce(&str) -> syn::Result<T>,
) -> syn::Result<MaybeParsedValue<T>> {
    let inlined_value: Option<String> = value.resolve_str();
    let runtime_value: Option<syn::ExprBlock> = value.resolve_block();

    Ok(match (inlined_value, runtime_value) {
        (Some(value), _) => MaybeParsedValue::Value(get_value(value.as_str())?),
        (None, Some(block)) => MaybeParsedValue::Expression(create_expression(block)),
        _ => panic!(""),
    })
}

#[derive(Debug)]
struct MaybeAttribute {
    name: AId,
    value: MaybeParsedValue<AttributeValue>,
}

impl ToTokens for MaybeAttribute {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let MaybeAttribute { name, value } = self;

        match value {
            MaybeParsedValue::Value(value) => quote! {
                Some(Attribute {
                    name: #name,
                    value: #value
                })
            },
            MaybeParsedValue::Expression(block) => {
                quote! {
                    #block.map(|value| Attribute {
                        name: #name,
                        value
                    })
                }
            }
        }
        .to_tokens(tokens);
    }
}

struct LiteralOption<T: ToTokens>(Option<T>);

impl<T: ToTokens> ToTokens for LiteralOption<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            LiteralOption(Some(value)) => quote! { Some(#value) },
            LiteralOption(None) => quote! { None },
        }
        .to_tokens(tokens)
    }
}

struct TokenizeableVec<T: ToTokens>(Vec<T>);

impl<T: ToTokens> ToTokens for TokenizeableVec<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let TokenizeableVec(vec) = self;

        quote! {
            vec![#(#vec),*]
        }
        .to_tokens(tokens)
    }
}

struct MaybeNodeData {
    pub kind: NestedNodeKind,
    pub attrs: Vec<MaybeAttribute>,
    pub children: Vec<MaybeParsedValue<MaybeNodeData>>,
}

impl ToTokens for MaybeNodeData {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let Self {
            kind,
            attrs,
            children,
        } = self;

        let children_tokens = tokenize_nodes(children);

        quote::quote! {
            Some(NestedNodeData {
                kind: #kind,
                attrs: vec![#(#attrs),*],
                children: #children_tokens,
            })
        }
        .to_tokens(tokens)
    }
}

/// Either inline nodes as values or if we have subtrees create into_flattened expression unwrapping trees.
fn tokenize_nodes(nodes: &[MaybeParsedValue<MaybeNodeData>]) -> TokenStream {
    if nodes
        .iter()
        .all(|c| matches!(c, MaybeParsedValue::Value(_)))
    {
        quote! { vec![#(#nodes),*] }
    } else {
        let mut subtrees = Vec::with_capacity(nodes.len());
        let mut last_inlined_tree = TokenizeableVec(vec![]);

        for node in nodes {
            match node {
                MaybeParsedValue::Value(value) => last_inlined_tree.0.push(value),
                MaybeParsedValue::Expression(expr) => {
                    subtrees.push(last_inlined_tree.to_token_stream());
                    subtrees.push(expr.clone());

                    last_inlined_tree = TokenizeableVec(vec![]);
                }
            }
        }

        if !last_inlined_tree.0.is_empty() {
            subtrees.push(last_inlined_tree.to_token_stream());
        }

        quote! {
            vec![#(#subtrees),*]
                .into_iter()
                .flatten()
                .collect::<Vec<Option<NestedNodeData>>>()
        }
    }
}

lazy_static::lazy_static! {
    static ref ATTRIBUTE_NAMES_LIST: Vec<&'static str> = attributes_list();
}

fn detail_attribute_error(attribute: &str, span: Span) -> syn::Error {
    syn::Error::new(
        span,
        match attribute {
            "xlink:href" => "Svgr do not support custom namespaces. Use `href` instead.".to_owned(),
            attribute if attribute.starts_with("xmlns:") => {
                "The `xmlns:` attributes and dynamic xml namespaces are not supported.\n\nMost of that popular namespaces are deprecated and will be resolved without namespace,\ne.g. the `xlink:href` will be resolved exactly the same as `href`.".to_owned()
            },
            "xml:space" => "xml:space attribute is used to control string trimming in XML and makes no sense in svgr macro,\nwhere you explicitly control the child string length, so if you need to trim the string just add `{text.trim()}` as children of `<text>`\n\nPlease remove this attribute.".to_owned(),
            _ => { 
                let fuzzy_match = rust_fuzzy_search::fuzzy_search_best_n(attribute, &ATTRIBUTE_NAMES_LIST, 1);
                let suggestion = match fuzzy_match.first() {
                    Some((suggestion, value)) if *value > 0.6 => format!("\n\nMaybe you meant `{suggestion}`?"),
                    _ => "".to_owned()
                };

                format!("{attribute} attribute is not valid or not supported{suggestion}")
            },
        },
    )
}

fn maybe_parse_svg_attribute(
    attribute: &Node,
    eid: EId,
) -> Result<Option<(AId, MaybeParsedValue<AttributeValue>)>, syn::Error> {
    let attribute_span = attribute.name_span().unwrap_or_else(|| {
        panic!("Critical parsing error. Trying to locate some attribute but couldn't.")
    });

    let attribute_name = attribute.name_as_string().ok_or_else(|| {
        syn::Error::new(attribute_span, "Dynamic attribute names are not supported.")
    })?;

    if attribute.name_as_string() == Some("xmlns".to_string()) {
        if attribute.value_as_string().as_deref() == Some(SVG_NS) {
            return Ok(None);
        } else {
            return Err(syn::Error::new(attribute_span, format!("Found non svg namespace: {}, please make sure that only svg xml is suppoerted.\nPlease make sure to enter a valid SVG namespace => {SVG_NS}", attribute.value_as_string().unwrap_or_default())));
        }
    }

    let aid = AId::from_str(attribute.name_as_string().unwrap().as_str())
        .ok_or_else(|| detail_attribute_error(attribute_name.as_str(), attribute_span))?;

    if aid == AId::Class {
        return Err(syn::Error::new(
            attribute_span,
            "The `class` attribute is not supported. Neither classes nor <style /> tags are supported because raw css is incredibly hard to support.\n\nUse inlined styles or style=\"\" attribute instead.",
        ));
    }

    let value = maybe_value(
        attribute,
        |block| quote! {parse::parse_svg_attribute(#eid, #aid, #block.to_string().as_str())},
        |value| {
            svgtree::parse::parse_svg_attribute(eid, aid, value).ok_or_else(|| {
                syn::Error::new(
                    attribute.value_span().unwrap(),
                    "value is not valid for attribute",
                )
            })
        },
    )?;

    Ok(Some((aid, value)))
}

fn map_text_node_children(
    nodes: &[Node],
    parent: EId,
    fframes_crate_ident: &syn::Ident
) -> syn::Result<Vec<MaybeParsedValue<MaybeNodeData>>> {
    let mut parsed_nodes = Vec::with_capacity(nodes.len());

    for node in nodes {
        if node.node_type == NodeType::Text {
            parsed_nodes.push(MaybeParsedValue::Value(MaybeNodeData {
                attrs: vec![],
                children: vec![],
                kind: svgtree::NestedNodeKind::Text(node.value_as_string().unwrap()),
            }));

            continue;
        }

        if node.node_type == NodeType::Block {
            if let Some(value) = parse_svgr_subtree(node, fframes_crate_ident) {
                parsed_nodes.push(value?);
            }

            continue;
        }

        if node.node_type != NodeType::Element {
            continue;
        }

        let tag_name = parse_tag_name(node)?;
        if tag_name == EId::A {
            return Err(syn::Error::new(
                node.name_span().unwrap(),
                "The `<a>` element is not supported because videos are completely static.\n\nYou can use <g> or <text> instead.",
            ));
        }

        if !matches!(tag_name, EId::Tspan | EId::TextPath) {
            continue;
        }

        // `textPath` must be a direct `text` child.
        if tag_name == EId::TextPath && parent != EId::Text {
            continue;
        }

        parsed_nodes.push(MaybeParsedValue::Value(MaybeNodeData {
            attrs: parse_element_attributes(node, tag_name)?,
            children: map_text_node_children(node.children.as_slice(), tag_name, fframes_crate_ident)?,
            kind: NestedNodeKind::Element { tag_name },
        }))
    }

    Ok(parsed_nodes)
}

fn parse_svgr_subtree(node: &Node, fframes_crate_ident: &syn::Ident) -> Option<Result<MaybeParsedValue<MaybeNodeData>, syn::Error>> {
    node.value_as_block().map(|block| {
        Ok(MaybeParsedValue::Expression(quote! {
            #fframes_crate_ident::Svgr::from(#block).svg_tree.nodes
        }))
    })
}

fn parse_element_attributes(node: &Node, eid: EId) -> Result<Vec<MaybeAttribute>, syn::Error> {
    node.attributes
        .iter()
        .filter_map(|attribute| -> Option<syn::Result<_>> {
            Some(
                maybe_parse_svg_attribute(attribute, eid)
                    .transpose()?
                    .map(|(aid, value)| MaybeAttribute { name: aid, value }),
            )
        })
        .collect::<syn::Result<Vec<_>>>()
}

fn parse_tag_name(node: &Node) -> Result<EId, syn::Error> {
    EId::from_str(node.name_as_string().unwrap().as_str())
        .ok_or_else(|| syn::Error::new(node.name_span().unwrap(), "element is not supported"))
}

fn map_inline_or_runtime_nodes(
    nodes: &[Node],
    fframes_crate_ident: &syn::Ident,
) -> syn::Result<Vec<MaybeParsedValue<MaybeNodeData>>> {
    let mut parsed_nodes = Vec::with_capacity(nodes.len());

    for node in nodes {
        if node.node_type == NodeType::Block {
            if let Some(value) = parse_svgr_subtree(node, fframes_crate_ident) {
                parsed_nodes.push(value?);
            }

            continue;
        }

        if node.node_type != NodeType::Element {
            continue;
        }

        let tag_name = parse_tag_name(node)?;
        if tag_name == EId::Style {
            return Err(syn::Error::new(
                node.name_span().unwrap(),
                "Style attribute is not supported, please use either element attributes or inline styles. <style> css is too complex for svg's which will involve much more indirection, if you are not agree though please open an issue.",
            ));
        }

        let attrs = parse_element_attributes(node, tag_name)?;

        let children = match tag_name {
            EId::Text => map_text_node_children(&node.children, tag_name, fframes_crate_ident),
            _ => map_inline_or_runtime_nodes(&node.children, fframes_crate_ident),
        }?;

        parsed_nodes.push(MaybeParsedValue::Value(MaybeNodeData {
            attrs,
            children,
            kind: svgtree::NestedNodeKind::Element { tag_name },
        }));
    }

    Ok(parsed_nodes)
}

pub fn nodes_to_svgtree(nodes: &[Node], fframes_crate_ident: &syn::Ident) -> syn::Result<TokenStream> {
    let nodes = map_inline_or_runtime_nodes(nodes, fframes_crate_ident)?;

    let tokens = tokenize_nodes(&nodes);
    let output_tree = quote! {
        NestedSvgDocument {
            nodes: #tokens
        }
    };

    Ok(output_tree)
}
