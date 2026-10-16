//! Outlines and labels the elements of a frame that have an `id`, to tell which shape is which
//! in a rendered PNG (`frame --ids`).
use super::{FFramesRendererError, FFramesRendererResult, RgbaFrame, sheet};
use crate::usvgr;
use serde::Serialize;
use std::fmt::Write as _;
use usvgr::tiny_skia_path::PathSegment;

/// More labels than this make the picture unreadable; the rest are listed, not drawn.
const MAX_LABELS: usize = 400;

/// An element with an `id` and its bounding box in the video's coordinates.
#[derive(Debug, Clone, Serialize)]
pub struct LabelledElement {
    pub id: String,
    /// `path`, `group`, `image` or `text`.
    pub kind: &'static str,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// The outline as SVG path data in the video's coordinates (paths only).
    #[serde(skip)]
    outline: Option<String>,
}

fn path_data(path: &usvgr::tiny_skia_path::Path, transform: usvgr::Transform) -> Option<String> {
    let path = path.clone().transform(transform)?;
    let mut data = String::new();
    for segment in path.segments() {
        let _ = match segment {
            PathSegment::MoveTo(p) => write!(data, "M{} {} ", p.x, p.y),
            PathSegment::LineTo(p) => write!(data, "L{} {} ", p.x, p.y),
            PathSegment::QuadTo(a, p) => write!(data, "Q{} {} {} {} ", a.x, a.y, p.x, p.y),
            PathSegment::CubicTo(a, b, p) => {
                write!(data, "C{} {} {} {} {} {} ", a.x, a.y, b.x, b.y, p.x, p.y)
            }
            PathSegment::Close => write!(data, "Z "),
        };
    }
    Some(data)
}

fn collect(group: &usvgr::Group, out: &mut Vec<LabelledElement>) {
    for node in group.children() {
        if let usvgr::Node::Group(child) = node
            && child.opacity().get() == 0.
        {
            continue;
        }
        let id = node.id();
        if !id.is_empty() {
            let bbox = node.abs_bounding_box();
            if bbox.width().is_finite() && bbox.height().is_finite() {
                let (kind, outline) = match node {
                    usvgr::Node::Path(path) => {
                        ("path", path_data(path.data(), path.abs_transform()))
                    }
                    usvgr::Node::FastShape(shape) => (
                        "path",
                        shape
                            .to_path()
                            .and_then(|path| path_data(path.data(), path.abs_transform())),
                    ),
                    usvgr::Node::Group(_) => ("group", None),
                    usvgr::Node::Image(_) => ("image", None),
                    usvgr::Node::Text(_) => ("text", None),
                };
                out.push(LabelledElement {
                    id: id.to_owned(),
                    kind,
                    x: bbox.x(),
                    y: bbox.y(),
                    width: bbox.width(),
                    height: bbox.height(),
                    outline,
                });
            }
        }
        if let usvgr::Node::Group(child) = node {
            collect(child, out);
        }
    }
}

/// Every element of `tree` with a non-empty `id`, in drawing order.
pub fn labelled_elements(tree: &usvgr::Tree) -> Vec<LabelledElement> {
    let mut elements = Vec::new();
    collect(tree.root(), &mut elements);
    elements
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Draws `elements` of `tree` over `frame` (a render of the same tree): path outlines, boxes
/// around groups, images and text, and the `id` of each.
pub fn draw_labels(
    frame: &RgbaFrame,
    tree: &usvgr::Tree,
    elements: &[LabelledElement],
    fontdb: &usvgr::fontdb::Database,
    font_family: &str,
) -> FFramesRendererResult<RgbaFrame> {
    // Labels need a font even when the video brings none.
    let system;
    let fontdb = if fontdb.faces().next().is_some() {
        fontdb
    } else {
        let mut db = usvgr::fontdb::Database::new();
        db.load_system_fonts();
        system = db;
        &system
    };
    let size = tree.size();
    // Lines and type keep the same size in pixels however large the frame is rendered.
    let unit = size.width() / frame.width as f32;
    let font_size = 12. * unit;
    let family = escape_xml(&sheet::label_font_family(fontdb, font_family));

    let mut shapes = String::new();
    let mut labels = String::new();
    for element in elements.iter().take(MAX_LABELS) {
        let colour = if element.kind == "path" {
            "#ff2bd6"
        } else {
            "#ffd400"
        };
        match &element.outline {
            Some(outline) => {
                let _ = write!(
                    shapes,
                    r#"<path d="{outline}" fill="none" stroke="{colour}" stroke-width="{}" />"#,
                    1.2 * unit
                );
            }
            None => {
                let _ = write!(
                    shapes,
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="{colour}" stroke-width="{}" stroke-dasharray="{} {}" />"#,
                    element.x,
                    element.y,
                    element.width,
                    element.height,
                    1.2 * unit,
                    6. * unit,
                    4. * unit
                );
            }
        }
        // Paths are labelled at their centre, containers at their top left corner.
        let (x, y) = if element.kind == "path" {
            (
                element.x + element.width / 2.,
                element.y + element.height / 2.,
            )
        } else {
            (element.x + 2. * unit, element.y + font_size)
        };
        let text = escape_xml(&element.id);
        let width = text.chars().count() as f32 * font_size * 0.6 + 6. * unit;
        let anchor_x = if element.kind == "path" {
            x - width / 2.
        } else {
            x
        };
        let _ = write!(
            labels,
            r##"<rect x="{}" y="{}" width="{width}" height="{}" rx="{}" fill="#000000" fill-opacity="0.7" /><text x="{}" y="{}" font-size="{font_size}" font-family="{family}" fill="#ffffff">{text}</text>"##,
            anchor_x,
            y - font_size * 0.95,
            font_size * 1.3,
            2. * unit,
            anchor_x + 3. * unit,
            y,
        );
    }

    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">{shapes}{labels}</svg>"#,
        frame.width,
        frame.height,
        size.width(),
        size.height()
    );
    let overlay = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), fontdb)
        .map_err(|err| FFramesRendererError::Internal(format!("element labels: {err}")))?;
    let mut pixmap = sheet::to_pixmap(frame)?;
    let ctx = svgr::Context::new_from_pixmap_unsafe(&pixmap);
    svgr::render(
        &overlay,
        super::fit_transform(&overlay, frame.width, frame.height),
        &mut pixmap.as_mut(),
        &mut svgr::SvgrCache::none(),
        &svgr::PixmapPool::new(),
        &ctx,
    );
    Ok(RgbaFrame::from_premultiplied(
        frame.width,
        frame.height,
        pixmap.take(),
    ))
}
