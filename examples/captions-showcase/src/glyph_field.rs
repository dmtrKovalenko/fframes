//! A cached distance/normal texture derived from `FFrames`' actual rasterized glyphs.
//!
//! R stores signed distance in pixels as `0.5 + distance / (2 * DISTANCE_RANGE)`;
//! positive distances are inside the glyph. G/B store the unit gradient toward
//! increasing distance as `0.5 + gradient.xy * 0.5`. A is always opaque. Keep the
//! original glyph texture for exact edge coverage: this 8-bit field supplies the
//! wider bevel, rim and material shading, not replacement text antialiasing.

use fframes::media::ImageData;

pub const DISTANCE_RANGE: f32 = 32.0;

/// Build once per cached glyph, retaining its dimensions and padding.
///
/// The two separable Euclidean distance transforms take O(width * height) time.
/// A transparent virtual border also gives fully filled input a finite interior
/// distance. WASM currently has no native pixel readback and returns `None`.
pub fn from_glyph(image: &ImageData<'_>) -> Option<ImageData<'static>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let pixels = fframes::media::image_pixels(&image.image)?;
        let rgba = field_pixels(
            pixels.width as usize,
            pixels.height as usize,
            pixels.data.as_ref(),
        )?;
        Some(fframes::media::rendered_image(
            pixels.width,
            pixels.height,
            rgba,
        ))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = image;
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn field_pixels(width: usize, height: usize, rgba: &[u8]) -> Option<Vec<u8>> {
    let count = width.checked_mul(height)?;
    if width == 0 || height == 0 || rgba.len() != count.checked_mul(4)? {
        return None;
    }
    let padded_width = width.checked_add(2)?;
    let padded_height = height.checked_add(2)?;
    let mut inside = vec![false; padded_width.checked_mul(padded_height)?];
    let mut glyph_top = height;
    let mut glyph_bottom = 0;
    for y in 0..height {
        for x in 0..width {
            let covered = rgba[(y * width + x) * 4 + 3] >= 128;
            inside[(y + 1) * padded_width + x + 1] = covered;
            if covered {
                glyph_top = glyph_top.min(y);
                glyph_bottom = glyph_bottom.max(y + 1);
            }
        }
    }
    let to_inside = distance_squared(&inside, padded_width, padded_height, true);
    let to_outside = distance_squared(&inside, padded_width, padded_height, false);
    let mut signed = vec![0.0; count];
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let padded = (y + 1) * padded_width + x + 1;
            let alpha = rgba[index * 4 + 3];
            let distance = match alpha {
                0 => 0.5 - to_inside[padded].sqrt(),
                255 => to_outside[padded].sqrt() - 0.5,
                // Preserve subpixel location at the coverage edge. Distances away
                // from that one-pixel edge use the exact binary EDT above.
                _ => f32::from(alpha) / 255.0 - 0.5,
            };
            // An empty glyph has no inside seed. Keep its background finite so
            // normal differences cannot produce NaN; it still encodes saturated R.
            signed[index] = distance.max(-DISTANCE_RANGE * 2.0);
        }
    }

    // Binary distance-transform sites step between neighboring coverage pixels.
    // Their one-pixel differences can therefore make a smooth diagonal bevel
    // alternate between two lighting normals. Smooth only the surface used for
    // normals, once when this cached texture is prepared. R retains the exact
    // signed distances and the original glyph retains its antialiased coverage.
    let normal_surface = smooth_normal_surface(&signed, width, height);
    // Large hero lettering magnifies even a tiny remaining difference between
    // raster sites. Estimate its normal across a wider local neighborhood; use
    // actual covered glyph height, so transparent effect padding does not change
    // the lighting. This leaves the distance used for contour positions intact.
    let normal_radius = glyph_bottom
        .saturating_sub(glyph_top)
        .div_ceil(64)
        .clamp(2, 8);
    let mut output = vec![0; count * 4];
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let left = normal_surface[y * width + x.saturating_sub(normal_radius)];
            let right = normal_surface[y * width + (x + normal_radius).min(width - 1)];
            let up = normal_surface[y.saturating_sub(normal_radius) * width + x];
            let down = normal_surface[(y + normal_radius).min(height - 1) * width + x];
            let dx = right - left;
            let dy = down - up;
            let length = dx.hypot(dy);
            let (nx, ny) = if length > 1e-6 {
                (dx / length, dy / length)
            } else {
                (0.0, 0.0)
            };
            let encode = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            output[index * 4] = encode(0.5 + signed[index] / (2.0 * DISTANCE_RANGE));
            output[index * 4 + 1] = encode(0.5 + nx * 0.5);
            output[index * 4 + 2] = encode(0.5 + ny * 0.5);
            output[index * 4 + 3] = 255;
        }
    }
    Some(output)
}

/// Separable five-tap binomial blur for the cached lighting surface only.
#[cfg(not(target_arch = "wasm32"))]
fn smooth_normal_surface(signed: &[f32], width: usize, height: usize) -> Vec<f32> {
    const WEIGHTS: [f32; 5] = [1.0, 4.0, 6.0, 4.0, 1.0];
    let mut horizontal = vec![0.0; signed.len()];
    let mut smoothed = vec![0.0; signed.len()];
    for y in 0..height {
        for x in 0..width {
            horizontal[y * width + x] = WEIGHTS
                .iter()
                .enumerate()
                .map(|(tap, weight)| {
                    let sample_x = x.saturating_add(tap).saturating_sub(2).min(width - 1);
                    signed[y * width + sample_x] * weight
                })
                .sum::<f32>()
                / 16.0;
        }
    }
    for y in 0..height {
        for x in 0..width {
            smoothed[y * width + x] = WEIGHTS
                .iter()
                .enumerate()
                .map(|(tap, weight)| {
                    let sample_y = y.saturating_add(tap).saturating_sub(2).min(height - 1);
                    horizontal[sample_y * width + x] * weight
                })
                .sum::<f32>()
                / 16.0;
        }
    }
    smoothed
}

#[cfg(not(target_arch = "wasm32"))]
fn distance_squared(mask: &[bool], width: usize, height: usize, seed: bool) -> Vec<f32> {
    let mut grid: Vec<_> = mask
        .iter()
        .map(|inside| if *inside == seed { 0.0 } else { f32::INFINITY })
        .collect();
    let longest = width.max(height);
    let mut input = vec![0.0; longest];
    let mut output = vec![0.0; longest];
    let mut sites = vec![0; longest];
    let mut breaks = vec![0.0; longest + 1];
    for row in grid.chunks_exact_mut(width) {
        edt_line(row, &mut output[..width], &mut sites, &mut breaks);
        row.copy_from_slice(&output[..width]);
    }
    for x in 0..width {
        for y in 0..height {
            input[y] = grid[y * width + x];
        }
        edt_line(
            &input[..height],
            &mut output[..height],
            &mut sites,
            &mut breaks,
        );
        for y in 0..height {
            grid[y * width + x] = output[y];
        }
    }
    grid
}

/// Lower envelope of the finite parabolas `(x - site)^2 + input[site]`.
#[cfg(not(target_arch = "wasm32"))]
fn edt_line(input: &[f32], output: &mut [f32], sites: &mut [usize], breaks: &mut [f64]) {
    let Some(first) = input.iter().position(|value| value.is_finite()) else {
        output.fill(f32::INFINITY);
        return;
    };
    sites[0] = first;
    breaks[0] = f64::NEG_INFINITY;
    breaks[1] = f64::INFINITY;
    let mut last = 0;
    for (q, value) in input.iter().enumerate().skip(first + 1) {
        if !value.is_finite() {
            continue;
        }
        let intersection = |site: usize| {
            let q = q as f64;
            let p = site as f64;
            (f64::from(*value) + q * q - f64::from(input[site]) - p * p) / (2.0 * (q - p))
        };
        let mut split = intersection(sites[last]);
        while last > 0 && split <= breaks[last] {
            last -= 1;
            split = intersection(sites[last]);
        }
        last += 1;
        sites[last] = q;
        breaks[last] = split;
        breaks[last + 1] = f64::INFINITY;
    }
    let mut segment = 0;
    for (x, value) in output.iter_mut().enumerate() {
        while segment < last && breaks[segment + 1] < x as f64 {
            segment += 1;
        }
        let delta = x as f64 - sites[segment] as f64;
        *value = (delta * delta + f64::from(input[sites[segment]])) as f32;
    }
}
