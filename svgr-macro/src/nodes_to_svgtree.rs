use crate::node::{Node, NodeType};
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use syn::ExprBlock;

use usvgr::svgtree::{self, parse::SVG_NS, AId, EId, NestedNodeKind};

/// Check if a token stream contains a reference to `frame`, which indicates
/// the expression depends on the current frame and changes between frames.
/// Expressions that don't reference `frame` are considered "stable" -- their
/// values remain constant across all frames and can be cached.
fn is_frame_dependent(tokens: &TokenStream) -> bool {
    for token in tokens.clone() {
        match token {
            proc_macro2::TokenTree::Ident(ref ident) if ident == "frame" => return true,
            proc_macro2::TokenTree::Group(ref group) => {
                if is_frame_dependent(&group.stream()) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

#[derive(Debug)]
enum MaybeParsedValue<T: ToTokens> {
    Value(T),
    Expression(TokenStream),
    /// A subtree expression that wraps an original value expression.
    /// `generated` is the full expression (e.g., `Svgr::from(expr).as_subtree()`)
    /// `original_expr` is the unwrapped expression (e.g., `expr`) used for hashing.
    SubtreeExpression {
        generated: TokenStream,
        original_expr: TokenStream,
    },
}

impl<T: ToTokens> ToTokens for MaybeParsedValue<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            MaybeParsedValue::Value(value) => value.to_tokens(tokens),
            MaybeParsedValue::Expression(expr) => expr.to_tokens(tokens),
            MaybeParsedValue::SubtreeExpression { generated, .. } => generated.to_tokens(tokens),
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

impl MaybeParsedValue<String> {
    /// Check if this value contains a frame-dependent expression.
    fn is_frame_dependent(&self) -> bool {
        match self {
            MaybeParsedValue::Value(_) => false,
            MaybeParsedValue::Expression(expr) => is_frame_dependent(expr),
            MaybeParsedValue::SubtreeExpression { original_expr, .. } => {
                is_frame_dependent(original_expr)
            }
        }
    }
}

impl MaybeParsedValue<MaybeNodeData> {
    /// Check if this child node value contains a frame-dependent expression.
    fn is_frame_dependent(&self) -> bool {
        match self {
            MaybeParsedValue::Value(node) => node.is_frame_dependent(),
            MaybeParsedValue::Expression(expr) => is_frame_dependent(expr),
            MaybeParsedValue::SubtreeExpression { original_expr, .. } => {
                is_frame_dependent(original_expr)
            }
        }
    }
}

fn path_segment_to_tokens(segment: &svgtree::svgrtypes::PathSegment) -> TokenStream {
    use svgtree::svgrtypes::PathSegment;
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

fn inline_attribute_value(value: &str, aid: AId) -> TokenStream {
    // Special handling for path data - parse at compile time as a static slice
    if aid == AId::D {
        let segments: Vec<_> = svgtree::svgrtypes::PathParser::from(value)
            .filter_map(|s| s.ok())
            .collect();

        if !segments.is_empty() {
            let segment_tokens: Vec<_> = segments.iter().map(path_segment_to_tokens).collect();
            // Generate a static slice reference to avoid allocation
            return quote! {
                SvgAttributeValue::PathData(std::borrow::Cow::Borrowed(&[#(#segment_tokens),*]))
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
            MaybeParsedValue::Expression(block)
            | MaybeParsedValue::SubtreeExpression {
                generated: block, ..
            } => {
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

impl MaybeAttribute {
    /// Check if this attribute is fully static (no runtime expressions)
    fn is_static(&self) -> bool {
        matches!(self.value, MaybeParsedValue::Value(_))
    }

    /// Check if this attribute depends on the current frame
    fn is_frame_dependent(&self) -> bool {
        self.value.is_frame_dependent()
    }
}

impl MaybeNodeData {
    /// Check if this node and ALL its descendants are fully static (no runtime expressions)
    fn is_fully_static(&self) -> bool {
        // All attributes must be static
        let attrs_static = self.attrs.iter().all(|a| a.is_static());
        // All children must be static Values (not Expressions) AND recursively static
        let children_static = self.children.iter().all(|c| match c {
            MaybeParsedValue::Value(node) => node.is_fully_static(),
            MaybeParsedValue::Expression(_) | MaybeParsedValue::SubtreeExpression { .. } => false,
        });

        attrs_static && children_static
    }

    /// Check if this node or any of its descendants depend on the current frame.
    /// A node is "stable" (not frame-dependent) if all runtime expressions within it
    /// do not reference `frame`. Stable nodes produce the same output for every frame
    /// of the video, so they can be cached after first render.
    fn is_frame_dependent(&self) -> bool {
        let attrs_frame_dependent = self.attrs.iter().any(|a| a.is_frame_dependent());
        let children_frame_dependent = self.children.iter().any(|c| c.is_frame_dependent());

        attrs_frame_dependent || children_frame_dependent
    }

    /// Check if this node is "stable" -- it may contain runtime expressions, but none
    /// of them depend on the current frame. Such nodes produce identical output across
    /// all frames and can be cached permanently after the first render.
    fn is_stable(&self) -> bool {
        !self.is_frame_dependent()
    }

    /// The static hash used as id but is a hash just for the availability to avoid rendering
    /// of potentially duplicated subtrees if they are literally duplicated.
    /// Not sure how this is impact the compile times but should be pretty fast
    fn compute_static_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        format!("{:?}", self.kind).hash(&mut hasher);

        // Hash all attributes
        for attr in &self.attrs {
            format!("{:?}", attr.name).hash(&mut hasher);
            if let MaybeParsedValue::Value(ref value) = attr.value {
                value.hash(&mut hasher);
            }
        }

        // Hash children recursively
        for child in &self.children {
            if let MaybeParsedValue::Value(node) = child {
                node.compute_static_hash().hash(&mut hasher);
            }
        }

        hasher.finish()
    }

    /// Compute a seed hash from the compile-time-known parts of this node.
    /// This seed is combined with runtime expression values to produce a full
    /// hash for stable-but-not-fully-static nodes.
    fn compute_stable_seed_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        format!("{:?}", self.kind).hash(&mut hasher);

        // Hash all attribute names, and values for static ones
        for attr in &self.attrs {
            format!("{:?}", attr.name).hash(&mut hasher);
            if let MaybeParsedValue::Value(ref value) = attr.value {
                value.hash(&mut hasher);
            }
        }

        // Hash children recursively where possible
        for child in &self.children {
            if let MaybeParsedValue::Value(node) = child {
                if node.is_fully_static() {
                    node.compute_static_hash().hash(&mut hasher);
                } else {
                    // For stable-but-not-static child nodes, include their seed
                    node.compute_stable_seed_hash().hash(&mut hasher);
                }
            }
        }

        hasher.finish()
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

        if self.is_fully_static() {
            // Case 1: Fully static -- all values known at compile time.
            // Compute hash at compile time (zero runtime cost).
            let hash = self.compute_static_hash();
            quote::quote! {
                Some(NestedNodeData {
                    kind: #kind,
                    attrs: vec![#(#attrs),*],
                    children: #children_tokens,
                    static_hash: Some(#hash),
                })
            }
            .to_tokens(tokens)
        } else if self.is_stable() {
            // Case 2: Stable but not fully static -- contains runtime expressions
            // that don't depend on frame (e.g., {self.slug}, {ctx.video_size}).
            // Build the node first, then compute a runtime hash from its actual values.
            let seed = self.compute_stable_seed_hash();

            quote::quote! {
                {
                    let mut __svgr_node = NestedNodeData {
                        kind: #kind,
                        attrs: vec![#(#attrs),*],
                        children: #children_tokens,
                        static_hash: None,
                    };

                    __svgr_node.static_hash = Some(
                        __svgr_node.compute_runtime_hash(#seed)
                    );

                    Some(__svgr_node)
                }
            }
            .to_tokens(tokens)
        } else {
            // Case 3: Frame-dependent -- contains expressions that reference `frame`.
            // Cannot be cached, hash must be None.
            quote::quote! {
                Some(NestedNodeData {
                    kind: #kind,
                    attrs: vec![#(#attrs),*],
                    children: #children_tokens,
                    static_hash: None,
                })
            }
            .to_tokens(tokens)
        };
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
                MaybeParsedValue::SubtreeExpression { generated, .. } => {
                    subtrees.push(last_inlined_tree.to_token_stream());
                    subtrees.push(generated.clone());

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
    syn::Error::new(
        span,
        match attribute {
            "xlink:href" => "FFrames svg does not support custom namespaces. Use `href` instead.".to_owned(),
            attribute if attribute.starts_with("xmlns:") => {
                "The `xmlns:` attributes and dynamic xml namespaces are not supported.\n\nMost of that popular namespaces are deprecated and will be resolved without namespace,\ne.g. the `xlink:href` will be resolved exactly the same as `href`.".to_owned()
            },
            "xml:space" => "xml:space attribute is used to control string trimming in XML and makes no sense in svgr macro,\nwhere you explicitly control the child string length, so if you need to trim the string just add `{text.trim()}` as children of `<text>`\n\nPlease remove this attribute.".to_owned(),
            _ => {
                let fuzzy_match = rust_fuzzy_search::fuzzy_search_best_n(attribute, &ATTRIBUTE_NAMES_LIST, 1);
                let suggestion = match fuzzy_match.first() {
                    Some((suggestion, value)) if *value > 0.6 => format!(" Did you mean `{suggestion}`?"),
                    _ => "".to_owned()
                };

                format!("{attribute} attribute is not supported or not valid for this element.{suggestion}")
            },
        },
    )
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

    if attribute.name_as_string() == Some("xmlns".to_string()) {
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
            parsed_nodes.push(MaybeParsedValue::Value(MaybeNodeData {
                attrs: vec![],
                children: vec![],
                kind: svgtree::NestedNodeKind::Text(svgtree::roxmltree::StringStorage::new_owned(
                    node.value_as_string()
                        .ok_or_else(|| {
                            syn::Error::new(
                                node.name_span().unwrap(),
                                "Failed to parse text element tag",
                            )
                        })?
                        .as_str(),
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
        let original_expr = block.to_token_stream();
        let generated = quote! {
            #fframes_crate_ident::Svgr::from(#block).as_subtree()
        };
        Ok(MaybeParsedValue::SubtreeExpression {
            generated,
            original_expr,
        })
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
