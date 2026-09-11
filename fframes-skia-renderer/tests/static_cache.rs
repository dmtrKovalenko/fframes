//! The render cache is keyed by the compile-time `static_hash` of a node, but
//! a node's final appearance can still change between frames: it may inherit
//! `fill` from a parent produced by a different `svgr!` invocation, or a
//! definition it references may be dynamic.  These tests render two "frames"
//! through one cache and require the second frame to reflect the new state.

use fframes::{Svgr, usvgr};
use fframes_skia_renderer::render::{RenderCache, render_tree};
use skia_safe::{AlphaType, ColorType, ImageInfo, Surface};

const SIZE: i32 = 100;
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

struct Frame {
    surface: Surface,
    info: ImageInfo,
    cache: RenderCache,
}

impl Frame {
    fn new() -> Self {
        let info = ImageInfo::new(
            (SIZE, SIZE),
            ColorType::RGBA8888,
            AlphaType::Premul,
            skia_safe::ColorSpace::new_srgb(),
        );
        let surface = skia_safe::surfaces::raster(&info, None, None).expect("raster surface");
        Self {
            surface,
            info,
            cache: RenderCache::new(),
        }
    }

    fn render(&mut self, svgr: Svgr) -> usvgr::Tree {
        let tree = svgr
            .into_svg_tree(
                &usvgr::Options::default(),
                &mut usvgr::Cache::default(),
                &usvgr::fontdb::Database::new(),
            )
            .expect("valid svgr");

        self.surface.canvas().clear(skia_safe::Color::WHITE);
        render_tree(&tree, self.surface.canvas(), &mut self.cache);
        tree
    }

    fn pixel(&mut self, x: i32, y: i32) -> [u8; 4] {
        let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
        assert!(
            self.surface
                .read_pixels(&self.info, &mut pixels, (SIZE * 4) as usize, (0, 0))
        );
        let i = ((y * SIZE + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    }
}

fn first_child_static_hash(tree: &usvgr::Tree) -> Option<u64> {
    match tree.root().children().first().expect("root has a child") {
        usvgr::Node::Group(g) => g.static_hash(),
        usvgr::Node::Path(p) => p.static_hash(),
        _ => None,
    }
}

fn static_rect() -> Svgr<'static> {
    fframes::svgr!(<rect x="10" y="10" width="80" height="80" />)
}

fn static_group() -> Svgr<'static> {
    fframes::svgr!(
        <g>
            <rect x="10" y="10" width="80" height="80" />
            <circle cx="50" cy="50" r="10" />
        </g>
    )
}

#[test]
fn cached_paint_follows_fill_inherited_across_invocations() {
    let mut frame = Frame::new();

    for (color, expected) in [("#ff0000", RED), ("#0000ff", BLUE)] {
        let tree = frame.render(fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <g fill={color}>{static_rect()}</g>
            </svg>
        ));

        // The inner rect is lexically static, so the cache is exercised.
        let usvgr::Node::Group(g) = &tree.root().children()[0] else {
            panic!("expected the wrapping group");
        };
        let usvgr::Node::Path(rect) = &g.children()[0] else {
            panic!("expected the rect path");
        };
        assert!(rect.static_hash().is_some());

        assert_eq!(frame.pixel(50, 50), expected, "fill {color}");
    }
}

#[test]
fn cached_picture_follows_state_inherited_across_invocations() {
    let mut frame = Frame::new();

    for (color, expected) in [("#ff0000", RED), ("#0000ff", BLUE)] {
        let tree = frame.render(fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <g fill={color}>{static_group()}</g>
            </svg>
        ));

        let usvgr::Node::Group(g) = &tree.root().children()[0] else {
            panic!("expected the wrapping group");
        };
        let usvgr::Node::Group(inner) = &g.children()[0] else {
            panic!("expected the static inner group");
        };
        assert!(
            inner.static_hash().is_some(),
            "inner group is cached as a picture"
        );

        assert_eq!(frame.pixel(50, 50), expected, "fill {color}");
    }
}

#[test]
fn dynamic_gradient_referenced_by_a_static_rect_is_not_cached() {
    let mut frame = Frame::new();

    for (color, expected) in [("#ff0000", RED), ("#0000ff", BLUE)] {
        let tree = frame.render(fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <defs>
                    <linearGradient id="g">
                        <stop offset="0" stop-color={color} />
                        <stop offset="1" stop-color={color} />
                    </linearGradient>
                </defs>
                <rect x="10" y="10" width="80" height="80" fill="url(#g)" />
            </svg>
        ));

        assert!(first_child_static_hash(&tree).is_none());
        assert_eq!(frame.pixel(50, 50), expected, "fill {color}");
    }
}

#[test]
fn unchanged_static_content_is_served_from_the_cache() {
    let mut frame = Frame::new();
    let mut pixels = Vec::new();

    for _ in 0..3 {
        let tree = frame.render(fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <g fill="#ff0000">{static_group()}</g>
            </svg>
        ));
        let usvgr::Node::Group(g) = &tree.root().children()[0] else {
            panic!("expected the wrapping group");
        };
        let usvgr::Node::Group(inner) = &g.children()[0] else {
            panic!("expected the static inner group");
        };
        assert!(
            inner.static_hash().is_some(),
            "inner group is cached as a picture"
        );
        pixels.push(frame.pixel(50, 50));
    }

    assert!(pixels.iter().all(|px| *px == RED));
}
