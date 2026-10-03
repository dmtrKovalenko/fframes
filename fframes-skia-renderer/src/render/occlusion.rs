use fframes::usvgr;
use skia_safe::{Canvas, ColorType, Contains, IRect, PathFillType, Rect, RoundOut};

use super::RenderCache;

// Coverage is valid only at this surface's scale, never in a reusable picture.
pub(super) fn visible_nodes(
    group: &usvgr::Group,
    canvas: &Canvas,
    cache: &mut RenderCache,
) -> Option<Vec<bool>> {
    #[cfg(test)]
    if cache.disable_occlusion {
        return None;
    }
    if group.children().len() < 64
        || canvas.image_info().color_type() == ColorType::Unknown
        || !canvas.is_clip_rect()
    {
        return None;
    }
    let matrix = canvas.local_to_device_as_3x3();
    if matrix.has_perspective() {
        return None;
    }
    let clip = canvas.device_clip_bounds()?.with_inset((2, 2));
    let bounds = group.bounding_box();
    let bounds = matrix
        .map_rect(Rect::from_xywh(
            bounds.x(),
            bounds.y(),
            bounds.width(),
            bounds.height(),
        ))
        .0;
    if !bounds.is_finite() || bounds.is_empty() {
        return None;
    }
    let mut covered = Coverage::new(bounds.with_outset((2., 2.)));
    let mut visible = vec![true; group.children().len()];
    let mut hidden = 0;
    for (i, node) in group.children().iter().enumerate().rev() {
        if group.children().len() - i > 128 && hidden == 0 && !covered.overlaps {
            return None;
        }
        let (path, fast_shape) = match node {
            usvgr::Node::Path(path) => (&**path, None),
            usvgr::Node::FastShape(fast_shape) => (fast_shape.path(), Some(fast_shape.kind())),
            _ => {
                // A group can blend with its backdrop, so coverage cannot cross it.
                covered.rows.fill(0);
                covered.overlaps = false;
                continue;
            }
        };
        if path.visibility() != usvgr::Visibility::Visible {
            continue;
        }
        if path.stroke().is_some() {
            continue;
        }
        let bounds = path.data().bounds();
        let bounds = matrix
            .map_rect(Rect::from_xywh(
                bounds.x(),
                bounds.y(),
                bounds.width(),
                bounds.height(),
            ))
            .0;
        if !bounds.is_finite() {
            covered.rows.fill(0);
            covered.overlaps = false;
            continue;
        }
        // Keep both antialiased edges and the shared clip's antialiased fringe.
        let outer: IRect = bounds.with_outset((1., 1.)).round_out();
        if clip.contains(&outer) && covered.contains(bounds.with_outset((1., 1.))) {
            visible[i] = false;
            hidden += 1;
            continue;
        }
        let Some(fill) = path.fill() else { continue };
        if fill.opacity().get() != 1. || !matches!(fill.paint(), usvgr::Paint::Color(_)) {
            continue;
        }
        if let Some(kind) = fast_shape
            && matrix.is_scale_translate()
        {
            covered.add(inscribed_rect(kind, &matrix).with_inset((1., 1.)));
            continue;
        }
        let mut shape = super::geometry_path(path, fast_shape, cache).make_transform(&matrix);
        shape.set_fill_type(match fill.rule() {
            usvgr::FillRule::NonZero => PathFillType::Winding,
            usvgr::FillRule::EvenOdd => PathFillType::EvenOdd,
        });
        // Convex paths can certify an interior rectangle; concave paths and holes
        // simply contribute no coverage. No SVG tag-specific rendering is needed.
        for fraction in [0., 0.125, 0.25, 0.375] {
            let inner = bounds.with_inset((bounds.width() * fraction, bounds.height() * fraction));
            if shape.conservatively_contains_rect(inner) {
                covered.add(inner.with_inset((1., 1.)));
                break;
            }
        }
    }
    Some(visible)
}

/// A rectangle inside a fast shape, in device space: inset by where the corner arcs (the whole
/// outline of an ellipse) pass 45 degrees. Only valid for a scale-translate `matrix`.
fn inscribed_rect(kind: usvgr::FastShapeKind, matrix: &skia_safe::Matrix) -> Rect {
    let (rect, rx, ry) = match kind {
        usvgr::FastShapeKind::Ellipse(rect) => (rect, rect.width() / 2., rect.height() / 2.),
        usvgr::FastShapeKind::RoundRect { rect, rx, ry } => (rect, rx, ry),
    };
    let inset = 1. - std::f32::consts::FRAC_1_SQRT_2;
    let inner = Rect::from_ltrb(rect.left(), rect.top(), rect.right(), rect.bottom())
        .with_inset((rx * inset, ry * inset));
    matrix.map_rect(inner).0
}

struct Coverage {
    rows: [u64; 64],
    overlaps: bool,
    bounds: Rect,
    sx: f32,
    sy: f32,
}

impl Coverage {
    fn new(bounds: Rect) -> Self {
        Self {
            rows: [0; 64],
            overlaps: false,
            sx: 64. / bounds.width(),
            sy: 64. / bounds.height(),
            bounds,
        }
    }

    fn cells(&self, rect: Rect, inward: bool) -> Option<(usize, usize, u64)> {
        let coordinates = [
            (rect.left - self.bounds.left) * self.sx,
            (rect.top - self.bounds.top) * self.sy,
            (rect.right - self.bounds.left) * self.sx,
            (rect.bottom - self.bounds.top) * self.sy,
        ];
        let [left, top, right, bottom] = coordinates.map(|v| v.clamp(0., 64.));
        let (left, top, right, bottom) = if inward {
            (
                left.ceil() as usize,
                top.ceil() as usize,
                right.floor() as usize,
                bottom.floor() as usize,
            )
        } else {
            if !self.bounds.contains(rect) {
                return None;
            }
            (
                left.floor() as usize,
                top.floor() as usize,
                right.ceil() as usize,
                bottom.ceil() as usize,
            )
        };
        if left >= right || top >= bottom {
            return None;
        }
        Some((top, bottom, (u64::MAX << left) & (u64::MAX >> (64 - right))))
    }

    fn contains(&self, rect: Rect) -> bool {
        self.cells(rect, false).is_some_and(|(top, bottom, mask)| {
            self.rows[top..bottom].iter().all(|row| row & mask == mask)
        })
    }

    fn add(&mut self, rect: Rect) {
        if let Some((top, bottom, mask)) = self.cells(rect, true) {
            for row in &mut self.rows[top..bottom] {
                self.overlaps |= *row & mask != 0;
                *row |= mask;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use super::*;
    use crate::{SkiaBackend, SkiaCpuCtx};

    fn parse(content: &str) -> usvgr::Tree {
        usvgr::Tree::from_str(
            &format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128">{content}</svg>"#),
            &usvgr::Options::default(),
            &usvgr::fontdb::Database::new(),
        )
        .unwrap()
    }

    fn overlapping_paths() -> String {
        let mut paths = String::new();
        for i in 0..160 {
            let x = 20 + i % 7;
            let y = 20 + i % 11;
            write!(
                paths,
                r##"<path d="M{x} {y} h40 v40 h-40z" fill="#{:06x}"/>"##,
                i * 7919 % 0x00ff_ffff
            )
            .unwrap();
        }
        paths
    }

    fn compare(backend: &impl SkiaBackend, tree: &usvgr::Tree, transform: &skia_safe::Matrix) {
        let render = |disabled| {
            let (mut surface, mut context) = backend.create_skia_surface().unwrap();
            let mut cache = RenderCache {
                disable_occlusion: disabled,
                ..Default::default()
            };
            surface.canvas().clear(skia_safe::Color::TRANSPARENT);
            surface.canvas().concat(transform);
            super::super::render_tree(tree, surface.canvas(), &mut cache);
            if let Some(context) = context.as_mut() {
                context.flush_submit_and_sync_cpu();
            }
            let info = skia_safe::ImageInfo::new_n32_premul((128, 128), None);
            let mut pixels = vec![0; 128 * 128 * 4];
            assert!(surface.read_pixels(&info, &mut pixels, 128 * 4, (0, 0)));
            pixels
        };
        assert_eq!(render(false), render(true), "occlusion changed pixels");
    }

    fn check_backend(backend: &impl SkiaBackend) {
        let paths = overlapping_paths();
        let defs = r#"<defs>
            <clipPath id="c"><circle cx="55" cy="55" r="37"/></clipPath>
            <clipPath id="r"><rect x="20.25" y="20.25" width="42.5" height="42.5"/></clipPath>
            <mask id="m"><rect width="128" height="128" fill="white"/><circle cx="30" cy="40" r="20" fill="black"/></mask>
            <filter id="f"><feGaussianBlur stdDeviation="2"/></filter>
            <linearGradient id="g"><stop stop-color="red"/><stop offset="1" stop-color="blue" stop-opacity=".2"/></linearGradient>
        </defs>"#;
        for attributes in [
            "",
            r#"opacity=".45""#,
            r#"filter="url(#f)""#,
            r#"clip-path="url(#c)""#,
            r#"clip-path="url(#r)""#,
            r#"mask="url(#m)""#,
            r#"style="mix-blend-mode:multiply""#,
        ] {
            for top in [
                r#"<path d="M15 15h65v65h-65z" fill="blue"/>"#,
                r#"<path d="M10 10h75v75h-75z M30 30h30v30h-30z" fill-rule="evenodd"/>"#,
                r#"<circle cx="45" cy="45" r="44" fill="green"/>"#,
                r#"<path d="M5 85L50 0L90 85Z" fill="orange"/>"#,
                r#"<rect x="10" y="10" width="80" height="80" fill="url(#g)"/>"#,
                r#"<rect x="10" y="10" width="80" height="80" fill="blue" fill-opacity=".5"/>"#,
                r#"<g style="mix-blend-mode:multiply"><rect x="10" y="10" width="80" height="80" fill="blue"/></g>"#,
            ] {
                let tree = parse(&format!("{defs}<g {attributes}>{paths}{top}</g>"));
                for transform in [
                    skia_safe::Matrix::new_identity(),
                    skia_safe::Matrix::scale((0.25, 0.25)),
                    skia_safe::Matrix::new_all(1.1, 0.2, 0.25, -0.15, 0.9, 12.5, 0., 0., 1.),
                ] {
                    compare(backend, &tree, &transform);
                }
            }
        }
        let tree = parse(&format!(
            r#"{paths}<path d="M15 15h70v70h-70z" fill="blue"/>"#
        ));
        let (mut surface, _context) = backend.create_skia_surface().unwrap();
        let visible =
            visible_nodes(tree.root(), surface.canvas(), &mut RenderCache::new()).unwrap();
        assert!(visible.iter().filter(|visible| !**visible).count() >= 150);
    }

    #[test]
    fn occlusion_preserves_raster_pixels() {
        check_backend(&SkiaCpuCtx::new(128, 128));
    }

    #[test]
    fn coverage_rounds_inward_and_preserves_gaps() {
        let mut coverage = Coverage::new(Rect::from_wh(64., 64.));
        coverage.add(Rect::new(0.25, 0.25, 31.75, 63.75));
        coverage.add(Rect::new(32.25, 0.25, 63.75, 63.75));
        assert!(coverage.contains(Rect::new(1., 1., 31., 63.)));
        assert!(!coverage.contains(Rect::new(0., 0., 31., 63.)));
        assert!(!coverage.contains(Rect::new(1., 1., 63., 63.)));
        assert!(!coverage.contains(Rect::new(-1., 1., 31., 63.)));
    }

    #[test]
    fn non_overlapping_paths_stop_the_scan() {
        let mut paths = String::new();
        for i in 0..160 {
            write!(
                paths,
                r#"<path d="M{} {}h2v2h-2z"/>"#,
                i % 16 * 8,
                i / 16 * 8
            )
            .unwrap();
        }
        let tree = parse(&paths);
        let (mut surface, _) = SkiaCpuCtx::new(128, 128).create_skia_surface().unwrap();
        assert!(visible_nodes(tree.root(), surface.canvas(), &mut RenderCache::new()).is_none());
    }

    #[test]
    fn partial_overlap_keeps_scanning_for_hidden_paths() {
        let mut paths = r#"<path d="M50 35h5v5h-5z"/>"#.repeat(100);
        for i in 0..160 {
            write!(paths, r#"<path d="M{} 20h50v50h-50z"/>"#, f64::from(i) / 4.).unwrap();
        }
        let tree = parse(&paths);
        let backend = SkiaCpuCtx::new(128, 128);
        let (mut surface, _) = backend.create_skia_surface().unwrap();
        let visible =
            visible_nodes(tree.root(), surface.canvas(), &mut RenderCache::new()).unwrap();
        assert!(visible[..100].iter().all(|visible| !visible));
        compare(&backend, &tree, &skia_safe::Matrix::new_identity());
    }

    #[cfg(all(target_os = "macos", feature = "metal"))]
    #[test]
    fn occlusion_preserves_metal_pixels() {
        let backend = crate::metal::SkiaMetalCtx::new(128, 128).unwrap();
        check_backend(&backend);
    }
}
