use std::fmt::Write;

// COPYRIGHT
// Not modestly copied code from the https://www.particleincell.com/2012/bezier-splines/
// The algorithm is taking up base points and using Thomas algorithm to find the unknown curve
// points on a bezier curve. Works by one axis taking one base point and returns 2 control points.
pub fn compute_control_points(points: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let n = points.len() - 1;
    let mut p1 = vec![0.0; n + 1];
    let mut p2 = vec![0.0; n + 1];
    let mut a = vec![0.0; n + 1];
    let mut b = vec![0.0; n + 1];
    let mut c = vec![0.0; n + 1];
    let mut r = vec![0.0; n + 1];

    a[0] = 0.0;
    b[0] = 2.0;
    c[0] = 1.0;
    r[0] = points[0] + 2.0 * points[1];

    for i in 1..n {
        a[i] = 1.0;
        b[i] = 4.0;
        c[i] = 1.0;
        r[i] = 4.0 * points[i] + 2.0 * points[i + 1];
    }

    a[n - 1] = 2.0;
    b[n - 1] = 7.0;
    c[n - 1] = 0.0;
    r[n - 1] = 8.0 * points[n - 1] + points[n];

    for i in 1..n {
        let m = a[i] / b[i - 1];
        b[i] -= m * c[i - 1];
        r[i] -= m * r[i - 1];
    }

    p1[n - 1] = r[n - 1] / b[n - 1];
    for i in (0..n).rev() {
        p1[i] = (r[i] - c[i] * p1[i + 1]) / b[i];
    }

    for i in 0..n - 1 {
        p2[i] = 2.0 * points[i + 1] - p1[i + 1];
    }

    p2[n - 1] = 0.5 * (points[n] + p1[n - 1]);

    (p1, p2)
}

/// Takes an input the frequencies as f32 ranged 0.0..1.0, x step between frequesnce,
/// and y multiplicator (represents the max height assigned to frequency at 1.0 value)
/// Returns the svg path's d value as a string with corresponding curve.
pub fn frequencies_to_path<FreqIter: Iterator<Item = f32>>(
    x_step: usize,
    y_multiplicator: f32,
    frequencies: FreqIter,
) -> String {
    let mut path = String::from("");

    let (base_x_points, base_y_points): (Vec<f32>, Vec<f32>) = std::iter::once(0.0)
        .chain(frequencies)
        .chain(std::iter::once(0.0))
        .enumerate()
        .map(|(i, frequency)| {
            let x = i * x_step;
            let y = frequency * y_multiplicator;
            let y = y.min(600.0);

            (x as f32, y)
        })
        .unzip();

    let (x1_control_points, x2_control_points) = compute_control_points(&base_x_points);
    let (y1_control_points, y2_control_points) = compute_control_points(&base_y_points);

    // can't be const until https://github.com/rust-lang/rust/issues/57241 landed
    fn align_y(y: f32) -> f32 {
        1080. - y
    }

    for (index, (x, y)) in base_x_points
        .into_iter()
        .zip(base_y_points.into_iter())
        .enumerate()
    {
        if index == 0 {
            write!(path, " M {} {}", x, align_y(y)).expect("Failed to write to string");
            continue;
        }

        write!(
            path,
            " C {} {} {} {} {} {}",
            x1_control_points[index - 1],
            align_y(y1_control_points[index - 1]),
            x2_control_points[index - 1],
            align_y(y2_control_points[index - 1]),
            x,
            align_y(y)
        )
        .expect("Failed to write to string")
    }

    path.push_str(" z");

    path
}
