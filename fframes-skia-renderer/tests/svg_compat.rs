//! Compatibility tests for the direct usvgr::Tree -> Skia Canvas renderer.
//!
//! The direct renderer replaces the old `tree.to_string()` + `svg::Dom` path,
//! so for every SVG feature that Skia's SVG DOM supports we render both ways
//! and require near-identical output.  Features the SVG DOM never supported
//! (feConvolveMatrix, feDropShadow, feTile, pattern viewBox, ...) are checked
//! with exact pixel probes instead.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use fframes::usvgr;
use fframes_skia_renderer::render::{RenderCache, render_tree};
use skia_safe::{AlphaType, ColorType, ImageInfo, Surface};

const W: i32 = 200;
const H: i32 = 200;

fn parse(svg: &str) -> usvgr::Tree {
    let opt = usvgr::Options::default();
    let fontdb = usvgr::fontdb::Database::new();
    usvgr::Tree::from_str(svg, &opt, &fontdb).expect("failed to parse test SVG")
}

fn parse_with_images(
    svg: &str,
    images: &HashMap<String, Arc<usvgr::PreloadedImageData>>,
) -> usvgr::Tree {
    let opt = usvgr::Options {
        image_data: Some(images),
        ..usvgr::Options::default()
    };
    let fontdb = usvgr::fontdb::Database::new();
    usvgr::Tree::from_str(svg, &opt, &fontdb).expect("failed to parse test SVG")
}

fn new_surface() -> (Surface, ImageInfo) {
    let info = ImageInfo::new(
        (W, H),
        ColorType::RGBA8888,
        AlphaType::Premul,
        skia_safe::ColorSpace::new_srgb(),
    );
    let surface = skia_safe::surfaces::raster(&info, None, None).expect("raster surface");
    (surface, info)
}

fn read_pixels(surface: &mut Surface, info: &ImageInfo) -> Vec<u8> {
    let mut pixels = vec![0u8; (W * H * 4) as usize];
    let row_bytes = (W * 4) as usize;
    assert!(
        surface.read_pixels(info, &mut pixels, row_bytes, (0, 0)),
        "failed to read pixels"
    );
    pixels
}

/// Render with the new direct renderer.
fn render_direct(tree: &usvgr::Tree) -> Vec<u8> {
    let (mut surface, info) = new_surface();
    surface.canvas().clear(skia_safe::Color::WHITE);
    let mut cache = RenderCache::new();
    render_tree(tree, surface.canvas(), &mut cache);
    read_pixels(&mut surface, &info)
}

/// Render with the old path: usvgr writer -> Skia SVG DOM.
fn render_reference(tree: &usvgr::Tree) -> Vec<u8> {
    let svg = tree.to_string(&usvgr::WriteOptions::default());
    let mut dom = skia_safe::svg::Dom::from_str(&svg, skia_safe::FontMgr::empty())
        .expect("Skia SVG DOM failed to parse writer output");
    dom.set_container_size((W as f32, H as f32));

    let (mut surface, info) = new_surface();
    surface.canvas().clear(skia_safe::Color::WHITE);
    dom.render(surface.canvas());
    read_pixels(&mut surface, &info)
}

fn dump_ppm(name: &str, suffix: &str, pixels: &[u8]) {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("svg_compat_failures");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{name}-{suffix}.ppm"));
    let mut out = format!("P6\n{W} {H}\n255\n").into_bytes();
    for px in pixels.chunks(4) {
        out.extend_from_slice(&px[..3]);
    }
    let _ = std::fs::write(&path, out);
    eprintln!("wrote {}", path.display());
}

/// Assert two renders are visually identical up to antialiasing noise.
fn assert_similar(name: &str, direct: &[u8], reference: &[u8]) {
    assert_eq!(direct.len(), reference.len());

    let mut total_diff: u64 = 0;
    let mut bad_pixels: u64 = 0;
    for (d, r) in direct.chunks(4).zip(reference.chunks(4)) {
        let mut max_chan = 0u8;
        for c in 0..4 {
            let diff = d[c].abs_diff(r[c]);
            total_diff += diff as u64;
            max_chan = max_chan.max(diff);
        }
        if max_chan > 40 {
            bad_pixels += 1;
        }
    }

    let mean = total_diff as f64 / direct.len() as f64;
    let bad_frac = bad_pixels as f64 / (W * H) as f64;

    if mean > 2.0 || bad_frac > 0.012 {
        dump_ppm(name, "direct", direct);
        dump_ppm(name, "reference", reference);
        panic!("{name}: renders diverge (mean channel diff {mean:.3}, {bad_frac:.4} of pixels differ by >40)");
    }
}

fn assert_matches_dom(name: &str, svg: &str) {
    let tree = parse(svg);
    let direct = render_direct(&tree);
    let reference = render_reference(&tree);
    assert_similar(name, &direct, &reference);
}

fn px(pixels: &[u8], x: i32, y: i32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn assert_px_near(pixels: &[u8], x: i32, y: i32, expected: [u8; 4], tol: u8, what: &str) {
    let got = px(pixels, x, y);
    for c in 0..4 {
        assert!(
            got[c].abs_diff(expected[c]) <= tol,
            "{what}: pixel ({x},{y}) = {got:?}, expected {expected:?} (±{tol})"
        );
    }
}

// ---------------------------------------------------------------------------
// Core geometry and paint servers
// ---------------------------------------------------------------------------

#[test]
fn shapes_strokes_and_transforms_match_dom() {
    assert_matches_dom(
        "shapes",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <rect x="10" y="10" width="60" height="40" fill="#e63946"/>
            <rect x="80" y="10" width="60" height="40" rx="12" fill="#457b9d" fill-opacity="0.6"/>
            <circle cx="40" cy="100" r="25" fill="none" stroke="#1d3557" stroke-width="6"/>
            <path d="M 90 80 L 150 80 L 120 130 Z" fill="#2a9d8f" stroke="#264653"
                  stroke-width="4" stroke-linejoin="round"/>
            <line x1="10" y1="160" x2="190" y2="160" stroke="#f4a261" stroke-width="8"
                  stroke-dasharray="12 6" stroke-dashoffset="4" stroke-linecap="round"/>
            <path d="M 20 170 h 40 v 20 h -40 z M 30 175 h 20 v 10 h -20 z"
                  fill="#9b5de5" fill-rule="evenodd"/>
            <g transform="translate(120 150) rotate(30) scale(0.8)">
                <rect x="0" y="0" width="50" height="30" fill="#00b4d8" stroke="#03045e"
                      stroke-width="3" stroke-miterlimit="2"/>
            </g>
        </svg>"##,
    );
}

#[test]
fn gradients_match_dom() {
    assert_matches_dom(
        "gradients",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <linearGradient id="pad" x1="0" y1="0" x2="1" y2="0">
                    <stop offset="0" stop-color="#ff0000"/>
                    <stop offset="1" stop-color="#0000ff" stop-opacity="0.5"/>
                </linearGradient>
                <linearGradient id="reflect" x1="0.25" y1="0" x2="0.5" y2="0" spreadMethod="reflect">
                    <stop offset="0" stop-color="#ffba08"/>
                    <stop offset="1" stop-color="#370617"/>
                </linearGradient>
                <linearGradient id="repeat" x1="0" y1="0" x2="0.25" y2="0.25" spreadMethod="repeat"
                                gradientTransform="rotate(15)">
                    <stop offset="0" stop-color="#80ffdb"/>
                    <stop offset="1" stop-color="#5390d9"/>
                </linearGradient>
                <radialGradient id="focal" cx="0.5" cy="0.5" r="0.5" fx="0.3" fy="0.3">
                    <stop offset="0" stop-color="#ffffff"/>
                    <stop offset="0.6" stop-color="#fb8500"/>
                    <stop offset="1" stop-color="#023047"/>
                </radialGradient>
                <linearGradient id="user" gradientUnits="userSpaceOnUse"
                                x1="10" y1="150" x2="190" y2="190">
                    <stop offset="0" stop-color="#d00000"/>
                    <stop offset="1" stop-color="#3f88c5"/>
                </linearGradient>
            </defs>
            <rect x="0" y="0" width="100" height="45" fill="url(#pad)"/>
            <rect x="100" y="0" width="100" height="45" fill="url(#reflect)"/>
            <rect x="0" y="50" width="100" height="45" fill="url(#repeat)"/>
            <circle cx="150" cy="72" r="22" fill="url(#focal)"/>
            <rect x="10" y="150" width="180" height="40" fill="url(#user)"
                  stroke="url(#pad)" stroke-width="4"/>
        </svg>"##,
    );
}

#[test]
fn pattern_matches_dom() {
    assert_matches_dom(
        "pattern",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <pattern id="dots" x="0" y="0" width="0.1" height="0.1">
                    <rect x="0" y="0" width="20" height="20" fill="#edf2f4"/>
                    <circle cx="10" cy="10" r="6" fill="#d90429"/>
                </pattern>
                <pattern id="stripes" patternUnits="userSpaceOnUse" x="0" y="0" width="16" height="16"
                         patternTransform="rotate(45)">
                    <rect x="0" y="0" width="16" height="16" fill="#ffd166"/>
                    <rect x="0" y="0" width="8" height="16" fill="#118ab2"/>
                </pattern>
            </defs>
            <rect x="0" y="0" width="200" height="95" fill="url(#dots)"/>
            <circle cx="100" cy="150" r="45" fill="url(#stripes)" stroke="#073b4c" stroke-width="3"/>
        </svg>"##,
    );
}

#[test]
fn pattern_viewbox_probe() {
    // Skia's SVG DOM never supported viewBox on <pattern>; verify tiling
    // and content scaling directly.  Pattern: 10x10 tile (viewBox 0 0 2 2)
    // where the left half is red and the right half is blue.
    let tree = parse(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <pattern id="p" patternUnits="userSpaceOnUse" width="10" height="10"
                         viewBox="0 0 2 2">
                    <rect x="0" y="0" width="1" height="2" fill="#ff0000"/>
                    <rect x="1" y="0" width="1" height="2" fill="#0000ff"/>
                </pattern>
            </defs>
            <rect x="0" y="0" width="200" height="200" fill="url(#p)"/>
        </svg>"##,
    );
    let direct = render_direct(&tree);

    // Tile is 10px: x in [0,5) red, [5,10) blue, repeating.
    assert_px_near(&direct, 2, 50, [255, 0, 0, 255], 2, "pattern red half");
    assert_px_near(&direct, 7, 50, [0, 0, 255, 255], 2, "pattern blue half");
    assert_px_near(&direct, 102, 150, [255, 0, 0, 255], 2, "repeated red half");
    assert_px_near(&direct, 107, 150, [0, 0, 255, 255], 2, "repeated blue half");
}

// ---------------------------------------------------------------------------
// Compositing: opacity, blend modes, clips, masks
// ---------------------------------------------------------------------------

#[test]
fn opacity_groups_match_dom() {
    assert_matches_dom(
        "opacity",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <rect x="0" y="0" width="200" height="200" fill="#cdb4db"/>
            <g opacity="0.5">
                <circle cx="70" cy="70" r="50" fill="#ff006e"/>
                <circle cx="110" cy="70" r="50" fill="#3a86ff"/>
            </g>
            <circle cx="100" cy="150" r="40" fill="#ffbe0b" fill-opacity="0.35"/>
        </svg>"##,
    );
}

#[test]
fn blend_modes_probe() {
    // Skia's SVG DOM ignored mix-blend-mode entirely, so the old path can't
    // serve as a reference; verify the blend math against the spec directly.
    let tree = parse(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <rect x="0" y="0" width="200" height="200" fill="#cdb4db"/>
            <rect x="10" y="10" width="50" height="50" fill="#ffbe0b"
                  style="mix-blend-mode:multiply"/>
            <rect x="70" y="10" width="50" height="50" fill="#fb5607"
                  style="mix-blend-mode:screen"/>
            <rect x="130" y="10" width="50" height="50" fill="#8338ec"
                  style="mix-blend-mode:difference"/>
        </svg>"##,
    );
    let direct = render_direct(&tree);
    // Backdrop #cdb4db = (205, 180, 219).
    // multiply(#ffbe0b): (205*255, 180*190, 219*11) / 255
    assert_px_near(&direct, 35, 35, [205, 134, 9, 255], 4, "multiply");
    // screen(#fb5607): 255 - (255-b)(255-s)/255
    assert_px_near(&direct, 95, 35, [254, 205, 220, 255], 4, "screen");
    // difference(#8338ec): |b - s|
    assert_px_near(&direct, 155, 35, [74, 124, 17, 255], 4, "difference");
}

#[test]
fn clip_paths_match_dom() {
    assert_matches_dom(
        "clip",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <clipPath id="circle-clip">
                    <circle cx="60" cy="60" r="40"/>
                </clipPath>
                <clipPath id="donut" clip-rule="evenodd">
                    <path d="M 100 10 A 50 50 0 1 0 100 110 A 50 50 0 1 0 100 10 Z
                             M 100 35 A 25 25 0 1 1 100 85 A 25 25 0 1 1 100 35 Z"/>
                </clipPath>
                <clipPath id="nested" clip-path="url(#circle-clip)">
                    <rect x="20" y="20" width="120" height="60"/>
                </clipPath>
                <clipPath id="grouped">
                    <g transform="translate(10 140)">
                        <rect x="0" y="0" width="80" height="50"/>
                    </g>
                </clipPath>
            </defs>
            <rect x="0" y="0" width="200" height="100" fill="#06d6a0" clip-path="url(#donut)"/>
            <rect x="0" y="0" width="200" height="120" fill="#ef476f" clip-path="url(#nested)"/>
            <rect x="0" y="120" width="200" height="80" fill="#118ab2" clip-path="url(#grouped)"/>
        </svg>"##,
    );
}

#[test]
fn masks_match_dom() {
    assert_matches_dom(
        "mask",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <linearGradient id="fade" x1="0" y1="0" x2="1" y2="0">
                    <stop offset="0" stop-color="#ffffff"/>
                    <stop offset="1" stop-color="#000000"/>
                </linearGradient>
                <mask id="lum">
                    <rect x="0" y="0" width="200" height="100" fill="url(#fade)"/>
                </mask>
                <mask id="shape">
                    <rect x="0" y="100" width="200" height="100" fill="#000000"/>
                    <circle cx="100" cy="150" r="40" fill="#ffffff"/>
                </mask>
            </defs>
            <rect x="0" y="0" width="200" height="100" fill="#e76f51" mask="url(#lum)"/>
            <rect x="0" y="100" width="200" height="100" fill="#2a9d8f" mask="url(#shape)"/>
        </svg>"##,
    );
}

// ---------------------------------------------------------------------------
// Filters supported by both renderers — must match the old DOM output
// ---------------------------------------------------------------------------

#[test]
fn filter_blur_offset_flood_composite_merge_match_dom() {
    assert_matches_dom(
        "filter-basic",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <filter id="blur"><feGaussianBlur stdDeviation="4"/></filter>
                <filter id="shadowish" x="-30%" y="-30%" width="160%" height="160%">
                    <feOffset in="SourceAlpha" dx="6" dy="6" result="off"/>
                    <feFlood flood-color="#000000" flood-opacity="0.4" result="col"/>
                    <feComposite in="col" in2="off" operator="in" result="shadow"/>
                    <feMerge>
                        <feMergeNode in="shadow"/>
                        <feMergeNode in="SourceGraphic"/>
                    </feMerge>
                </filter>
                <filter id="arith">
                    <feComposite in="SourceGraphic" in2="SourceGraphic" operator="arithmetic"
                                 k1="0" k2="0.5" k3="0.3" k4="0.1"/>
                </filter>
            </defs>
            <rect x="20" y="20" width="60" height="40" fill="#d62828" filter="url(#blur)"/>
            <rect x="110" y="20" width="60" height="40" fill="#003049" filter="url(#shadowish)"/>
            <circle cx="100" cy="140" r="40" fill="#f77f00" filter="url(#arith)"/>
        </svg>"##,
    );
}

#[test]
fn filter_color_matrix_and_component_transfer_match_dom() {
    assert_matches_dom(
        "filter-color",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <filter id="sat"><feColorMatrix type="saturate" values="0.2"/></filter>
                <filter id="hue"><feColorMatrix type="hueRotate" values="120"/></filter>
                <filter id="mat">
                    <feColorMatrix type="matrix"
                        values="0.4 0.4 0.2 0 0  0.2 0.6 0.2 0 0  0.1 0.2 0.7 0 0  0 0 0 1 0"/>
                </filter>
                <filter id="ct">
                    <feComponentTransfer>
                        <feFuncR type="linear" slope="0.6" intercept="0.2"/>
                        <feFuncG type="gamma" amplitude="1" exponent="2" offset="0"/>
                        <feFuncB type="table" tableValues="1 0"/>
                    </feComponentTransfer>
                </filter>
                <filter id="disc">
                    <feComponentTransfer>
                        <feFuncR type="discrete" tableValues="0 0.5 1"/>
                        <feFuncG type="discrete" tableValues="0 1"/>
                    </feComponentTransfer>
                </filter>
            </defs>
            <rect x="0" y="0" width="90" height="40" fill="#e07a5f" filter="url(#sat)"/>
            <rect x="100" y="0" width="90" height="40" fill="#81b29a" filter="url(#hue)"/>
            <rect x="0" y="50" width="90" height="40" fill="#f2cc8f" filter="url(#mat)"/>
            <rect x="100" y="50" width="90" height="40" fill="#3d405b" filter="url(#ct)"/>
            <rect x="0" y="100" width="190" height="40" fill="#5f797b" filter="url(#disc)"/>
        </svg>"##,
    );
}

#[test]
fn filter_morphology_matches_dom() {
    assert_matches_dom(
        "filter-morph",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <filter id="erode"><feMorphology operator="erode" radius="2"/></filter>
                <filter id="dilate"><feMorphology operator="dilate" radius="3"/></filter>
            </defs>
            <rect x="20" y="30" width="70" height="50" fill="#9d0208" filter="url(#erode)"/>
            <rect x="110" y="30" width="70" height="50" fill="#0a9396" filter="url(#dilate)"/>
        </svg>"##,
    );
}

#[test]
fn filter_turbulence_and_displacement_match_dom() {
    assert_matches_dom(
        "filter-turb",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <filter id="noise" x="0%" y="0%" width="100%" height="100%">
                    <feTurbulence type="turbulence" baseFrequency="0.08" numOctaves="2" seed="3"/>
                </filter>
                <filter id="fractal" x="0%" y="0%" width="100%" height="100%">
                    <feTurbulence type="fractalNoise" baseFrequency="0.12 0.05" numOctaves="3" seed="7"/>
                </filter>
                <filter id="warp">
                    <feTurbulence type="fractalNoise" baseFrequency="0.05" numOctaves="2"
                                  seed="2" result="n"/>
                    <feDisplacementMap in="SourceGraphic" in2="n" scale="15"
                                       xChannelSelector="R" yChannelSelector="G"/>
                </filter>
            </defs>
            <rect x="0" y="0" width="95" height="90" filter="url(#noise)"/>
            <rect x="100" y="0" width="95" height="90" filter="url(#fractal)"/>
            <rect x="40" y="110" width="120" height="60" fill="#5e60ce" filter="url(#warp)"/>
        </svg>"##,
    );
}

#[test]
fn filter_lighting_matches_dom() {
    assert_matches_dom(
        "filter-light",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <filter id="diffuse-distant">
                    <feDiffuseLighting surfaceScale="2" diffuseConstant="1" lighting-color="#ffffff">
                        <feDistantLight azimuth="45" elevation="60"/>
                    </feDiffuseLighting>
                </filter>
                <filter id="diffuse-point">
                    <feDiffuseLighting surfaceScale="3" diffuseConstant="1.2" lighting-color="#ffe8d6">
                        <fePointLight x="50" y="40" z="30"/>
                    </feDiffuseLighting>
                </filter>
                <filter id="spec-spot">
                    <feSpecularLighting surfaceScale="2" specularConstant="0.8"
                                        specularExponent="12" lighting-color="#ffffff">
                        <feSpotLight x="100" y="100" z="60" pointsAtX="100" pointsAtY="150"
                                     pointsAtZ="0" specularExponent="4" limitingConeAngle="30"/>
                    </feSpecularLighting>
                </filter>
            </defs>
            <circle cx="50" cy="50" r="35" fill="#6b705c" filter="url(#diffuse-distant)"/>
            <circle cx="150" cy="50" r="35" fill="#cb997e" filter="url(#diffuse-point)"/>
            <circle cx="100" cy="145" r="40" fill="#3a5a40" filter="url(#spec-spot)"/>
        </svg>"##,
    );
}

#[test]
fn filter_color_interpolation_srgb_matches_dom() {
    // Explicit sRGB primitives mixed with default (linearRGB) primitives —
    // exercises the colorspace conversion insertion logic against the DOM.
    assert_matches_dom(
        "filter-cs",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <filter id="srgb-blur" color-interpolation-filters="sRGB">
                    <feGaussianBlur stdDeviation="5"/>
                </filter>
                <filter id="linear-blur" color-interpolation-filters="linearRGB">
                    <feGaussianBlur stdDeviation="5"/>
                </filter>
                <filter id="mixed">
                    <feGaussianBlur stdDeviation="3" result="b"
                                    color-interpolation-filters="linearRGB"/>
                    <feColorMatrix in="b" type="saturate" values="0.3"
                                   color-interpolation-filters="sRGB"/>
                </filter>
            </defs>
            <rect x="20" y="20" width="70" height="50" fill="#ff5400" filter="url(#srgb-blur)"/>
            <rect x="110" y="20" width="70" height="50" fill="#38b000" filter="url(#linear-blur)"/>
            <rect x="50" y="110" width="100" height="60" fill="#9e0059" filter="url(#mixed)"/>
        </svg>"##,
    );
}

// ---------------------------------------------------------------------------
// Filters the old DOM path never supported — probe the direct renderer
// ---------------------------------------------------------------------------

#[test]
fn fe_convolve_matrix_identity_is_noop() {
    let plain = parse(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <rect x="40" y="40" width="120" height="120" fill="#0077b6"/>
        </svg>"##,
    );
    let filtered = parse(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <filter id="id" color-interpolation-filters="sRGB">
                    <feConvolveMatrix order="3" kernelMatrix="0 0 0 0 1 0 0 0 0"/>
                </filter>
            </defs>
            <rect x="40" y="40" width="120" height="120" fill="#0077b6" filter="url(#id)"/>
        </svg>"##,
    );
    let a = render_direct(&plain);
    let b = render_direct(&filtered);
    assert_similar("convolve-identity", &b, &a);
}

#[test]
fn fe_convolve_matrix_shift_kernel_moves_content() {
    // A single off-center kernel weight with a non-centered target shifts the
    // image.  Per the SVG spec formula, kernelMatrix[0] = 1 (top-left) with
    // targetX/Y = 0 yields result(x,y) = source(x+2, y+2): a 2px up-left
    // shift.  This exercises the kernel reversal AND the target offset
    // mapping (a centered target can't tell a mirrored offset apart).
    let tree = parse(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <filter id="shift" color-interpolation-filters="sRGB" x="-20%" y="-20%"
                        width="140%" height="140%">
                    <feConvolveMatrix order="3" targetX="0" targetY="0" edgeMode="none"
                                      kernelMatrix="1 0 0 0 0 0 0 0 0"/>
                </filter>
            </defs>
            <rect x="80" y="80" width="40" height="40" fill="#ff0000" filter="url(#shift)"/>
        </svg>"##,
    );
    let direct = render_direct(&tree);
    // Original rect spans 80..120; shifted up-left by 2 it spans 78..118.
    assert_px_near(&direct, 79, 79, [255, 0, 0, 255], 4, "shifted up-left");
    assert_px_near(&direct, 119, 119, [255, 255, 255, 255], 4, "vacated bottom-right");
}

#[test]
fn fe_drop_shadow_renders() {
    let tree = parse(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <filter id="ds" x="-50%" y="-50%" width="200%" height="200%"
                        color-interpolation-filters="sRGB">
                    <feDropShadow dx="15" dy="15" stdDeviation="0.01" flood-color="#000000"
                                  flood-opacity="1"/>
                </filter>
            </defs>
            <rect x="40" y="40" width="80" height="80" fill="#e63946" filter="url(#ds)"/>
        </svg>"##,
    );
    let direct = render_direct(&tree);
    assert_px_near(&direct, 80, 80, [230, 57, 70, 255], 4, "original content");
    // Shadow visible at (offset area beyond the rect): rect ends at 120,
    // shadow extends to 135.
    assert_px_near(&direct, 128, 128, [0, 0, 0, 255], 8, "shadow area");
    assert_px_near(&direct, 150, 150, [255, 255, 255, 255], 4, "outside shadow");
}

#[test]
fn fe_tile_repeats_input_subregion() {
    let tree = parse(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <filter id="tile" x="0" y="0" width="200" height="200"
                        filterUnits="userSpaceOnUse" primitiveUnits="userSpaceOnUse"
                        color-interpolation-filters="sRGB">
                    <feFlood x="0" y="0" width="10" height="10" flood-color="#00ff00" result="t"/>
                    <feTile in="t" x="0" y="0" width="200" height="200"/>
                </filter>
            </defs>
            <rect x="0" y="0" width="200" height="200" fill="#000000" filter="url(#tile)"/>
        </svg>"##,
    );
    let direct = render_direct(&tree);
    assert_px_near(&direct, 5, 5, [0, 255, 0, 255], 2, "tile origin");
    assert_px_near(&direct, 105, 55, [0, 255, 0, 255], 2, "tiled across region");
    assert_px_near(&direct, 195, 195, [0, 255, 0, 255], 2, "tiled to corner");
}

#[test]
fn fe_image_use_reference_renders() {
    let tree = parse(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200"
             xmlns:xlink="http://www.w3.org/1999/xlink">
            <defs>
                <rect id="stamp" width="40" height="40" fill="#7209b7"/>
                <filter id="fi" x="0" y="0" width="200" height="200"
                        filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
                    <feImage xlink:href="#stamp" x="30" y="30" width="40" height="40"/>
                </filter>
            </defs>
            <rect x="0" y="0" width="200" height="200" fill="#ffffff" filter="url(#fi)"/>
        </svg>"##,
    );
    let direct = render_direct(&tree);
    assert_px_near(&direct, 50, 50, [114, 9, 183, 255], 4, "feImage content");
    assert_px_near(&direct, 100, 100, [255, 255, 255, 255], 4, "outside feImage");
}

// ---------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------

fn checker_image(id: &str) -> Arc<usvgr::PreloadedImageData> {
    // 2x2 checkerboard: red, green / blue, white (opaque).
    #[rustfmt::skip]
    let data: Vec<u8> = vec![
        255, 0, 0, 255,    0, 255, 0, 255,
        0, 0, 255, 255,    255, 255, 255, 255,
    ];
    Arc::new(usvgr::PreloadedImageData {
        data: Cow::Owned(data),
        width: 2,
        height: 2,
        id: id.to_string(),
    })
}

#[test]
fn raster_image_scales_and_clips() {
    let mut images = HashMap::new();
    images.insert("checker".to_string(), checker_image("checker"));

    let tree = parse_with_images(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200"
             xmlns:xlink="http://www.w3.org/1999/xlink">
            <image xlink:href="checker" x="0" y="0" width="100" height="100"
                   image-rendering="optimizeSpeed" preserveAspectRatio="none"/>
            <image xlink:href="checker" x="100" y="100" width="100" height="50"
                   image-rendering="optimizeSpeed" preserveAspectRatio="xMidYMid slice"/>
        </svg>"##,
        &images,
    );
    let direct = render_direct(&tree);

    // First image: 2x2 stretched to 100x100 with nearest sampling -> quadrants.
    assert_px_near(&direct, 20, 20, [255, 0, 0, 255], 2, "checker top-left");
    assert_px_near(&direct, 80, 20, [0, 255, 0, 255], 2, "checker top-right");
    assert_px_near(&direct, 20, 80, [0, 0, 255, 255], 2, "checker bottom-left");

    // Second image: slice scales to cover 100x100, vertically centered, so the
    // visible 50px band is the middle of the image; content must not leak
    // outside the 100x50 element rect.
    assert_px_near(&direct, 120, 110, [255, 0, 0, 255], 2, "slice top-left quadrant");
    assert_px_near(&direct, 120, 160, [255, 255, 255, 255], 4, "no overflow below rect");
}

#[test]
fn nested_svg_image_scales_to_viewport() {
    // An <image> referencing an SVG must scale the child's intrinsic size
    // (40x40) to the element rect (100x100).
    let mut sub_trees = HashMap::new();
    let child = parse(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40">
            <rect x="0" y="0" width="40" height="40" fill="#ff0000"/>
            <rect x="20" y="0" width="20" height="40" fill="#0000ff"/>
        </svg>"##,
    );
    sub_trees.insert("child".to_string(), Arc::new(child));

    // usvgr's get_href_data short-circuits on `image_data: None` before it
    // ever consults sub_svg_data, so an (empty) image map must be provided.
    let images = HashMap::new();
    let opt = usvgr::Options {
        image_data: Some(&images),
        sub_svg_data: Some(&sub_trees),
        ..usvgr::Options::default()
    };
    let fontdb = usvgr::fontdb::Database::new();
    let tree = usvgr::Tree::from_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200"
             xmlns:xlink="http://www.w3.org/1999/xlink">
            <image xlink:href="child" x="50" y="50" width="100" height="100"/>
        </svg>"##,
        &opt,
        &fontdb,
    )
    .expect("parse svg with sub-tree");

    let direct = render_direct(&tree);
    assert_px_near(&direct, 75, 100, [255, 0, 0, 255], 2, "scaled left half");
    assert_px_near(&direct, 125, 100, [0, 0, 255, 255], 2, "scaled right half");
    assert_px_near(&direct, 25, 100, [255, 255, 255, 255], 2, "outside image");
}

// ---------------------------------------------------------------------------
// Spec-order and caching invariants
// ---------------------------------------------------------------------------

#[test]
fn mask_applies_after_filter() {
    // The filter dilates a small green square into a larger one; the mask then
    // reveals only the left half.  If the mask were (incorrectly) applied
    // before the filter, the dilated content would leak outside the mask.
    let tree = parse(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <filter id="grow" x="-50%" y="-50%" width="200%" height="200%"
                        color-interpolation-filters="sRGB">
                    <feMorphology operator="dilate" radius="20"/>
                </filter>
                <mask id="left-half">
                    <rect x="0" y="0" width="100" height="200" fill="#ffffff"/>
                </mask>
            </defs>
            <g filter="url(#grow)" mask="url(#left-half)">
                <rect x="80" y="80" width="40" height="40" fill="#00ff00"/>
            </g>
        </svg>"##,
    );
    let direct = render_direct(&tree);
    // Dilated rect spans 60..140; mask cuts at x=100.
    assert_px_near(&direct, 90, 100, [0, 255, 0, 255], 2, "filtered content inside mask");
    assert_px_near(&direct, 110, 100, [255, 255, 255, 255], 2, "filtered content masked out");
}

#[test]
fn render_cache_is_stable_across_frames() {
    // Render the same tree three times through one cache: pictures, paths and
    // paints are reused on later frames and must produce identical output.
    let tree = parse(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <pattern id="p" patternUnits="userSpaceOnUse" width="20" height="20">
                    <rect width="20" height="20" fill="#fff3b0"/>
                    <circle cx="10" cy="10" r="5" fill="#335c67"/>
                </pattern>
                <filter id="b"><feGaussianBlur stdDeviation="2"/></filter>
                <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
                    <stop offset="0" stop-color="#9e2a2b"/>
                    <stop offset="1" stop-color="#e09f3e"/>
                </linearGradient>
            </defs>
            <rect x="0" y="0" width="200" height="100" fill="url(#p)"/>
            <rect x="20" y="110" width="70" height="60" fill="url(#g)" filter="url(#b)"/>
            <circle cx="150" cy="140" r="30" fill="#540b0e" stroke="#9e2a2b" stroke-width="5"
                    stroke-dasharray="8 4"/>
        </svg>"##,
    );

    let (mut surface, info) = new_surface();
    let mut cache = RenderCache::new();
    let mut frames = Vec::new();
    for _ in 0..3 {
        surface.canvas().clear(skia_safe::Color::WHITE);
        render_tree(&tree, surface.canvas(), &mut cache);
        frames.push(read_pixels(&mut surface, &info));
    }

    assert_eq!(frames[0], frames[1], "frame 2 (cached) differs from frame 1");
    assert_eq!(frames[0], frames[2], "frame 3 (cached) differs from frame 1");
}

#[test]
fn linear_rgb_roundtrip_preserves_flat_colors() {
    // A blur in linearRGB over a large flat region must not shift interior
    // colors: sRGB -> linear -> blur -> sRGB is an identity for flat areas.
    let tree = parse(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
            <defs>
                <filter id="lb" color-interpolation-filters="linearRGB">
                    <feGaussianBlur stdDeviation="3"/>
                </filter>
            </defs>
            <rect x="20" y="20" width="160" height="160" fill="#3a86ff" filter="url(#lb)"/>
        </svg>"##,
    );
    let direct = render_direct(&tree);
    assert_px_near(&direct, 100, 100, [58, 134, 255, 255], 6, "interior color preserved");
}
