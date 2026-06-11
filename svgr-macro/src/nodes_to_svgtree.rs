use crate::node::{Node, NodeType};
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use syn::ExprBlock;

use usvgr::svgtree::{self, parse::SVG_NS, svgrtypes::PathSegment, AId, EId, NestedNodeKind};

/// Convert a PathSegment to TokenStream manually since ToTokens impl
/// from svgrtypes isn't visible in proc-macro context
fn path_segment_to_tokens(segment: &PathSegment) -> TokenStream {
    match segment {
        PathSegment::MoveTo { abs, x, y } => {
            quote! { svgrtypes::PathSegment::MoveTo { abs: #abs, x: #x, y: #y } }
        }
        PathSegment::LineTo { abs, x, y } => {
            quote! { svgrtypes::PathSegment::LineTo { abs: #abs, x: #x, y: #y } }
        }
        PathSegment::HorizontalLineTo { abs, x } => {
            quote! { svgrtypes::PathSegment::HorizontalLineTo { abs: #abs, x: #x } }
        }
        PathSegment::VerticalLineTo { abs, y } => {
            quote! { svgrtypes::PathSegment::VerticalLineTo { abs: #abs, y: #y } }
        }
        PathSegment::CurveTo {
            abs,
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        } => {
            quote! { svgrtypes::PathSegment::CurveTo { abs: #abs, x1: #x1, y1: #y1, x2: #x2, y2: #y2, x: #x, y: #y } }
        }
        PathSegment::SmoothCurveTo { abs, x2, y2, x, y } => {
            quote! { svgrtypes::PathSegment::SmoothCurveTo { abs: #abs, x2: #x2, y2: #y2, x: #x, y: #y } }
        }
        PathSegment::Quadratic { abs, x1, y1, x, y } => {
            quote! { svgrtypes::PathSegment::Quadratic { abs: #abs, x1: #x1, y1: #y1, x: #x, y: #y } }
        }
        PathSegment::SmoothQuadratic { abs, x, y } => {
            quote! { svgrtypes::PathSegment::SmoothQuadratic { abs: #abs, x: #x, y: #y } }
        }
        PathSegment::EllipticalArc {
            abs,
            rx,
            ry,
            x_axis_rotation,
            large_arc,
            sweep,
            x,
            y,
        } => {
            quote! { svgrtypes::PathSegment::EllipticalArc { abs: #abs, rx: #rx, ry: #ry, x_axis_rotation: #x_axis_rotation, large_arc: #large_arc, sweep: #sweep, x: #x, y: #y } }
        }
        PathSegment::ClosePath { abs } => {
            quote! { svgrtypes::PathSegment::ClosePath { abs: #abs } }
        }
    }
}

#[derive(Debug)]
enum MaybeParsedValue<T: ToTokens> {
    Value(T),
    Expression(TokenStream),
}

impl<T: ToTokens> MaybeParsedValue<T> {
    fn is_static(&self) -> bool {
        matches!(self, MaybeParsedValue::Value(_))
    }
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

    match (inlined_value, runtime_value) {
        (Some(value), _) => Ok(MaybeParsedValue::Value(get_value(value.as_str())?)),
        (None, Some(block)) => Ok(MaybeParsedValue::Expression(create_expression(block))),
        _ => Err(syn::Error::new(
            Span::call_site(),
            "Attribute must be either a string or a block",
        )),
    }
}

#[derive(Debug)]
struct MaybeAttribute {
    name: AId,
    value: MaybeParsedValue<String>,
}

impl MaybeAttribute {
    fn is_static(&self) -> bool {
        self.value.is_static()
    }

    /// Compute a hash of the attribute's static content for cache key generation
    fn hash_static_content(&self, hasher: &mut impl Hasher) {
        if let MaybeParsedValue::Value(ref s) = self.value {
            // Hash the attribute ID and value
            (self.name as u16).hash(hasher);
            s.hash(hasher);
        }
    }
}

fn inline_attribute_value(value: &str, aid: AId) -> TokenStream {
    // Special handling for path data - parse at compile time
    if aid == AId::D {
        let segments: Vec<_> = svgtree::svgrtypes::PathParser::from(value)
            .filter_map(|s| s.ok())
            .collect();

        if !segments.is_empty() {
            let segment_tokens: Vec<_> = segments.iter().map(path_segment_to_tokens).collect();
            return quote! {
                {
                    static SEGMENTS: &[svgrtypes::PathSegment] = &[#(#segment_tokens),*];
                    SvgAttributeValue::PathData(std::borrow::Cow::Borrowed(SEGMENTS))
                }
            };
        }
        // Fall through to string if parsing fails
    }

    if let Ok(float) = f32::from_str(value) {
        // This is required to suppress rust analyzer errors which is not expecting the -
        // token before the float lieterals coming from the proc macro generated code.
        if float.is_sign_negative() {
            let float = float.abs();
            quote! {
                SvgAttributeValue::Float(- #float, StringStorage::Borrowed(#value))
            }
        } else {
            quote! {
                SvgAttributeValue::Float(#float, StringStorage::Borrowed(#value))
            }
        }
    } else if let Ok(color) = svgtree::svgrtypes::Color::from_str(value) {
        quote! {
            SvgAttributeValue::Color(#color)
        }
    } else if let Ok(length) = svgtree::svgrtypes::Length::from_str(value) {
        quote! {
            SvgAttributeValue::Length(#length)
        }
    } else if let Ok(transform) = svgtree::svgrtypes::Transform::from_str(value) {
        quote! {
            SvgAttributeValue::Transform(#transform)
        }
    } else {
        quote! {
            SvgAttributeValue::StringStorage(
                StringStorage::Borrowed(#value)
            )
        }
    }
}

impl ToTokens for MaybeAttribute {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let MaybeAttribute { name, value } = self;
        let name_tokens = name.to_tokens();

        match value {
            MaybeParsedValue::Value(value) => {
                let value_tokens = inline_attribute_value(value, *name);
                quote! {
                    Attribute {
                        name: #name_tokens,
                        value: #value_tokens
                    }
                }
            }
            MaybeParsedValue::Expression(block) => {
                quote! {
                    Attribute {
                        name: #name_tokens,
                        value: SvgAttributeValue::from(#block)
                    }
                }
            }
        }
        .to_tokens(tokens);
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
    pub kind: NestedNodeKind<'static>,
    pub attrs: Vec<MaybeAttribute>,
    pub children: Vec<MaybeParsedValue<MaybeNodeData>>,
}

impl MaybeNodeData {
    /// Check if this node and ALL its descendants are fully static (no runtime expressions)
    fn is_fully_static(&self) -> bool {
        // All attributes must be static
        let attrs_static = self.attrs.iter().all(|a| a.is_static());
        // All children must be static Values (not Expressions) AND recursively static
        let children_static = self.children.iter().all(|c| match c {
            MaybeParsedValue::Value(node) => node.is_fully_static(),
            MaybeParsedValue::Expression(_) => false,
        });

        attrs_static && children_static
    }

    /// Compute a compile-time hash of this node's content for static caching.
    /// Only valid if is_fully_static() returns true.
    fn compute_static_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();
        self.hash_content(&mut hasher);
        hasher.finish()
    }

    fn hash_content(&self, hasher: &mut impl Hasher) {
        // Hash the node kind
        match &self.kind {
            NestedNodeKind::Root => {
                2u8.hash(hasher); // discriminant for Root
            }
            NestedNodeKind::Element { tag_name } => {
                0u8.hash(hasher); // discriminant
                (*tag_name as u16).hash(hasher);
            }
            NestedNodeKind::Text(storage) => {
                1u8.hash(hasher); // discriminant
                                  // Hash the text content
                match storage {
                    svgtree::roxmltree::StringStorage::Borrowed(s) => s.hash(hasher),
                    svgtree::roxmltree::StringStorage::Owned(s) => s.as_ref().hash(hasher),
                }
            }
        }

        // Hash all attributes
        self.attrs.len().hash(hasher);
        for attr in &self.attrs {
            attr.hash_static_content(hasher);
        }

        // Recursively hash children
        self.children.len().hash(hasher);
        for child in &self.children {
            if let MaybeParsedValue::Value(node) = child {
                node.hash_content(hasher);
            }
        }
    }
}

impl ToTokens for MaybeNodeData {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let Self {
            kind,
            attrs,
            children,
        } = self;

        let children_tokens = tokenize_nodes(children);

        // Compute static hash if this node is fully static
        let static_hash_token = if self.is_fully_static() {
            let hash = self.compute_static_hash();
            quote! { Some(#hash) }
        } else {
            quote! { None }
        };

        quote::quote! {
            Some(NestedNodeData {
                kind: #kind,
                attrs: vec![#(#attrs),*].into_boxed_slice(),
                children: #children_tokens,
                static_hash: #static_hash_token,
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
    static ref ATTRIBUTE_NAMES_LIST: Vec<&'static str> =
        svgtree::ATTRIBUTES.entries.iter().map(|(name, _)| *name).collect();
}

fn detailed_attribute_error(attribute: &str, span: Span) -> syn::Error {
    use std::borrow::Cow;
    let msg: Cow<'static, str> = match attribute {
        "xlink:href" => Cow::Borrowed("FFrames svg does not support custom namespaces. Use `href` instead."),
        attribute if attribute.starts_with("xmlns:") => Cow::Borrowed(
            "The `xmlns:` attributes and dynamic xml namespaces are not supported.\n\nMost of that popular namespaces are deprecated and will be resolved without namespace,\ne.g. the `xlink:href` will be resolved exactly the same as `href`.",
        ),
        "xml:space" => Cow::Borrowed("xml:space attribute is used to control string trimming in XML and makes no sense in svgr macro,\nwhere you explicitly control the child string length, so if you need to trim the string just add `{text.trim()}` as children of `<text>`\n\nPlease remove this attribute."),
        _ => {
            let fuzzy_match = rust_fuzzy_search::fuzzy_search_best_n(attribute, &ATTRIBUTE_NAMES_LIST, 1);
            let suggestion = match fuzzy_match.first() {
                Some((suggestion, value)) if *value > 0.6 => format!(" Did you mean `{suggestion}`?"),
                _ => String::new(),
            };
            Cow::Owned(format!("{attribute} attribute is not supported or not valid for this element.{suggestion}"))
        },
    };
    syn::Error::new(span, msg)
}

// TODO: parse and precache attribute value instead of always inlining as string
fn maybe_parse_svg_attribute(
    attribute: &Node,
    _eid: EId,
) -> Result<Option<(AId, MaybeParsedValue<String>)>, syn::Error> {
    let attribute_span = attribute.name_span().unwrap_or_else(|| {
        panic!("Critical parsing error. Trying to locate some attribute but couldn't.")
    });

    let attribute_name = attribute.name_as_string().ok_or_else(|| {
        syn::Error::new(attribute_span, "Dynamic attribute names are not supported.")
    })?;

    if attribute.name_as_string().as_deref() == Some("xmlns") {
        if attribute.value_as_string().as_deref() == Some(SVG_NS) {
            return Ok(None);
        } else {
            return Err(syn::Error::new(attribute_span, format!("Found non svg namespace: {}, please make sure that only svg xml is supported.\nPlease make sure to enter a valid SVG namespace => {SVG_NS}", attribute.value_as_string().unwrap_or_default())));
        }
    }

    let aid = AId::from_str(attribute.name_as_string().unwrap().as_str())
        .ok_or_else(|| detailed_attribute_error(attribute_name.as_str(), attribute_span))?;

    if aid == AId::Class {
        return Err(syn::Error::new(
            attribute_span,
            "The `class` attribute is not supported. Neither classes nor <style /> tags are supported because raw css is incredibly hard to support.\n\nUse inlined styles or style=\"\" attribute instead.",
        ));
    }

    let value = maybe_value(
        attribute,
        |block| block.into_token_stream(),
        |value| Ok(String::from(value)),
    )?;

    Ok(Some((aid, value)))
}

fn map_text_node_children(
    nodes: &[Node],
    parent: EId,
    fframes_crate_ident: &syn::Ident,
) -> syn::Result<Vec<MaybeParsedValue<MaybeNodeData>>> {
    let mut parsed_nodes = Vec::with_capacity(nodes.len());

    for node in nodes {
        if node.node_type == NodeType::Text {
            let text_content = node.value_as_string().ok_or_else(|| {
                syn::Error::new(
                    node.name_span().unwrap(),
                    "Failed to parse text element tag",
                )
            })?;
            parsed_nodes.push(MaybeParsedValue::Value(MaybeNodeData {
                attrs: vec![],
                children: vec![],
                kind: svgtree::NestedNodeKind::Text(svgtree::roxmltree::StringStorage::new_owned(
                    text_content.as_str(),
                )),
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
            children: map_text_node_children(
                node.children.as_slice(),
                tag_name,
                fframes_crate_ident,
            )?,
            kind: NestedNodeKind::Element { tag_name },
        }))
    }

    Ok(parsed_nodes)
}

fn parse_svgr_subtree(
    node: &Node,
    fframes_crate_ident: &syn::Ident,
) -> Option<Result<MaybeParsedValue<MaybeNodeData>, syn::Error>> {
    node.value_as_block().map(|block| {
        Ok(MaybeParsedValue::Expression(quote! {
            #fframes_crate_ident::Svgr::from(#block).as_subtree()
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
            EId::Text | EId::Tspan | EId::TextPath => {
                map_text_node_children(&node.children, tag_name, fframes_crate_ident)
            }
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

pub fn nodes_to_svgtree(
    nodes: &[Node],
    fframes_crate_ident: &syn::Ident,
) -> syn::Result<TokenStream> {
    let nodes = map_inline_or_runtime_nodes(nodes, fframes_crate_ident)?;

    let tokens = tokenize_nodes(&nodes);
    let output_tree = quote! {
        NestedSvgDocument::from_nodes(#tokens)
    };

    Ok(output_tree)
}
