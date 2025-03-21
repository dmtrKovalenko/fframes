//! A port of https://github.com/gre/bezier-easing
//! by Gaëtan Renaudeau 2014 - 2015 – MIT License

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
        let x1_clamped = x1.clamp(0.0, 1.0);
        let x2_clamped = x2.clamp(0.0, 1.0);

        let mut sample_values = [0.0; K_SPLINE_TABLE_SIZE];
        for (i, value) in sample_values.iter_mut().enumerate() {
            *value = calc_bezier(i as f32 * K_SAMPLE_STEP_SIZE, x1_clamped, x2_clamped);
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
        let x_clamped = x.clamp(0.0, 1.0);

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

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn identity(x: f32) -> f32 {
        x
    }

    fn all_equals<F1, F2>(be1: F1, be2: F2, samples: usize, max_relative: Option<f32>)
    where
        F1: Fn(f32) -> f32,
        F2: Fn(f32) -> f32,
    {
        for i in 0..=samples {
            let x = i as f32 / samples as f32;
            let a = be1(x);
            let b = be2(x);

            if let Some(epsilon) = max_relative {
                assert_relative_eq!(a, b, max_relative = epsilon, epsilon = f32::EPSILON,);
            } else {
                assert_relative_eq!(a, b, epsilon = f32::EPSILON,);
            }
        }
    }

    #[test]
    fn test_linear_curves() {
        // Test that linear curves are actually linear
        let easing1 = CubicBezierRuntime::new(0.0, 0.0, 1.0, 1.0);
        let easing2 = CubicBezierRuntime::new(1.0, 1.0, 0.0, 0.0);

        all_equals(|x| easing1.solve(x), |x| easing2.solve(x), 100, None);

        all_equals(|x| easing1.solve(x), identity, 100, None);
    }

    #[test]
    fn test_extremes() {
        // Test that the easing function returns 0 at 0 and 1 at 1
        // Using standard test values instead of random ones
        let test_cases = [
            (0.25, 0.1, 0.75, 0.9),
            (0.4, 0.2, 0.6, 0.8),
            (0.1, -0.3, 0.9, 1.3),
            (0.7, 0.5, 0.3, 0.5),
        ];

        for &(a, b, c, d) in &test_cases {
            let easing = CubicBezierRuntime::new(a, b, c, d);

            assert_relative_eq!(easing.solve(0.0), 0.0, epsilon = f32::EPSILON);
            assert_relative_eq!(easing.solve(1.0), 1.0, epsilon = f32::EPSILON);
        }
    }

    #[test]
    fn test_projected_curve() {
        // Test that the easing function approaches the projected value of its x=y projected curve
        let test_cases = [
            (0.25, 0.1, 0.75, 0.9),
            (0.4, 0.2, 0.6, 0.8),
            (0.1, 0.3, 0.9, 0.7),
            (0.7, 0.5, 0.3, 0.5),
        ];

        for &(a, b, c, d) in &test_cases {
            let easing = CubicBezierRuntime::new(a, b, c, d);
            let projected = CubicBezierRuntime::new(b, a, d, c);

            all_equals(
                identity,
                |x| projected.solve(easing.solve(x)),
                100,
                Some(0.05),
            );
        }
    }

    #[test]
    fn test_same_instances() {
        // Test that two instances with the same parameters produce the same results
        let test_cases = [
            (0.25, 0.1, 0.75, 0.9),
            (0.4, 0.2, 0.6, 0.8),
            (0.1, -0.3, 0.9, 1.3),
            (0.7, 0.5, 0.3, 0.5),
        ];

        for &(a, b, c, d) in &test_cases {
            let easing1 = CubicBezierRuntime::new(a, b, c, d);
            let easing2 = CubicBezierRuntime::new(a, b, c, d);

            all_equals(|x| easing1.solve(x), |x| easing2.solve(x), 100, None);
        }
    }

    #[test]
    fn test_invalid_arguments() {
        // Test that the constructor handles invalid arguments
        // Note: In the Rust implementation, values are clamped rather than throwing errors
        let easing_with_invalid_x1 = CubicBezierRuntime::new(-2.0, 0.5, 0.5, 0.5);
        assert_eq!(
            easing_with_invalid_x1.x1, 0.0,
            "x1 should be clamped to 0.0"
        );

        let easing_with_invalid_x1_high = CubicBezierRuntime::new(2.0, 0.5, 0.5, 0.5);
        assert_eq!(
            easing_with_invalid_x1_high.x1, 1.0,
            "x1 should be clamped to 1.0"
        );

        let easing_with_invalid_x2 = CubicBezierRuntime::new(0.5, 0.5, -5.0, 0.5);
        assert_eq!(
            easing_with_invalid_x2.x2, 0.0,
            "x2 should be clamped to 0.0"
        );

        let easing_with_invalid_x2_high = CubicBezierRuntime::new(0.5, 0.5, 5.0, 0.5);
        assert_eq!(
            easing_with_invalid_x2_high.x2, 1.0,
            "x2 should be clamped to 1.0"
        );
    }

    #[test]
    fn test_predefined_curves() {
        // Test the predefined curves
        let ease_in = CubicBezierRuntime::ease_in();
        let ease_out = CubicBezierRuntime::ease_out();
        let ease_in_out = CubicBezierRuntime::ease_in_out();

        assert_eq!(ease_in.x1, 0.42);
        assert_eq!(ease_in.y1, 0.0);
        assert_eq!(ease_in.x2, 1.0);
        assert_eq!(ease_in.y2, 1.0);

        assert_eq!(ease_out.x1, 0.0);
        assert_eq!(ease_out.y1, 0.0);
        assert_eq!(ease_out.x2, 0.58);
        assert_eq!(ease_out.y2, 1.0);

        assert_eq!(ease_in_out.x1, 0.42);
        assert_eq!(ease_in_out.y1, 0.0);
        assert_eq!(ease_in_out.x2, 0.58);
        assert_eq!(ease_in_out.y2, 1.0);
    }
}
