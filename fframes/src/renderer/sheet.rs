//! Compositing several frames into one image: labelled contact sheets and onion skins, which
//! show a movement in a single picture.
use super::{FFramesRendererError, FFramesRendererResult, RgbaFrame};
use crate::usvgr;
use svgr::tiny_skia;

/// A frame of a contact sheet with the label drawn under it.
pub struct SheetCell {
    pub frame: RgbaFrame,
    pub label: String,
}

fn to_pixmap(frame: &RgbaFrame) -> FFramesRendererResult<tiny_skia::Pixmap> {
    let mut data = frame.pixels.clone();
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3] as u32;
        if a != 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * a + 127) / 255) as u8;
            }
        }
    }

    tiny_skia::Pixmap::from_vec(
        data,
        tiny_skia::IntSize::from_wh(frame.width, frame.height)
            .ok_or_else(|| FFramesRendererError::Internal("empty frame".to_owned()))?,
    )
    .ok_or_else(|| FFramesRendererError::Internal("invalid frame buffer".to_owned()))
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Font families for labels: the requested one, then whatever the database has.
fn label_font_family(fontdb: &usvgr::fontdb::Database, preferred: &str) -> String {
    let mut families = vec![format!("'{}'", preferred)];
    if let Some(face) = fontdb.faces().next()
        && let Some((family, _)) = face.families.first()
    {
        families.push(format!("'{family}'"));
    }
    families.push("sans-serif".to_owned());
    families.join(", ")
}

/// Lays `cells` out in a grid with `columns` columns and a label bar under every frame.
pub fn contact_sheet(
    cells: &[SheetCell],
    columns: usize,
    fontdb: &usvgr::fontdb::Database,
    font_family: &str,
) -> FFramesRendererResult<RgbaFrame> {
    let first = cells
        .first()
        .ok_or_else(|| FFramesRendererError::Custom("no frames for the contact sheet".into()))?;
    let (cell_w, cell_h) = (first.frame.width, first.frame.height);
    let columns = columns.clamp(1, cells.len()) as u32;
    let rows = (cells.len() as u32).div_ceil(columns);
    let gap = 8;
    let label_h = (cell_w / 16).clamp(18, 40);
    let sheet_w = columns * cell_w + (columns + 1) * gap;
    let sheet_h = rows * (cell_h + label_h) + (rows + 1) * gap;

    let mut sheet = tiny_skia::Pixmap::new(sheet_w, sheet_h)
        .ok_or_else(|| FFramesRendererError::Internal("contact sheet is too large".into()))?;
    sheet.fill(tiny_skia::Color::from_rgba8(24, 24, 27, 255));

    let mut labels = String::new();
    let family = escape_xml(&label_font_family(fontdb, font_family));
    for (i, cell) in cells.iter().enumerate() {
        let col = i as u32 % columns;
        let row = i as u32 / columns;
        let x = gap + col * (cell_w + gap);
        let y = gap + row * (cell_h + label_h + gap);

        sheet.draw_pixmap(
            x as i32,
            y as i32,
            to_pixmap(&cell.frame)?.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::identity(),
            None,
        );

        labels.push_str(&format!(
            r##"<text x="{}" y="{}" font-size="{}" font-family="{family}" fill="#f4f4f5">{}</text>"##,
            x + 6,
            y + cell_h + label_h * 3 / 4,
            label_h * 3 / 5,
            escape_xml(&cell.label),
        ));
    }

    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{sheet_w}" height="{sheet_h}">{labels}</svg>"#
    );
    let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), fontdb)
        .map_err(|err| FFramesRendererError::Internal(format!("contact sheet labels: {err}")))?;
    let ctx = svgr::Context::new_from_pixmap_unsafe(&sheet);
    svgr::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut sheet.as_mut(),
        &mut svgr::SvgrCache::none(),
        &svgr::PixmapPool::new(),
        &ctx,
    );

    Ok(RgbaFrame::from_premultiplied(
        sheet_w,
        sheet_h,
        sheet.take(),
    ))
}

/// Blends frames into one image, later frames more opaque, so a movement shows as a trail
/// ending in the last frame.
pub fn onion_skin(frames: &[RgbaFrame]) -> FFramesRendererResult<RgbaFrame> {
    let last = frames
        .last()
        .ok_or_else(|| FFramesRendererError::Custom("no frames for the onion skin".into()))?;
    if frames
        .iter()
        .any(|f| f.width != last.width || f.height != last.height)
    {
        return Err(FFramesRendererError::Custom(
            "onion skin frames have different sizes".into(),
        ));
    }

    // Weights grow linearly: the last frame dominates, the first is the faintest.
    let weights: Vec<f32> = (1..=frames.len()).map(|w| w as f32).collect();
    let total: f32 = weights.iter().sum();
    let mut pixels = vec![0u8; last.pixels.len()];
    for (i, px) in pixels.iter_mut().enumerate() {
        let value: f32 = frames
            .iter()
            .zip(&weights)
            .map(|(frame, w)| frame.pixels[i] as f32 * w)
            .sum();
        *px = (value / total).round().clamp(0., 255.) as u8;
    }

    Ok(RgbaFrame {
        width: last.width,
        height: last.height,
        pixels,
    })
}
