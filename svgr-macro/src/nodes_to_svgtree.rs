use crate::node::{Node, NodeType};
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use syn::ExprBlock;

use usvgr::svgtree::{self, parse::SVG_NS, svgrtypes::PathSegment, AId, EId, NestedNodeKind};

/// Convert a `PathSegment` to `TokenStream` manually since `ToTokens` impl
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

/// A `data-fframes-*` attribute: not an SVG attribute, usvgr keeps it on the converted node
/// for fframes to read (`node.fframes_data().get(key)`).
#[derive(Debug)]
struct MaybeDataAttribute {
    /// The name without the `data-fframes-` prefix.
    key: String,
    value: MaybeParsedValue<String>,
}

impl ToTokens for MaybeDataAttribute {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let key = &self.key;
        match &self.value {
            MaybeParsedValue::Value(value) => quote! {
                (#key, SvgAttributeValue::StringStorage(StringStorage::Borrowed(#value)))
            },
            MaybeParsedValue::Expression(block) => quote! {
                (#key, SvgAttributeValue::from(#block))
            },
        }
        .to_tokens(tokens);
    }
}

#[derive(Debug)]
struct MaybeAttribute {
    name: AId,
    value: MaybeParsedValue<String>,
    /// The element the attribute belongs to.
    element: EId,
}

impl MaybeAttribute {
    /// Mix the attribute into a node's static hash; dynamic values contribute nothing.
    fn hash_static_content(&self, hasher: &mut impl Hasher) {
        if let MaybeParsedValue::Value(ref s) = self.value {
            (self.name as u16).hash(hasher);
            s.hash(hasher);
        }
    }
}

/// Wraps compile-time path segments into a `static` whose `tiny_skia` path is built
/// once per process, see `usvgr::svgtree::StaticPathData`.
fn static_path_tokens(segments: &[PathSegment]) -> TokenStream {
    let segment_tokens: Vec<_> = segments.iter().map(path_segment_to_tokens).collect();
    quote! {
        {
            static PATH: StaticPathData = StaticPathData::new(&[#(#segment_tokens),*]);
            SvgAttributeValue::StaticPath(&PATH)
        }
    }
}

/// `points` of a `polyline` or `polygon` as path segments, mirroring how `usvgr`
/// builds these shapes. `None` when the shape would not render (less than 2 points).
fn points_to_segments(value: &str, element: EId) -> Option<Vec<PathSegment>> {
    let mut segments: Vec<_> = svgtree::svgrtypes::PointsParser::from(value)
        .enumerate()
        .map(|(index, (x, y))| {
            if index == 0 {
                PathSegment::MoveTo { abs: true, x, y }
            } else {
                PathSegment::LineTo { abs: true, x, y }
            }
        })
        .collect();

    if segments.len() < 2 {
        return None;
    }

    if element == EId::Polygon {
        segments.push(PathSegment::ClosePath { abs: true });
    }

    Some(segments)
}

fn inline_attribute_value(value: &str, aid: AId, element: EId) -> TokenStream {
    // Special handling for path data - parse at compile time
    if aid == AId::D {
        // Only pre-parse when the whole string is valid: a partially parsed
        // path would silently render different geometry than the source.
        let segments: Option<Vec<_>> = svgtree::svgrtypes::PathParser::from(value)
            .collect::<Result<_, _>>()
            .ok()
            .filter(|segments: &Vec<_>| !segments.is_empty());

        if let Some(segments) = segments {
            return static_path_tokens(&segments);
        }
        // Fall through to string if parsing fails
    }

    if aid == AId::Points && matches!(element, EId::Polygon | EId::Polyline) {
        if let Some(segments) = points_to_segments(value, element) {
            return static_path_tokens(&segments);
        }
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
        let MaybeAttribute {
            name,
            value,
            element,
        } = self;
        let name_tokens = name.to_tokens();

        match value {
            MaybeParsedValue::Value(value) => {
                let value_tokens = inline_attribute_value(value, *name, *element);
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
        .to_tokens(tokens);
    }
}

struct MaybeNodeData {
    pub kind: NestedNodeKind<'static>,
    pub attrs: Vec<MaybeAttribute>,
    pub data: Vec<MaybeDataAttribute>,
    pub children: Vec<MaybeParsedValue<MaybeNodeData>>,
    /// Content-identity hash for nodes whose rendering is fully known at
    /// compile time.  Assigned by [`assign_static_hashes`] once the whole
    /// invocation has been parsed; `None` until then and for dynamic nodes.
    pub static_hash: Option<u64>,
}

impl MaybeNodeData {
    fn new(
        kind: NestedNodeKind<'static>,
        attrs: Vec<MaybeAttribute>,
        children: Vec<MaybeParsedValue<MaybeNodeData>>,
    ) -> Self {
        Self {
            kind,
            attrs,
            data: Vec::new(),
            children,
            static_hash: None,
        }
    }

    fn tag_name(&self) -> Option<EId> {
        match self.kind {
            NestedNodeKind::Element { tag_name } => Some(tag_name),
            _ => None,
        }
    }

    fn static_attr(&self, name: AId) -> Option<&str> {
        self.attrs
            .iter()
            .find_map(|attr| match (&attr.name, &attr.value) {
                (aid, MaybeParsedValue::Value(value)) if *aid == name => Some(value.as_str()),
                _ => None,
            })
    }

    /// Hash of this node's own lexical content and its inline children.
    /// References to other elements are resolved separately, see
    /// [`assign_static_hashes`].
    fn hash_content(&self, hasher: &mut impl Hasher) {
        match &self.kind {
            NestedNodeKind::Root => 2u8.hash(hasher),
            NestedNodeKind::Element { tag_name } => {
                0u8.hash(hasher);
                (*tag_name as u16).hash(hasher);
            }
            NestedNodeKind::Text(storage) => {
                1u8.hash(hasher);
                match storage {
                    svgtree::roxmltree::StringStorage::Borrowed(s) => s.hash(hasher),
                    svgtree::roxmltree::StringStorage::Owned(s) => s.as_ref().hash(hasher),
                }
            }
        }

        self.attrs.len().hash(hasher);
        for attr in &self.attrs {
            attr.hash_static_content(hasher);
        }

        // Hints change what fframes does with a subtree, so they are part of its identity.
        self.data.len().hash(hasher);
        for data in &self.data {
            data.key.hash(hasher);
            if let MaybeParsedValue::Value(value) = &data.value {
                value.hash(hasher);
            }
        }

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
            data,
            children,
            static_hash,
        } = self;

        let children_tokens = tokenize_nodes(children);
        let static_hash_token = if let Some(hash) = static_hash {
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
                data: vec![#(#data),*].into_boxed_slice(),
            })
        }
        .to_tokens(tokens);
    }
}

/// Presentation attributes that children inherit from their ancestors
/// (mirrors `usvgr`'s inheritance rules).  A dynamic value on any of these
/// changes how every descendant renders, so descendants can not be static.
fn is_inheritable(aid: AId) -> bool {
    matches!(
        aid,
        AId::ClipRule
            | AId::Color
            | AId::ColorInterpolation
            | AId::ColorInterpolationFilters
            | AId::ColorRendering
            | AId::Direction
            | AId::Fill
            | AId::FillOpacity
            | AId::FillRule
            | AId::FontFamily
            | AId::FontKerning
            | AId::FontSize
            | AId::FontSizeAdjust
            | AId::FontStretch
            | AId::FontStyle
            | AId::FontVariant
            | AId::FontWeight
            | AId::GlyphOrientationHorizontal
            | AId::GlyphOrientationVertical
            | AId::ImageRendering
            | AId::Isolation
            | AId::LetterSpacing
            | AId::MarkerEnd
            | AId::MarkerMid
            | AId::MarkerStart
            | AId::MaskType
            | AId::MixBlendMode
            | AId::PaintOrder
            | AId::ShapeRendering
            | AId::Stroke
            | AId::StrokeDasharray
            | AId::StrokeDashoffset
            | AId::StrokeLinecap
            | AId::StrokeLinejoin
            | AId::StrokeMiterlimit
            | AId::StrokeOpacity
            | AId::StrokeWidth
            | AId::TextAnchor
            | AId::TextOverflow
            | AId::TextRendering
            | AId::UnicodeBidi
            | AId::VectorEffect
            | AId::Visibility
            | AId::WhiteSpace
            | AId::WordSpacing
            | AId::WritingMode
            // Not inheritable per spec, but usvgr resolves it from any ancestor.
            | AId::TextDecoration
            // `style` can set any of the above.
            | AId::Style
    )
}

/// Elements that establish a viewport for percentage lengths of their
/// descendants.
fn establishes_viewport(eid: EId) -> bool {
    matches!(eid, EId::Svg | EId::Symbol | EId::Pattern | EId::Marker)
}

fn is_viewport_attribute(aid: AId) -> bool {
    matches!(
        aid,
        AId::Width | AId::Height | AId::ViewBox | AId::PreserveAspectRatio
    )
}

/// Collect the element ids a static attribute value refers to:
/// `url(#id)` anywhere in the value and `href="#id"`.
fn collect_references(aid: AId, value: &str, out: &mut Vec<String>) {
    if aid == AId::Href {
        if let Some(id) = value.strip_prefix('#') {
            out.push(id.to_owned());
        }
        return;
    }

    let mut rest = value;
    while let Some(pos) = rest.find("url(") {
        rest = &rest[pos + "url(".len()..];
        let end = rest.find(')').unwrap_or(rest.len());
        let target = rest[..end].trim().trim_matches(['"', '\'']).trim();
        if let Some(id) = target.strip_prefix('#') {
            out.push(id.to_owned());
        }
        rest = &rest[end..];
    }
}

/// What a node inherits from its ancestors inside this macro invocation.
#[derive(Clone, Copy, Default)]
struct AncestorContext {
    /// An ancestor has a dynamic inheritable attribute.
    dynamic_inherited: bool,
    /// An ancestor viewport has dynamic dimensions, so percentage lengths
    /// resolve differently per frame.
    dynamic_viewport: bool,
    /// Hash of the static inheritable attributes and viewport dimensions of
    /// all ancestors.  Mixed into every descendant's hash so that the same
    /// `<rect/>` under `<g fill="red">` and `<g fill="blue">` gets different
    /// cache keys instead of evicting each other every frame.
    inherited_hash: u64,
}

/// Per-node facts gathered while walking the tree, in pre-order.
struct StaticCandidate {
    /// Hash of the node's own lexical content, `None` if the node or any of
    /// its inline descendants is dynamic.
    local_hash: Option<u64>,
    /// Ids referenced by the node or its inline descendants.
    references: Vec<String>,
}

fn collect_static_candidates(
    node: &MaybeNodeData,
    ctx: AncestorContext,
    candidates: &mut Vec<StaticCandidate>,
    ids: &mut std::collections::HashMap<String, usize>,
) -> (Option<u64>, Vec<String>) {
    use std::collections::hash_map::DefaultHasher;

    let index = candidates.len();
    candidates.push(StaticCandidate {
        local_hash: None,
        references: Vec::new(),
    });

    if let Some(id) = node.static_attr(AId::Id) {
        ids.entry(id.to_owned()).or_insert(index);
    }

    let mut is_static = !ctx.dynamic_inherited;
    let mut references = Vec::new();
    let mut child_ctx = ctx;

    let is_viewport = node.tag_name().is_some_and(establishes_viewport);
    for attr in &node.attrs {
        let affects_descendants =
            is_inheritable(attr.name) || (is_viewport && is_viewport_attribute(attr.name));

        match &attr.value {
            MaybeParsedValue::Value(value) => {
                collect_references(attr.name, value, &mut references);
                if ctx.dynamic_viewport && value.contains('%') {
                    is_static = false;
                }
                if affects_descendants {
                    let mut hasher = DefaultHasher::new();
                    child_ctx.inherited_hash.hash(&mut hasher);
                    (attr.name as u16).hash(&mut hasher);
                    value.hash(&mut hasher);
                    child_ctx.inherited_hash = hasher.finish();
                }
            }
            MaybeParsedValue::Expression(_) => {
                is_static = false;
                if is_inheritable(attr.name) {
                    child_ctx.dynamic_inherited = true;
                }
                if is_viewport && is_viewport_attribute(attr.name) {
                    child_ctx.dynamic_viewport = true;
                }
            }
        }
    }

    if node
        .data
        .iter()
        .any(|data| matches!(data.value, MaybeParsedValue::Expression(_)))
    {
        is_static = false;
    }

    for child in &node.children {
        match child {
            MaybeParsedValue::Value(child) => {
                let (child_hash, child_refs) =
                    collect_static_candidates(child, child_ctx, candidates, ids);
                is_static &= child_hash.is_some();
                references.extend(child_refs);
            }
            MaybeParsedValue::Expression(_) => is_static = false,
        }
    }

    references.sort();
    references.dedup();

    let local_hash = is_static.then(|| {
        let mut hasher = DefaultHasher::new();
        ctx.inherited_hash.hash(&mut hasher);
        node.hash_content(&mut hasher);
        hasher.finish()
    });

    candidates[index] = StaticCandidate {
        local_hash,
        references: references.clone(),
    };

    (local_hash, references)
}

/// Final hash of a candidate: its local hash mixed with the resolved hashes
/// of everything it references.  A reference that can not be resolved to a
/// static element in this invocation (an element defined elsewhere, a
/// dynamic element, or a reference cycle) makes the node dynamic.
// `None` is not resolved yet, `Some(None)` is resolved to no static hash.
#[allow(clippy::option_option)]
fn resolve_static_hash(
    index: usize,
    candidates: &[StaticCandidate],
    ids: &std::collections::HashMap<String, usize>,
    resolved: &mut [Option<Option<u64>>],
    visiting: &mut Vec<usize>,
) -> Option<u64> {
    use std::collections::hash_map::DefaultHasher;

    if let Some(result) = resolved[index] {
        return result;
    }
    if visiting.contains(&index) {
        return None;
    }

    let candidate = &candidates[index];
    let result = candidate.local_hash.and_then(|local_hash| {
        if candidate.references.is_empty() {
            return Some(local_hash);
        }

        visiting.push(index);
        let mut hasher = DefaultHasher::new();
        local_hash.hash(&mut hasher);
        let mut all_static = true;
        for reference in &candidate.references {
            if let Some(hash) = ids.get(reference).and_then(|&target| {
                resolve_static_hash(target, candidates, ids, resolved, visiting)
            }) {
                hash.hash(&mut hasher);
            } else {
                all_static = false;
                break;
            }
        }
        visiting.pop();

        all_static.then(|| hasher.finish())
    });

    resolved[index] = Some(result);
    result
}

fn write_static_hashes(node: &mut MaybeNodeData, hashes: &[Option<u64>], cursor: &mut usize) {
    node.static_hash = hashes[*cursor];
    *cursor += 1;
    for child in &mut node.children {
        if let MaybeParsedValue::Value(child) = child {
            write_static_hashes(child, hashes, cursor);
        }
    }
}

/// Assign `static_hash` to every node whose rendering is fully determined by
/// the macro input.  Renderers use the hash as a cache key, so a node must
/// only get one when nothing that influences its output can change between
/// frames: its own attributes and inline children, the attributes it
/// inherits from ancestors in this invocation, the viewport its percentage
/// lengths resolve against, and every element it references by id.
///
/// What the macro can not see — a parent from another invocation that sets
/// `fill`, or the `<use>` site a `<symbol>` clone is rendered under — is
/// covered at runtime: renderers validate every cached entry with a
/// fingerprint of the resolved node before reusing it.
fn assign_static_hashes(nodes: &mut [MaybeParsedValue<MaybeNodeData>]) {
    let mut candidates = Vec::new();
    let mut ids = std::collections::HashMap::new();
    for node in nodes.iter() {
        if let MaybeParsedValue::Value(node) = node {
            collect_static_candidates(node, AncestorContext::default(), &mut candidates, &mut ids);
        }
    }

    let mut resolved = vec![None; candidates.len()];
    let hashes: Vec<Option<u64>> = (0..candidates.len())
        .map(|index| resolve_static_hash(index, &candidates, &ids, &mut resolved, &mut Vec::new()))
        .collect();

    let mut cursor = 0;
    for node in nodes.iter_mut() {
        if let MaybeParsedValue::Value(node) = node {
            write_static_hashes(node, &hashes, &mut cursor);
        }
    }
}

/// Either inline nodes as values or if we have subtrees create `into_flattened` expression unwrapping trees.
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

static ATTRIBUTE_NAMES_LIST: std::sync::LazyLock<Vec<&'static str>> =
    std::sync::LazyLock::new(|| {
        svgtree::ATTRIBUTES
            .entries
            .iter()
            .map(|(name, _)| *name)
            .collect()
    });

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
        }
        return Err(syn::Error::new(attribute_span, format!("Found non svg namespace: {}, please make sure that only svg xml is supported.\nPlease make sure to enter a valid SVG namespace => {SVG_NS}", attribute.value_as_string().unwrap_or_default())));
    }

    let aid = AId::from_str(attribute.name_as_string().unwrap().as_str())
        .ok_or_else(|| detailed_attribute_error(attribute_name.as_str(), attribute_span))?;

    if aid == AId::Class {
        return Err(syn::Error::new(
            attribute_span,
            "The `class` attribute is not supported. Neither classes nor <style /> tags are supported because raw css is incredibly hard to support.\n\nUse inlined styles or style=\"\" attribute instead.",
        ));
    }

    let value = maybe_value(attribute, quote::ToTokens::into_token_stream, |value| {
        Ok(String::from(value))
    })?;

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
            parsed_nodes.push(MaybeParsedValue::Value(MaybeNodeData::new(
                svgtree::NestedNodeKind::Text(svgtree::roxmltree::StringStorage::new_owned(
                    text_content.as_str(),
                )),
                vec![],
                vec![],
            )));

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

        let mut element = MaybeNodeData::new(
            NestedNodeKind::Element { tag_name },
            parse_element_attributes(node, tag_name)?,
            map_text_node_children(node.children.as_slice(), tag_name, fframes_crate_ident)?,
        );
        element.data = parse_data_attributes(node)?;
        parsed_nodes.push(MaybeParsedValue::Value(element));
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

const DATA_FFRAMES_PREFIX: &str = "data-fframes-";

fn is_data_attribute(attribute: &Node) -> bool {
    attribute
        .name_as_string()
        .is_some_and(|name| name.starts_with(DATA_FFRAMES_PREFIX))
}

/// `data-fframes-*` attributes, static or `{dynamic}`.
fn parse_data_attributes(node: &Node) -> syn::Result<Vec<MaybeDataAttribute>> {
    node.attributes
        .iter()
        .filter(|attribute| is_data_attribute(attribute))
        .map(|attribute| {
            let name = attribute.name_as_string().unwrap_or_default();
            let key = name[DATA_FFRAMES_PREFIX.len()..].to_owned();
            if key.is_empty() {
                return Err(syn::Error::new(
                    attribute.name_span().unwrap_or_else(Span::call_site),
                    "`data-fframes-` needs a name after the prefix, e.g. `data-fframes-inspect`.",
                ));
            }
            let value = maybe_value(attribute, quote::ToTokens::into_token_stream, |value| {
                Ok(String::from(value))
            })?;
            Ok(MaybeDataAttribute { key, value })
        })
        .collect()
}

fn parse_element_attributes(node: &Node, eid: EId) -> Result<Vec<MaybeAttribute>, syn::Error> {
    let attributes = node
        .attributes
        .iter()
        .filter(|attribute| !is_data_attribute(attribute))
        .filter_map(|attribute| -> Option<syn::Result<_>> {
            Some(
                maybe_parse_svg_attribute(attribute, eid)
                    .transpose()?
                    .map(|(aid, value)| MaybeAttribute {
                        name: aid,
                        value,
                        element: eid,
                    }),
            )
        })
        .collect::<syn::Result<Vec<_>>>()?;

    Ok(expand_static_style(attributes))
}

/// Splits a static `style="fill: red; opacity: .5"` into the presentation attributes it
/// sets, so they are parsed once here instead of on every frame. `usvgr` does the same
/// at runtime: only presentation properties are kept, a later declaration wins and a
/// declaration overrides the element's attribute of the same name. Styles that need the
/// runtime (`inherit`, or an explicit attribute with the same name, whose priority
/// depends on the attribute order) are left as they are.
fn expand_static_style(attributes: Vec<MaybeAttribute>) -> Vec<MaybeAttribute> {
    let Some(style_index) = attributes.iter().position(|attr| {
        attr.name == AId::Style && matches!(attr.value, MaybeParsedValue::Value(_))
    }) else {
        return attributes;
    };

    let MaybeParsedValue::Value(style) = &attributes[style_index].value else {
        return attributes;
    };
    let element = attributes[style_index].element;

    let mut declarations: Vec<(AId, String)> = Vec::new();
    for declaration in simplecss::DeclarationTokenizer::from(style.as_str()) {
        let Some(aid) = AId::from_str(declaration.name) else {
            continue;
        };
        if !aid.is_presentation() {
            continue;
        }
        if declaration.value == "inherit" {
            return attributes;
        }

        declarations.retain(|(existing, _)| *existing != aid);
        declarations.push((aid, declaration.value.to_owned()));
    }

    let clashes = attributes.iter().enumerate().any(|(index, attr)| {
        index != style_index && declarations.iter().any(|(aid, _)| *aid == attr.name)
    });
    if clashes {
        return attributes;
    }

    let mut expanded = Vec::with_capacity(attributes.len() + declarations.len());
    for (index, attr) in attributes.into_iter().enumerate() {
        if index == style_index {
            expanded.extend(declarations.drain(..).map(|(name, value)| MaybeAttribute {
                name,
                value: MaybeParsedValue::Value(value),
                element,
            }));
        } else {
            expanded.push(attr);
        }
    }

    expanded
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

        let mut element =
            MaybeNodeData::new(svgtree::NestedNodeKind::Element { tag_name }, attrs, children);
        element.data = parse_data_attributes(node)?;
        parsed_nodes.push(MaybeParsedValue::Value(element));
    }

    Ok(parsed_nodes)
}

pub fn nodes_to_svgtree(
    nodes: &[Node],
    fframes_crate_ident: &syn::Ident,
) -> syn::Result<TokenStream> {
    let mut nodes = map_inline_or_runtime_nodes(nodes, fframes_crate_ident)?;
    assign_static_hashes(&mut nodes);

    let tokens = tokenize_nodes(&nodes);
    let output_tree = quote! {
        NestedSvgDocument::from_nodes(#tokens)
    };

    Ok(output_tree)
}
