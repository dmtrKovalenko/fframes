//! Frame diagnostics: problems that do not fail a render but produce a wrong picture, such as
//! a missing image, an unknown font family or text pushed off the canvas. The collector reports
//! them without rasterizing anything, from three sources:
//!
//! - media lookups through `FFramesContext::get_*` that return `None`,
//! - warnings of the SVG converter (missing fonts, glyphs, font fallbacks),
//! - a walk over the converted tree (text outside the canvas, non-finite transforms, empty frames).
//!
//! Collection is scoped to the current thread, so it costs nothing unless a caller opted in:
//!
//! ```ignore
//! let (pixels, diagnostics) = fframes::diagnostics::collect(|| previewer.render(frame));
//! ```
use serde::Serialize;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Audio,
    Video,
    Subtitles,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Diagnostic {
    /// `ctx.get_image/get_audio/get_video/get_subtitles` did not find the file.
    MissingMedia { media: MediaKind, name: String },
    /// No font in the database matches the requested family, the text is drawn with a fallback.
    MissingFont { message: String },
    /// None of the loaded fonts has a glyph for a character, it is not drawn.
    MissingGlyph { message: String },
    /// Any other warning of the SVG converter.
    RendererWarning { message: String },
    /// Text is partially outside the canvas, it is cut off in the video.
    TextClipped { text: String, bbox: [f32; 4] },
    /// Text is entirely outside the canvas (fine while it animates in or out).
    TextOffCanvas { text: String, bbox: [f32; 4] },
    /// A node's transform contains NaN or infinity, the node is not drawn.
    InvalidTransform { id: String },
    /// The frame has no visible content besides the background color.
    EmptyFrame,
    /// `render_frame` panicked.
    Panic { message: String },
}

impl Diagnostic {
    /// Identity of a finding across frames: the same text moving around is one finding.
    pub fn group_key(&self) -> String {
        match self {
            Diagnostic::TextClipped { text, .. } => format!("text_clipped:{text}"),
            Diagnostic::TextOffCanvas { text, .. } => format!("text_off_canvas:{text}"),
            other => other.to_string(),
        }
    }

    pub fn severity(&self) -> Severity {
        match self {
            Diagnostic::Panic { .. }
            | Diagnostic::MissingMedia { .. }
            | Diagnostic::MissingGlyph { .. }
            | Diagnostic::InvalidTransform { .. } => Severity::Error,
            Diagnostic::MissingFont { .. }
            | Diagnostic::TextClipped { .. }
            | Diagnostic::EmptyFrame
            | Diagnostic::RendererWarning { .. } => Severity::Warning,
            Diagnostic::TextOffCanvas { .. } => Severity::Info,
        }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let [x, y, w, h] = match self {
            Diagnostic::TextClipped { bbox, .. } | Diagnostic::TextOffCanvas { bbox, .. } => *bbox,
            _ => [0.; 4],
        };

        match self {
            Diagnostic::MissingMedia { media, name } => {
                write!(f, "{media:?} \"{name}\" is not in the media provider")
            }
            Diagnostic::MissingFont { message }
            | Diagnostic::MissingGlyph { message }
            | Diagnostic::RendererWarning { message } => write!(f, "{message}"),
            Diagnostic::TextClipped { text, .. } => write!(
                f,
                "text \"{text}\" is cut off by the canvas edge (x={x:.0} y={y:.0} w={w:.0} h={h:.0})"
            ),
            Diagnostic::TextOffCanvas { text, .. } => write!(
                f,
                "text \"{text}\" is outside the canvas (x={x:.0} y={y:.0} w={w:.0} h={h:.0})"
            ),
            Diagnostic::InvalidTransform { id } => {
                write!(f, "node \"{id}\" has a NaN or infinite transform")
            }
            Diagnostic::EmptyFrame => write!(f, "frame is empty"),
            Diagnostic::Panic { message } => write!(f, "render_frame panicked: {message}"),
        }
    }
}

thread_local! {
    static COLLECTOR: RefCell<Option<Vec<Diagnostic>>> = const { RefCell::new(None) };
}

/// Media that was requested but missing during any render in this process. Unlike the scoped
/// collector this is always on (it only records on a miss) so full renders can report it.
static MISSING_MEDIA: Mutex<BTreeSet<(MediaKind, String)>> = Mutex::new(BTreeSet::new());

/// Runs `f` and returns everything reported on this thread while it ran. Nested calls report
/// to the innermost collector.
pub fn collect<R>(f: impl FnOnce() -> R) -> (R, Vec<Diagnostic>) {
    install_log_capture();
    let previous = COLLECTOR.with(|c| c.borrow_mut().replace(Vec::new()));
    let result = f();
    let collected = COLLECTOR.with(|c| std::mem::replace(&mut *c.borrow_mut(), previous));

    (result, dedup(collected.unwrap_or_default()))
}

/// Reports a diagnostic to the collector of the current thread, if there is one.
pub fn report(diagnostic: Diagnostic) {
    COLLECTOR.with(|c| {
        if let Some(collected) = c.borrow_mut().as_mut() {
            collected.push(diagnostic);
        }
    });
}

pub(crate) fn report_missing_media(media: MediaKind, name: &str) {
    if let Ok(mut missing) = MISSING_MEDIA.lock()
        && !missing.iter().any(|(k, n)| *k == media && n == name)
    {
        missing.insert((media, name.to_owned()));
    }

    report(Diagnostic::MissingMedia {
        media,
        name: name.to_owned(),
    });
}

/// Drains the media files that were requested but missing since the last call.
pub fn take_missing_media() -> Vec<(MediaKind, String)> {
    MISSING_MEDIA
        .lock()
        .map(|mut missing| std::mem::take(&mut *missing).into_iter().collect())
        .unwrap_or_default()
}

fn dedup(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut unique: Vec<Diagnostic> = Vec::with_capacity(diagnostics.len());
    for diagnostic in diagnostics {
        if !unique.contains(&diagnostic) {
            unique.push(diagnostic);
        }
    }
    unique
}

/// Classifies a warning of the SVG converter.
fn from_renderer_warning(message: String) -> Diagnostic {
    if message.starts_with("No match for") {
        Diagnostic::MissingFont { message }
    } else if message.starts_with("No fonts with a") {
        Diagnostic::MissingGlyph { message }
    } else {
        Diagnostic::RendererWarning { message }
    }
}

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            report(from_renderer_warning(record.args().to_string()));
        }
    }

    fn flush(&self) {}
}

static CAPTURE_LOGGER: CaptureLogger = CaptureLogger;

/// Routes `log` warnings (the SVG converter reports missing fonts through it) into the
/// diagnostics collector. Does nothing if the application installed its own logger.
pub fn install_log_capture() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        if log::set_logger(&CAPTURE_LOGGER).is_ok() {
            log::set_max_level(log::LevelFilter::Warn);
        }
    });
}

/// Walks a converted frame and reports layout problems. `width`/`height` is the canvas size.
pub fn inspect_tree(tree: &usvgr::Tree, width: f32, height: f32) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let root = tree.root();

    if !root.has_children() {
        diagnostics.push(Diagnostic::EmptyFrame);
    }

    inspect_group(root, width, height, &mut diagnostics);
    diagnostics
}

fn inspect_group(group: &usvgr::Group, width: f32, height: f32, out: &mut Vec<Diagnostic>) {
    if !group.abs_transform().is_finite() {
        out.push(Diagnostic::InvalidTransform {
            id: group.id().to_owned(),
        });
        return;
    }

    for node in group.children() {
        match node {
            usvgr::Node::Group(group) => {
                // Content under a clip path or mask is cut on purpose (scrolling lists,
                // reveals) and invisible groups do not matter.
                if group.clip_path().is_some()
                    || group.mask().is_some()
                    || group.opacity().get() == 0.
                {
                    check_transforms(group, out);
                } else {
                    inspect_group(group, width, height, out);
                }
            }
            usvgr::Node::Text(text) => {
                if !text.abs_transform().is_finite() {
                    out.push(Diagnostic::InvalidTransform {
                        id: text.id().to_owned(),
                    });
                    continue;
                }

                let bbox = text.abs_bounding_box();
                let rect = [bbox.x(), bbox.y(), bbox.width(), bbox.height()];
                let content = text
                    .chunks()
                    .iter()
                    .map(usvgr::TextChunk::text)
                    .collect::<Vec<_>>()
                    .join(" ");
                // A pixel of tolerance for anti-aliasing and rounding at the edges.
                let inside = bbox.left() >= -1.
                    && bbox.top() >= -1.
                    && bbox.right() <= width + 1.
                    && bbox.bottom() <= height + 1.;
                let disjoint = bbox.right() <= 0.
                    || bbox.bottom() <= 0.
                    || bbox.left() >= width
                    || bbox.top() >= height;

                if disjoint {
                    out.push(Diagnostic::TextOffCanvas {
                        text: content,
                        bbox: rect,
                    });
                } else if !inside {
                    out.push(Diagnostic::TextClipped {
                        text: content,
                        bbox: rect,
                    });
                }
            }
            usvgr::Node::Path(_) | usvgr::Node::FastShape(_) => {
                if !node.abs_transform().is_finite() {
                    out.push(Diagnostic::InvalidTransform {
                        id: node.id().to_owned(),
                    });
                }
            }
            usvgr::Node::Image(image) => {
                if !image.abs_transform().is_finite() {
                    out.push(Diagnostic::InvalidTransform {
                        id: image.id().to_owned(),
                    });
                }
            }
        }
    }
}

/// Only the transform check, for subtrees whose layout is intentionally cut.
fn check_transforms(group: &usvgr::Group, out: &mut Vec<Diagnostic>) {
    if !group.abs_transform().is_finite() {
        out.push(Diagnostic::InvalidTransform {
            id: group.id().to_owned(),
        });
        return;
    }
    for node in group.children() {
        if let usvgr::Node::Group(group) = node {
            check_transforms(group, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_only_inside_the_scope() {
        report(Diagnostic::EmptyFrame);
        let ((), collected) = collect(|| {
            report(Diagnostic::EmptyFrame);
            report(Diagnostic::EmptyFrame);
        });
        assert_eq!(collected, vec![Diagnostic::EmptyFrame]);

        let ((), nothing) = collect(|| ());
        assert_eq!(nothing, Vec::<Diagnostic>::new());
    }

    #[test]
    fn classifies_converter_warnings() {
        assert!(matches!(
            from_renderer_warning("No match for '\"Foo\"' font-family.".into()),
            Diagnostic::MissingFont { .. }
        ));
        assert!(matches!(
            from_renderer_warning("No fonts with a x/U+78 character were found.".into()),
            Diagnostic::MissingGlyph { .. }
        ));
    }

    #[test]
    fn finds_clipped_and_off_canvas_text() {
        // A font from the repository instead of the system fonts: usvgr lays the text out with
        // its default family, "Times New Roman", which the CI runner does not have, and text
        // without a font has no box to be clipped.
        let mut fontdb = usvgr::fontdb::Database::new();
        fontdb
            .load_font_file(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../cargo-fframes/templates/DMSans-Medium.ttf"
            ))
            .expect("the scaffolder's font is part of the repository");
        let options = usvgr::Options {
            font_family: "DM Sans".to_owned(),
            ..Default::default()
        };
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
            <text x="150" y="50" font-size="40">clipped</text>
            <text x="500" y="50" font-size="40">gone</text>
            <text x="10" y="50" font-size="10">ok</text>
            <clipPath id="c"><rect width="200" height="100"/></clipPath>
            <g clip-path="url(#c)"><text x="150" y="50" font-size="40">scrolling</text></g>
        </svg>"#;
        let tree = usvgr::Tree::from_str(svg, &options, &fontdb).unwrap();
        let diagnostics = inspect_tree(&tree, 200., 100.);

        assert!(
            diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::TextClipped { text, .. } if text == "clipped"))
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::TextOffCanvas { text, .. } if text == "gone"))
        );
        assert!(
            !diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::TextClipped { text, .. } | Diagnostic::TextOffCanvas { text, .. } if text == "ok" || text == "scrolling"))
        );
    }
}
