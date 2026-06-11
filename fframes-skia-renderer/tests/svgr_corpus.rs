//! Corpus-wide compatibility check: renders every SVG from the svgr
//! (resvg fork) test suite through both the direct usvgr->Skia renderer and
//! the old `tree.to_string()` -> Skia SVG DOM path.
//!
//! A file passes when the two renders match (the compat layer is identical).
//! When they diverge, the direct render is compared against the corpus's
//! expected PNG (svgr's own reference output): matching ground truth means
//! the old DOM path simply lacked the feature and the direct renderer is
//! strictly more correct.  Only files matching *neither* fail the test.
//!
//! Test files are discovered dynamically at runtime.  Point `SVGR_TESTS_DIR`
//! at the `crates/svgr/tests` directory of a checkout of
//! <https://github.com/dmtrKovalenko/svgr>, or clone it to `/tmp/svgr`.
//! When the corpus is not present the test is skipped.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fframes::usvgr;
use fframes_skia_renderer::render::{RenderCache, render_tree};
use skia_safe::{AlphaType, ColorType, ImageInfo, Surface};

/// Per-pixel channel difference above which a pixel counts as "bad".
const PIXEL_TOLERANCE: u8 = 40;
/// Fraction of bad pixels above which two same-engine renders diverge.
/// Antialiasing produces isolated bad pixels in any dual-renderer
/// comparison; structural differences light up whole regions.
const DOM_BAD_FRACTION: f64 = 0.02;
/// Looser threshold for the ground-truth comparison: svgr renders with
/// tiny-skia, whose antialiasing/blur kernels differ from Skia's.
const GROUND_TRUTH_TOLERANCE: u8 = 48;
const GROUND_TRUTH_BAD_FRACTION: f64 = 0.10;

/// Files verified by hand where neither automated comparison can work.
/// Each entry documents why.
const VERIFIED_BY_HAND: &[&str] = &[
    // The old DOM path never implemented stitchTiles (a literal TODO in
    // SkSVGFeTurbulence), and stitched Perlin noise is pixel-incomparable
    // between Skia and tiny-skia.  The direct renderer passes the subregion
    // as the Skia noise tile size, which is the intended behavior.
    "tests/filters/feTurbulence/stitchTiles=stitch.svg",
    // Filter regions under non-axis-aligned element transforms: the direct
    // renderer follows the spec/Chrome (the region is defined in user space
    // and the filtered output transforms with the element — a skewed
    // parallelogram).  svgr rasterizes filters axis-aligned in canvas
    // space, and the old DOM path disagreed with both (for feImage it shows
    // the unfiltered source).  All three engines render these differently.
    "tests/filters/feFlood/complex-transform.svg",
    "tests/filters/feImage/link-on-an-element-with-complex-transform.svg",
];

fn corpus_dir() -> Option<PathBuf> {
    let candidates = [
        std::env::var("SVGR_TESTS_DIR").ok().map(PathBuf::from),
        Some(PathBuf::from("/tmp/svgr/crates/svgr/tests")),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|dir| dir.join("tests").is_dir())
}

fn collect_svgs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_svgs(&path, out);
        } else if path.extension().is_some_and(|e| e == "svg") {
            out.push(path);
        }
    }
}

fn build_fontdb(corpus: &Path) -> usvgr::fontdb::Database {
    // Mirrors the font setup of svgr's own integration tests.
    let mut fontdb = usvgr::fontdb::Database::new();
    fontdb.load_fonts_dir(corpus.join("fonts"));
    fontdb.set_serif_family("Noto Serif");
    fontdb.set_sans_serif_family("Noto Sans");
    fontdb.set_cursive_family("Yellowtail");
    fontdb.set_fantasy_family("Sedgwick Ave Display");
    fontdb.set_monospace_family("Noto Mono");
    fontdb
}

fn new_surface(width: i32, height: i32) -> (Surface, ImageInfo) {
    let info = ImageInfo::new(
        (width, height),
        ColorType::RGBA8888,
        AlphaType::Premul,
        skia_safe::ColorSpace::new_srgb(),
    );
    let surface = skia_safe::surfaces::raster(&info, None, None).expect("raster surface");
    (surface, info)
}

fn read_pixels(surface: &mut Surface, info: &ImageInfo, width: i32, height: i32) -> Vec<u8> {
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    assert!(surface.read_pixels(info, &mut pixels, (width * 4) as usize, (0, 0)));
    pixels
}

/// Render with the direct renderer, white background, at the tree's size
/// scaled by `scale`.
fn render_direct(tree: &usvgr::Tree, width: i32, height: i32, scale: f32) -> Vec<u8> {
    let (mut surface, info) = new_surface(width, height);
    surface.canvas().clear(skia_safe::Color::WHITE);
    surface.canvas().scale((scale, scale));
    let mut cache = RenderCache::new();
    render_tree(tree, surface.canvas(), &mut cache);
    read_pixels(&mut surface, &info, width, height)
}

/// Render with the direct renderer on a *transparent* backdrop (like svgr's
/// reference renders — blend modes interact with the backdrop), then
/// composite over white for comparison.
fn render_direct_over_white(tree: &usvgr::Tree, width: i32, height: i32, scale: f32) -> Vec<u8> {
    let (mut surface, info) = new_surface(width, height);
    surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    surface.canvas().scale((scale, scale));
    let mut cache = RenderCache::new();
    render_tree(tree, surface.canvas(), &mut cache);
    let mut pixels = read_pixels(&mut surface, &info, width, height);
    for px in pixels.chunks_mut(4) {
        // Premultiplied source over opaque white.
        let a = px[3] as u32;
        for c in &mut px[..3] {
            *c = (*c as u32 + 255 - a).min(255) as u8;
        }
        px[3] = 255;
    }
    pixels
}

/// Render with the old path: usvgr writer -> Skia SVG DOM.
fn render_reference(tree: &usvgr::Tree, width: i32, height: i32) -> Option<Vec<u8>> {
    let svg = tree.to_string(&usvgr::WriteOptions::default());
    let mut dom = skia_safe::svg::Dom::from_str(&svg, skia_safe::FontMgr::empty()).ok()?;
    dom.set_container_size((width as f32, height as f32));

    let (mut surface, info) = new_surface(width, height);
    surface.canvas().clear(skia_safe::Color::WHITE);
    dom.render(surface.canvas());
    Some(read_pixels(&mut surface, &info, width, height))
}

/// Load svgr's expected PNG (straight alpha) composited over white.
/// The corpus images are palette-optimized, so expand to 8-bit RGB(A).
fn load_ground_truth(path: &Path) -> Option<(Vec<u8>, i32, i32)> {
    let file = std::fs::File::open(path).ok()?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).ok()?;
    buf.truncate(frame.buffer_size());

    let rgba = match frame.color_type {
        png::ColorType::Rgba => {
            for px in buf.chunks_mut(4) {
                let a = px[3] as u32;
                for c in &mut px[..3] {
                    *c = ((*c as u32 * a + 255 * (255 - a)) / 255) as u8;
                }
                px[3] = 255;
            }
            buf
        }
        png::ColorType::Rgb => buf
            .chunks(3)
            .flat_map(|px| [px[0], px[1], px[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .chunks(2)
            .flat_map(|px| {
                let (g, a) = (px[0] as u32, px[1] as u32);
                let c = ((g * a + 255 * (255 - a)) / 255) as u8;
                [c, c, c, 255]
            })
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some((rgba, frame.width as i32, frame.height as i32))
}

/// Fraction of pixels whose max channel difference exceeds `tolerance`.
/// Compares RGB only — both buffers are over an opaque background.
fn bad_pixel_fraction(a: &[u8], b: &[u8], tolerance: u8) -> f64 {
    let total = a.len() / 4;
    let bad = a
        .chunks(4)
        .zip(b.chunks(4))
        .filter(|(pa, pb)| (0..3).any(|c| pa[c].abs_diff(pb[c]) > tolerance))
        .count();
    bad as f64 / total as f64
}

fn dump_ppm(dir: &Path, name: &str, pixels: &[u8], width: i32, height: i32) {
    let _ = std::fs::create_dir_all(dir);
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    for px in pixels.chunks(4) {
        out.extend_from_slice(&px[..3]);
    }
    let _ = std::fs::write(dir.join(name), out);
}

#[test]
fn svgr_corpus_compat() {
    let Some(corpus) = corpus_dir() else {
        eprintln!(
            "svgr corpus not found; set SVGR_TESTS_DIR or clone \
             https://github.com/dmtrKovalenko/svgr to /tmp/svgr — skipping"
        );
        return;
    };

    let fontdb = build_fontdb(&corpus);
    let images: HashMap<String, Arc<usvgr::PreloadedImageData>> = HashMap::new();
    let opt = usvgr::Options {
        // The corpus resolves images relative to the SVG files; the usvgr
        // fork only resolves hrefs through these maps, so image elements
        // simply drop out of the tree — identically for both renderers.
        image_data: Some(&images),
        ..usvgr::Options::default()
    };

    let mut files = Vec::new();
    collect_svgs(&corpus.join("tests"), &mut files);
    files.sort();
    assert!(!files.is_empty(), "no SVG files found in {}", corpus.display());

    let dump_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("svgr_corpus_failures");
    let _ = std::fs::remove_dir_all(&dump_dir);

    let mut matched_old_path = 0usize;
    let mut parse_skipped = 0usize;
    let mut verified_by_hand = 0usize;
    let mut improved: Vec<String> = Vec::new();
    let mut failed: Vec<(String, f64, f64)> = Vec::new();

    for file in &files {
        let rel = file
            .strip_prefix(&corpus)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");

        let Ok(text) = std::fs::read_to_string(file) else {
            parse_skipped += 1;
            continue;
        };
        // Some corpus files are deliberately malformed; they can't reach
        // either renderer, so there is nothing to compare.
        let Ok(tree) = usvgr::Tree::from_str(&text, &opt, &fontdb) else {
            parse_skipped += 1;
            continue;
        };

        // Stage 1: the compat check — direct renderer vs the old DOM path,
        // at the tree's native size.
        let w = tree.size().width().round() as i32;
        let h = tree.size().height().round() as i32;
        let direct = render_direct(&tree, w, h, 1.0);
        let dom_fraction = match render_reference(&tree, w, h) {
            Some(reference) => bad_pixel_fraction(&direct, &reference, PIXEL_TOLERANCE),
            // The DOM cannot even parse the writer output — treat as a full
            // divergence and let ground truth decide.
            None => 1.0,
        };
        if dom_fraction <= DOM_BAD_FRACTION {
            matched_old_path += 1;
            continue;
        }

        if VERIFIED_BY_HAND.contains(&rel.as_str()) {
            verified_by_hand += 1;
            continue;
        }

        // Stage 2: divergence from the old path — check against svgr's own
        // expected PNG at its native resolution.
        let gt_fraction = if let Some((expected, gw, gh)) =
            load_ground_truth(&file.with_extension("png"))
        {
            let scale = gw as f32 / tree.size().width();
            let direct_scaled = render_direct_over_white(&tree, gw, gh, scale);
            bad_pixel_fraction(&direct_scaled, &expected, GROUND_TRUTH_TOLERANCE)
        } else {
            1.0
        };
        if gt_fraction <= GROUND_TRUTH_BAD_FRACTION {
            improved.push(rel);
            continue;
        }

        let safe = rel.replace('/', "_");
        dump_ppm(&dump_dir, &format!("{safe}.direct.ppm"), &direct, w, h);
        if let Some(reference) = render_reference(&tree, w, h) {
            dump_ppm(&dump_dir, &format!("{safe}.reference.ppm"), &reference, w, h);
        }
        failed.push((rel, dom_fraction, gt_fraction));
    }

    failed.sort_by(|a, b| b.1.total_cmp(&a.1));

    eprintln!(
        "svgr corpus: {} files | {} match the old path | {} diverge but match \
         svgr ground truth (features the old path lacked) | {} verified by hand \
         | {} skipped (unparsable) | {} FAILED",
        files.len(),
        matched_old_path,
        improved.len(),
        verified_by_hand,
        parse_skipped,
        failed.len(),
    );
    if !improved.is_empty() {
        let mut by_dir: HashMap<String, usize> = HashMap::new();
        for rel in &improved {
            let dir = rel.rsplit_once('/').map(|(d, _)| d).unwrap_or(rel);
            *by_dir.entry(dir.to_string()).or_default() += 1;
        }
        let mut by_dir: Vec<_> = by_dir.into_iter().collect();
        by_dir.sort();
        for (dir, count) in by_dir {
            eprintln!("  improved: {dir} ({count})");
        }
    }
    for (rel, dom_fraction, gt_fraction) in &failed {
        eprintln!("  FAILED {rel} (vs old path: {dom_fraction:.4}, vs ground truth: {gt_fraction:.4})");
    }

    assert!(
        failed.is_empty(),
        "{} corpus files match neither the old DOM path nor svgr's expected \
         output (images dumped to {})",
        failed.len(),
        dump_dir.display(),
    );
}
