//! A port of <https://github.com/gre/bezier-easing>
//! by Gaëtan Renaudeau 2014 - 2026 – MIT License

/// Solves x(t) = ((2a * t + 3b) * t + 3c) * t = x for t, with x in (0, 1):
/// u = 1/t is the largest real root of x·u³ − 3c·u² − 3b·u − 2a = 0
fn solve_t_for_x(x: f64, a: f64, b: f64, c: f64) -> f64 {
    let j = 1.0 / c.max(x.sqrt());
    let k = x * j;
    let l = k * j;
    let s = c * j;
    let q = b * l;
    let m = s * s + q;
    let h = -s * (s * s + 1.5 * q) - a * k * l;
    let d = h * h - m * m * m;
    let v = if m == 0.0 || d > 1e-12 * h * h {
        // one real root (Cardano)
        let u = -(if h < 0.0 { h - d.sqrt() } else { h + d.sqrt() }).cbrt();
        let v = u + m / u;
        // triple root (m = h = 0) gives NaN
        if v.is_nan() {
            0.0
        } else {
            v
        }
    } else {
        // three real roots, take the largest
        let r = m.sqrt();
        2.0 * r * ((-h / (m * r)).clamp(-1.0, 1.0).acos() / 3.0).cos()
    };
    (k / (v + s)).min(1.0)
}

#[derive(Clone, Copy, Debug)]
pub struct CubicBezierRuntime {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
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
        CubicBezierRuntime {
            x1: x1.clamp(0.0, 1.0),
            y1,
            x2: x2.clamp(0.0, 1.0),
            y2,
        }
    }

    pub fn solve(&self, x: f32) -> f32 {
        if self.x1 == self.y1 && self.x2 == self.y2 {
            return x;
        }

        // x outside (0, 1) saturates to 0 / 1 (NaN stays NaN)
        if !(x > 0.0 && x < 1.0) {
            return x.clamp(0.0, 1.0);
        }

        // x(t) = ((2a * t + 3b) * t + 3c) * t with a = (3x1 - 3x2 + 1) / 2, b = x2 - 2x1, c = x1
        let (x1, y1, x2, y2) = (
            self.x1 as f64,
            self.y1 as f64,
            self.x2 as f64,
            self.y2 as f64,
        );
        let t = solve_t_for_x(
            x as f64,
            (3.0 * x1 - 3.0 * x2 + 1.0) / 2.0,
            x2 - 2.0 * x1,
            x1,
        );
        ((((3.0 * y1 - 3.0 * y2 + 1.0) * t + 3.0 * (y2 - 2.0 * y1)) * t + 3.0 * y1) * t) as f32
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
    fn test_steep_curve_is_monotonic() {
        // (1, 0, 0, 1) used to jump backwards around x = 0.5
        let easing = CubicBezierRuntime::new(1.0, 0.0, 0.0, 1.0);
        let mut previous = 0.0;
        for i in 0..=10000 {
            let y = easing.solve(0.49 + 0.02 * i as f32 / 10000.0);
            assert!(y >= previous, "not monotonic at step {}", i);
            previous = y;
        }
        assert_relative_eq!(easing.solve(0.4996), 0.4306, epsilon = 1e-4);
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
