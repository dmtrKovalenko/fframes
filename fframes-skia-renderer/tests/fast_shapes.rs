//! `usvgr::Options::fast_shapes` keeps `circle`, `ellipse` and rounded `rect` as
//! `Node::FastShape`, which this renderer draws with Skia's analytic ovals and rounded rects
//! instead of the generic path. Both must look the same: same coverage, and dashes starting
//! where the SVG spec's path decomposition starts.

use fframes::usvgr;
use fframes_skia_renderer::render::{RenderCache, render_tree};
use skia_safe::{AlphaType, ColorType, ImageInfo};

const SIZE: i32 = 200;

fn tree(svg: &str, fast_shapes: bool) -> usvgr::Tree {
    let options = usvgr::Options {
        fast_shapes,
        ..Default::default()
    };
    usvgr::Tree::from_str(svg, &options, &usvgr::fontdb::Database::new()).expect("valid svg")
}

fn render(tree: &usvgr::Tree) -> Vec<u8> {
    let info = ImageInfo::new(
        (SIZE, SIZE),
        ColorType::RGBA8888,
        AlphaType::Premul,
        skia_safe::ColorSpace::new_srgb(),
    );
    let mut surface = skia_safe::surfaces::raster(&info, None, None).expect("raster surface");
    surface.canvas().clear(skia_safe::Color::WHITE);
    render_tree(tree, surface.canvas(), &mut RenderCache::new());

    let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
    assert!(surface.read_pixels(&info, &mut pixels, (SIZE * 4) as usize, (0, 0)));
    pixels
}

fn count_fast_shapes(group: &usvgr::Group) -> usize {
    group
        .children()
        .iter()
        .map(|node| match node {
            usvgr::Node::FastShape(_) => 1,
            usvgr::Node::Group(group) => count_fast_shapes(group),
            _ => 0,
        })
        .sum()
}

fn svg(body: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{SIZE}" height="{SIZE}" viewBox="0 0 200 200">{body}</svg>"#
    )
}

/// Renders `body` as paths and as fast shapes; returns how many pixels differ by more than
/// anti-aliasing does. Native ovals are antialiased a little differently (up to ~45 levels on
/// edge pixels), and dashes are measured along Skia's exact oval instead of our cubics (448.13
/// vs 448.46 around a 90x50 ellipse), which moves dash ends by up to a third of a pixel. A dash
/// pattern that starts elsewhere changes hundreds of pixels completely.
fn differing_pixels(body: &str) -> usize {
    let svg = svg(body);
    let paths = render(&tree(&svg, false));
    let shapes = render(&tree(&svg, true));
    paths
        .chunks_exact(4)
        .zip(shapes.chunks_exact(4))
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 96))
        .count()
}

#[test]
fn only_round_shapes_become_fast_shapes() {
    let svg = svg(r#"
        <circle cx="50" cy="50" r="20"/>
        <ellipse cx="100" cy="50" rx="30" ry="10"/>
        <rect x="10" y="100" width="50" height="30" rx="8"/>
        <rect x="70" y="100" width="50" height="30"/>
        <polygon points="150,100 190,100 170,130"/>
        <line x1="10" y1="190" x2="190" y2="190" stroke="black"/>
    "#);

    assert_eq!(count_fast_shapes(tree(&svg, false).root()), 0);
    assert_eq!(count_fast_shapes(tree(&svg, true).root()), 3);
}

#[test]
fn fast_shapes_render_like_paths() {
    let cases = [
        (
            "filled circle",
            r#"<circle cx="100" cy="100" r="70" fill="teal"/>"#,
        ),
        (
            "filled ellipse",
            r#"<ellipse cx="100" cy="100" rx="90" ry="40" fill="teal"/>"#,
        ),
        (
            "stroked circle",
            r#"<circle cx="100" cy="100" r="70" fill="none" stroke="navy" stroke-width="9"/>"#,
        ),
        (
            "dashed circle",
            r#"<circle cx="100" cy="100" r="70" fill="none" stroke="navy" stroke-width="10" stroke-dasharray="40 20"/>"#,
        ),
        (
            "dashed ellipse",
            r#"<ellipse cx="100" cy="100" rx="90" ry="50" fill="none" stroke="navy" stroke-width="8" stroke-dasharray="30 15" stroke-dashoffset="7"/>"#,
        ),
        (
            "rounded rect",
            r#"<rect x="20" y="40" width="160" height="120" rx="24" ry="16" fill="orange" stroke="black" stroke-width="4"/>"#,
        ),
        (
            "dashed rounded rect",
            r#"<rect x="20" y="40" width="160" height="120" rx="30" fill="none" stroke="black" stroke-width="6" stroke-dasharray="25 10"/>"#,
        ),
        (
            "pill",
            r#"<rect x="20" y="70" width="160" height="60" rx="100" fill="orange" stroke="black" stroke-width="4" stroke-dasharray="20 8"/>"#,
        ),
        (
            "non-uniform scale",
            r#"<g transform="scale(1.8 0.6)"><circle cx="55" cy="160" r="40" fill="purple" stroke="black" stroke-width="5"/></g>"#,
        ),
        (
            "skewed rounded rect",
            r#"<g transform="skewX(20)"><rect x="10" y="40" width="120" height="100" rx="20" fill="olive" stroke="black" stroke-width="6" stroke-dasharray="12 6"/></g>"#,
        ),
        (
            "gradient and opacity",
            r#"<defs><linearGradient id="g"><stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient></defs>
               <circle cx="100" cy="100" r="80" fill="url(#g)" opacity="0.6"/>"#,
        ),
    ];

    // Occlusion culling only runs for groups of 64 or more children; fast shapes certify the
    // area they cover analytically, so overlapping stacks check that nothing visible is culled.
    let stack = |transform: &str| {
        let shapes: String = (0..150)
            .map(|i| {
                let (x, y) = (80 + (i * 7) % 40, 80 + (i * 11) % 40);
                let color = format!(
                    "rgb({},{},{})",
                    (i * 41) % 256,
                    (i * 97) % 256,
                    (i * 13) % 256
                );
                if i % 3 == 0 {
                    format!(
                        r#"<rect x="{}" y="{}" width="70" height="60" rx="18" fill="{color}"/>"#,
                        x - 35,
                        y - 30
                    )
                } else {
                    format!(
                        r#"<ellipse cx="{x}" cy="{y}" rx="{}" ry="32" fill="{color}"/>"#,
                        30 + i % 15
                    )
                }
            })
            .collect();
        format!(r#"<g transform="{transform}">{shapes}</g>"#)
    };
    let stacks = [
        ("overlapping stack", stack("translate(0 0)")),
        ("scaled stack", stack("translate(10 0) scale(0.9 1.1)")),
        ("rotated stack", stack("rotate(12 100 100)")),
    ];

    let failures: Vec<_> = cases
        .iter()
        .map(|(name, body)| (*name, body.to_string()))
        .chain(stacks.iter().map(|(name, body)| (*name, body.clone())))
        .map(|(name, body)| (name, differing_pixels(&body)))
        .filter(|(_, differing)| *differing > 24)
        .collect();
    assert!(
        failures.is_empty(),
        "fast shapes render differently from paths (case, differing pixels): {failures:?}"
    );
}

fn leaves(group: &usvgr::Group, out: &mut Vec<usvgr::Node>) {
    for node in group.children() {
        match node {
            usvgr::Node::Group(group) => leaves(group, out),
            node => out.push(node.clone()),
        }
    }
}

/// Fast shapes skip building and measuring the outline; their bounding boxes come from the
/// rectangle and stroke width and must equal what measuring the outline gives.
#[test]
fn fast_shape_bounding_boxes_match_paths() {
    let svg = svg(r#"
        <circle cx="50" cy="50" r="20" fill="red"/>
        <circle cx="50" cy="50" r="20" fill="none" stroke="red" stroke-width="7"/>
        <ellipse cx="120" cy="60" rx="40" ry="15" stroke="red" stroke-width="3" stroke-linejoin="miter"/>
        <rect x="10" y="100" width="80" height="40" rx="10" ry="6" stroke="red" stroke-width="5"/>
        <rect x="110" y="110" width="80" height="40" rx="90" stroke="red" stroke-width="2"/>
        <g transform="rotate(30 100 100) skewX(15)">
            <ellipse cx="100" cy="150" rx="40" ry="20" stroke="red" stroke-width="4"/>
        </g>
    "#);
    let (mut paths, mut shapes) = (Vec::new(), Vec::new());
    leaves(tree(&svg, false).root(), &mut paths);
    leaves(tree(&svg, true).root(), &mut shapes);
    assert_eq!(paths.len(), shapes.len());

    let near = |a: usvgr::Rect, b: usvgr::Rect| {
        [
            (a.left(), b.left()),
            (a.top(), b.top()),
            (a.right(), b.right()),
            (a.bottom(), b.bottom()),
        ]
        .iter()
        .all(|(a, b)| (a - b).abs() < 0.05)
    };
    for (path, shape) in paths.iter().zip(&shapes) {
        let usvgr::Node::FastShape(fast_shape) = shape else {
            panic!("not a fast shape: {shape:?}");
        };
        for (what, a, b) in [
            ("bbox", path.bounding_box(), shape.bounding_box()),
            (
                "stroke bbox",
                path.stroke_bounding_box(),
                shape.stroke_bounding_box(),
            ),
            (
                "abs bbox",
                path.abs_bounding_box(),
                shape.abs_bounding_box(),
            ),
            (
                "abs stroke bbox",
                path.abs_stroke_bounding_box(),
                shape.abs_stroke_bounding_box(),
            ),
        ] {
            assert!(
                near(a, b),
                "{what} of {:?}: path {a:?}, fast shape {b:?}",
                fast_shape.kind()
            );
        }

        // The outline built on demand is the one the regular conversion would build.
        let usvgr::Node::Path(path) = path else {
            unreachable!()
        };
        let outline = fast_shape.to_path().expect("outline");
        assert!(near(
            path.data().compute_tight_bounds().unwrap(),
            outline.data().compute_tight_bounds().unwrap()
        ));
    }
}
