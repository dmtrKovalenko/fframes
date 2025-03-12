use fframes::Video;
use rand::Rng;
use std::ops::Range;

use crate::PixelVideo;

#[derive(Debug)]
pub struct BokehCircle {
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
    pub fill: String,
    pub opacity: f64,
    pub filter: String,
    pub amplitude_factor_x: f64,
    pub amplitude_factor_y: f64,
    pub final_heart_point: (f64, f64),
}

fn map_to_heart_coordinates(index: usize, total_points: usize, heart_scale: f64) -> (f64, f64) {
    let t = (index as f64 / total_points as f64) * 2.0 * std::f64::consts::PI;

    let x = 16.0 * t.sin().powi(3);
    let y = -(13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos());

    // Scale and center the heart
    let center_x = PixelVideo::WIDTH as f64 / 2.0;
    let center_y = PixelVideo::HEIGHT as f64 / 2.0;

    // Scale the heart relative to screen height
    let actual_scale = heart_scale * (PixelVideo::HEIGHT as f64 / 50.0);

    let scaled_x = center_x + x * actual_scale;
    let scaled_y = center_y + y * actual_scale;

    (scaled_x, scaled_y)
}

/// Generates bokeh circles with sinusoidal pattern and random deviation
pub fn generate_bokeh(rng: &mut impl Rng, count: Range<usize>) -> Vec<BokehCircle> {
    let count = rng.gen_range(count);

    // Container for all bokeh circles
    let mut circles = Vec::with_capacity(count);

    // Canvas dimensions
    let width = 1920.0;
    let height = 1080.0;

    // Sine wave parameters
    let center_y = height / 2.0;
    let amplitude = 250.0;

    // Gradient and filter options
    let gradients = [
        "url(#bokeh-gold-bright)",
        "url(#bokeh-gold-medium)",
        "url(#bokeh-gold-soft)",
    ];
    let filters = [
        "url(#bokeh-blur-large)",
        "url(#bokeh-blur-medium)",
        "url(#bokeh-blur-small)",
    ];

    let main_wave_count = count * 6 / 10; // 60% follow main sine wave closely
    let secondary_wave_count = count * 3 / 10; // 30% follow with more deviation
    let random_count = count - main_wave_count - secondary_wave_count; // 10% completely random

    // Generate main sine wave bokeh (60% of total)
    for i in 0..main_wave_count {
        // Distribute evenly across width
        let x = width * (i as f64 / main_wave_count as f64);

        // Calculate sine wave position
        let wave_x = x / width * 2.0 * std::f64::consts::PI * 1.3; // 2 complete waves
        let sine_offset = amplitude * wave_x.sin();

        // Add moderate random deviation (up to 30% of amplitude)
        let deviation = rng.gen_range(-amplitude * 0.3..amplitude * 0.3);

        // Final position with bounds checking
        let x_with_jitter = (x + rng.gen_range(
            -width / main_wave_count as f64 * 0.3..width / main_wave_count as f64 * 0.3,
        ))
        .min(width - 10.0)
        .max(10.0);
        let y = (center_y + sine_offset + deviation)
            .min(height - 10.0)
            .max(10.0);

        // Size based on position in the wave
        let wave_position = (sine_offset / amplitude).abs(); // 0 to 1 based on position in wave
        let radius = if wave_position < 0.3 {
            // Near the center of the wave
            rng.gen_range(20.0..35.0)
        } else if wave_position < 0.7 {
            // Middle area of the wave
            rng.gen_range(12.0..25.0)
        } else {
            // Edges of the wave
            rng.gen_range(5.0..15.0)
        };

        // Opacity based on size
        let opacity = if radius > 25.0 {
            rng.gen_range(0.5..0.7)
        } else if radius > 15.0 {
            rng.gen_range(0.4..0.6)
        } else {
            rng.gen_range(0.3..0.5)
        };

        circles.push(BokehCircle::create(
            x_with_jitter,
            y,
            radius,
            opacity,
            &gradients,
            &filters,
            map_to_heart_coordinates(i, count, 1.0),
            rng,
        ));
    }

    // Generate secondary sine wave bokeh (30% of total) with more deviation
    for i in 0..secondary_wave_count {
        // Distribute evenly across width with offset from main wave
        let x = width * (i as f64 / secondary_wave_count as f64)
            + width / (2.0 * secondary_wave_count as f64);

        // Calculate sine wave position with phase shift
        let wave_x = x / width * 2.0 * std::f64::consts::PI * 2.0 + 0.5; // Phase shifted
        let sine_offset = amplitude * 0.8 * wave_x.sin(); // Slightly reduced amplitude

        // Add larger random deviation (up to 60% of amplitude)
        let deviation = rng.gen_range(-amplitude * 0.6..amplitude * 0.6);

        // Final position with bounds checking
        let x_with_jitter = (x + rng.gen_range(
            -width / secondary_wave_count as f64 * 0.4..width / secondary_wave_count as f64 * 0.4,
        ))
        .min(width - 10.0)
        .max(10.0);
        let y = (center_y + sine_offset + deviation)
            .min(height - 10.0)
            .max(10.0);

        // Generally smaller circles for secondary wave
        let radius = rng.gen_range(3.0..20.0);

        // Slightly lower opacity
        let opacity = rng.gen_range(0.2..0.5);

        circles.push(BokehCircle::create(
            x_with_jitter,
            y,
            radius,
            opacity,
            &gradients,
            &filters,
            map_to_heart_coordinates(i + main_wave_count, count, 1.0),
            rng,
        ));
    }

    // Generate completely random bokeh (10% of total)
    for i in 0..random_count {
        let x = rng.gen_range(10.0..width - 10.0);
        let y = rng.gen_range(10.0..height - 10.0);

        // Random sizes with preference for smaller circles
        let size_roll = rng.gen_range(0.0..1.0);
        let radius = if size_roll > 0.9 {
            rng.gen_range(20.0..30.0) // Few large circles
        } else if size_roll > 0.7 {
            rng.gen_range(10.0..20.0) // Some medium circles
        } else {
            rng.gen_range(3.0..10.0) // Mostly small circles
        };

        let opacity = rng.gen_range(0.2..0.5);

        circles.push(BokehCircle::create(
            x,
            y,
            radius,
            opacity,
            &gradients,
            &filters,
            map_to_heart_coordinates(
                i + main_wave_count + secondary_wave_count,
                count,
                1.0,
            ),
            rng,
        ));
    }

    circles
}

// Add this implementation for BokehCircle
impl BokehCircle {
    fn create(
        cx: f64,
        cy: f64,
        r: f64,
        opacity: f64,
        gradients: &[&str],
        filters: &[&str],
        final_heart_point: (f64, f64),
        rng: &mut impl Rng,
    ) -> Self {
        let size_factor = 1.0 - (r / 40.0f64).min(0.7); // 0.3 to 1.0
        let gradient_idx = rng.gen_range(0..gradients.len());

        // Choose filter based on radius
        let filter_idx = if r > 25.0 {
            0 // Large blur for large circles
        } else if r > 15.0 {
            1 // Medium blur for medium circles
        } else {
            2 // Small blur for small circles
        };

        BokehCircle {
            cx,
            cy,
            r,
            fill: gradients[gradient_idx].to_string(),
            opacity,
            filter: filters[filter_idx].to_string(),
            amplitude_factor_x: rng.gen_range(-1.5..1.5) * size_factor,
            amplitude_factor_y: rng.gen_range(-1.5..1.5) * size_factor,
            final_heart_point,
        }
    }
}
