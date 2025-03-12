//! https://github.com/gre/bezier-easing
//! BezierEasing - use bezier curve for transition easing function
//! by Gaëtan Renaudeau 2014 - 2015 – MIT License
//! Ported to Rust

// These values are established by empiricism with tests (tradeoff: performance VS precision)
const NEWTON_ITERATIONS: usize = 4;
const NEWTON_MIN_SLOPE: f32 = 0.001;
const SUBDIVISION_PRECISION: f32 = 0.0000001;
const SUBDIVISION_MAX_ITERATIONS: usize = 10;

const K_SPLINE_TABLE_SIZE: usize = 11;
const K_SAMPLE_STEP_SIZE: f32 = 1.0 / (K_SPLINE_TABLE_SIZE as f32 - 1.0);

#[inline]
fn a(a1: f32, a2: f32) -> f32 {
    1.0 - 3.0 * a2 + 3.0 * a1
}

#[inline]
fn b(a1: f32, a2: f32) -> f32 {
    3.0 * a2 - 6.0 * a1
}

#[inline]
fn c(a1: f32) -> f32 {
    3.0 * a1
}

#[inline]
fn get_slope(t: f32, a1: f32, a2: f32) -> f32 {
    3.0 * a(a1, a2) * t * t + 2.0 * b(a1, a2) * t + c(a1)
}

#[inline]
fn calc_bezier(t: f32, a1: f32, a2: f32) -> f32 {
    ((a(a1, a2) * t + b(a1, a2)) * t + c(a1)) * t
}

fn binary_subdivide(x: f32, a: f32, b: f32, x1: f32, x2: f32) -> f32 {
    let mut current_x: f32;
    let mut current_t: f32;
    let mut i = 0;
    let mut a_a = a;
    let mut a_b = b;

    loop {
        current_t = a_a + (a_b - a_a) / 2.0;
        current_x = calc_bezier(current_t, x1, x2) - x;
        if current_x > 0.0 {
            a_b = current_t;
        } else {
            a_a = current_t;
        }

        if !(current_x.abs() > SUBDIVISION_PRECISION && i < SUBDIVISION_MAX_ITERATIONS) {
            break;
        }
        i += 1;
    }

    current_t
}

fn newton_raphson_iterate(x: f32, guess_t: f32, x1: f32, x2: f32) -> f32 {
    let mut guess = guess_t;

    for _ in 0..NEWTON_ITERATIONS {
        let current_slope = get_slope(guess, x1, x2);
        if current_slope == 0.0 {
            return guess;
        }
        let current_x = calc_bezier(guess, x1, x2) - x;
        guess -= current_x / current_slope;
    }

    guess
}

/// A struct representing a cubic bezier curve for animation easing.
#[derive(Clone, Copy, Debug)]
pub struct CubicBezierRuntime {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    sample_values: [f32; K_SPLINE_TABLE_SIZE],
}

impl CubicBezierRuntime {
    pub fn ease_in() -> Self {
        CubicBezierRuntime::new(0.42, 0.0, 1.0, 1.0)
    }

    pub fn ease_out() -> Self {
        CubicBezierRuntime::new(0.0, 0.0, 0.58, 1.0)
    }

    pub fn ease_in_out() -> Self {
        CubicBezierRuntime::new(0.42, 0.0, 0.58, 1.0)
    }

    pub fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        // Clamp x values to [0, 1] range
        let x1_clamped = x1.max(0.0).min(1.0);
        let x2_clamped = x2.max(0.0).min(1.0);

        // Precompute samples table
        let mut sample_values = [0.0; K_SPLINE_TABLE_SIZE];
        for i in 0..K_SPLINE_TABLE_SIZE {
            sample_values[i] = calc_bezier(i as f32 * K_SAMPLE_STEP_SIZE, x1_clamped, x2_clamped);
        }

        CubicBezierRuntime {
            x1: x1_clamped,
            y1,
            x2: x2_clamped,
            y2,
            sample_values,
        }
    }

    pub fn solve(&self, x: f32) -> f32 {
        let x_clamped = x.max(0.0).min(1.0);

        if self.x1 == self.y1 && self.x2 == self.y2 {
            return x;
        }

        if x == 0.0 || x == 1.0 {
            return x;
        }

        calc_bezier(self.get_t_for_x(x_clamped), self.y1, self.y2)
    }

    fn get_t_for_x(&self, x: f32) -> f32 {
        let mut interval_start = 0.0;
        let mut current_sample = 1;
        let last_sample = K_SPLINE_TABLE_SIZE - 1;

        while current_sample != last_sample && self.sample_values[current_sample] <= x {
            interval_start += K_SAMPLE_STEP_SIZE;
            current_sample += 1;
        }
        current_sample -= 1;

        // Interpolate to provide an initial guess for t
        let dist = (x - self.sample_values[current_sample])
            / (self.sample_values[current_sample + 1] - self.sample_values[current_sample]);
        let guess_for_t = interval_start + dist * K_SAMPLE_STEP_SIZE;

        let initial_slope = get_slope(guess_for_t, self.x1, self.x2);
        if initial_slope >= NEWTON_MIN_SLOPE {
            newton_raphson_iterate(x, guess_for_t, self.x1, self.x2)
        } else if initial_slope == 0.0 {
            guess_for_t
        } else {
            binary_subdivide(
                x,
                interval_start,
                interval_start + K_SAMPLE_STEP_SIZE,
                self.x1,
                self.x2,
            )
        }
    }
}

#[test]
fn cubic_bezier_animation() {
    let runtime = CubicBezierRuntime::new(0.25, 0.1, 0.25, 1.0);
    assert_eq!(runtime.solve(0.0), 0.0);
    assert_eq!(runtime.solve(0.5), 0.5);
    assert_eq!(runtime.solve(1.0), 1.0);
}
