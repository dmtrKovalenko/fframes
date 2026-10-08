//! Procedural pen ink. Every mark is a centreline turned into a filled ribbon whose
//! width follows a pressure curve. Marks are drawn on along their length and "boil":
//! the centreline is re-jittered on every exposure (two frames), like hand-drawn
//! animation traced frame by frame.

use std::fmt::Write;

use fframes::Svgr;

pub(crate) const BROWN: &str = "#3d2419";
pub(crate) const BLACK: &str = "#161311";
pub(crate) const RED: &str = "#c8281b";
pub(crate) const ORANGE: &str = "#fb6a22";
pub(crate) const CREAM: &str = "#f3eee1";

pub(crate) type P = [f32; 2];

pub(crate) fn hash(n: f32) -> f32 {
    ((n * 12.989_8 + 78.233).sin() * 43_758.547).rem_euclid(1.0)
}

/// Smooth value noise in [-1, 1].
pub(crate) fn noise(x: f32, seed: f32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let f = f * f * (3. - 2. * f);
    let a = hash(i + seed * 17.13);
    let b = hash(i + 1. + seed * 17.13);
    (a + (b - a) * f) * 2. - 1.
}

pub(crate) fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

pub(crate) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Catmull-Rom spline through the control points, sampled every `spacing` px.
pub(crate) fn spline(ctrl: &[P], spacing: f32) -> Vec<P> {
    if ctrl.len() < 2 {
        return ctrl.to_vec();
    }
    let mut dense = Vec::new();
    for i in 0..ctrl.len() - 1 {
        let p0 = ctrl[i.saturating_sub(1)];
        let p1 = ctrl[i];
        let p2 = ctrl[i + 1];
        let p3 = ctrl[(i + 2).min(ctrl.len() - 1)];
        let len = ((p2[0] - p1[0]).powi(2) + (p2[1] - p1[1]).powi(2)).sqrt();
        let steps = ((len / spacing).ceil() as usize).max(2);
        for s in 0..steps {
            let t = s as f32 / steps as f32;
            let (t2, t3) = (t * t, t * t * t);
            let c = |a: f32, b: f32, c: f32, d: f32| {
                0.5 * (2. * b
                    + (-a + c) * t
                    + (2. * a - 5. * b + 4. * c - d) * t2
                    + (-a + 3. * b - 3. * c + d) * t3)
            };
            dense.push([c(p0[0], p1[0], p2[0], p3[0]), c(p0[1], p1[1], p2[1], p3[1])]);
        }
    }
    dense.push(*ctrl.last().unwrap());
    dense
}

#[derive(Clone, Debug)]
pub(crate) struct Stroke {
    pub pts: Vec<P>,
    pub width: f32,
    pub color: &'static str,
    pub seed: f32,
    pub boil: f32,
    pub opacity: f32,
    /// Fraction of the length the pen needs to reach full pressure / to lift off.
    pub taper: (f32, f32),
}

impl Stroke {
    pub(crate) fn new(ctrl: &[P], width: f32, color: &'static str, seed: f32) -> Self {
        Self {
            pts: spline(ctrl, 3.),
            width,
            color,
            seed,
            boil: 1.6,
            opacity: 1.,
            taper: (0.12, 0.3),
        }
    }

    pub(crate) fn boil(mut self, amount: f32) -> Self {
        self.boil = amount;
        self
    }

    pub(crate) fn taper(mut self, start: f32, end: f32) -> Self {
        self.taper = (start, end);
        self
    }

    /// The outline of the first `reveal` of the stroke at the given exposure.
    pub(crate) fn outline(&self, reveal: f32, exposure: usize) -> Option<String> {
        let n = self.pts.len();
        if n < 2 || reveal <= 0.003 {
            return None;
        }
        let m = ((reveal.min(1.) * (n - 1) as f32).ceil() as usize + 1).clamp(2, n);
        let e = exposure as f32;
        let drawing = reveal < 0.999;
        let mut left = Vec::with_capacity(m);
        let mut right = Vec::with_capacity(m);
        for i in 0..m {
            let a = self.pts[i.saturating_sub(1)];
            let b = self.pts[(i + 1).min(n - 1)];
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len = (dx * dx + dy * dy).sqrt().max(1e-4);
            let (nx, ny) = (-dy / len, dx / len);
            let t = i as f32 / (n - 1) as f32;
            let jitter = noise(t * 4.5 + e * 0.73, self.seed + e * 3.1) * self.boil;
            let wob = noise(t * 2.2, self.seed + e * 1.7) * self.boil * 0.6;
            let p = self.pts[i];
            let cx = p[0] + nx * jitter + wob * 0.4;
            let cy = p[1] + ny * jitter - wob * 0.3;
            let pressure = smoothstep(0., self.taper.0, t) * smoothstep(1., 1. - self.taper.1, t);
            let pressure = 0.22 + 0.78 * pressure * (0.82 + 0.18 * noise(t * 7., self.seed + 9.));
            let mut w = self.width * pressure;
            if drawing {
                // Ink pools under the nib while it is still moving.
                let tip = (m - 1) as f32 / (n - 1) as f32;
                w *= 1. + 0.5 * smoothstep(tip - 0.05, tip, t);
            }
            let w = w * 0.5;
            left.push([cx + nx * w, cy + ny * w]);
            right.push([cx - nx * w, cy - ny * w]);
        }
        let mut d = String::with_capacity(m * 26);
        let _ = write!(d, "M{:.1} {:.1}", left[0][0], left[0][1]);
        for p in &left[1..] {
            let _ = write!(d, "L{:.1} {:.1}", p[0], p[1]);
        }
        for p in right.iter().rev() {
            let _ = write!(d, "L{:.1} {:.1}", p[0], p[1]);
        }
        d.push('Z');
        Some(d)
    }

    pub(crate) fn draw(&self, reveal: f32, exposure: usize) -> Svgr<'static> {
        let Some(d) = self.outline(reveal, exposure) else {
            return Svgr::empty();
        };
        // A blot where the nib touched the paper.
        let blot = if self.width >= 8. && hash(self.seed * 3.7) > 0.45 {
            let p = self.pts[0];
            let r = self.width * (0.45 + 0.35 * hash(self.seed + 1.));
            fframes::svgr!(<circle cx={p[0]} cy={p[1]} r={r} fill={self.color} />)
        } else {
            Svgr::empty()
        };
        fframes::svgr!(<g opacity={self.opacity}><path d={d} fill={self.color} />{blot}</g>)
    }

    /// Erase the stroke from its start, the way a pen line is "pulled" off screen.
    pub(crate) fn draw_window(&self, from: f32, to: f32, exposure: usize) -> Svgr<'static> {
        let n = self.pts.len();
        let a = ((from.clamp(0., 1.) * (n - 1) as f32) as usize).min(n.saturating_sub(2));
        let sub = Stroke {
            pts: self.pts[a..].to_vec(),
            ..self.clone()
        };
        let span = (1. - from).max(1e-3);
        sub.draw(((to - from) / span).clamp(0., 1.), exposure)
    }
}

/// Exposure index: ink is redrawn on twos.
pub(crate) fn exposure(global_frame: usize) -> usize {
    global_frame / 2
}

// ---------------------------------------------------------------- shapes

/// A quick underline with a flick at the end.
pub(crate) fn underline(x0: f32, x1: f32, y: f32, seed: f32) -> Vec<P> {
    let j = |k: f32| (hash(seed + k) - 0.5) * 6.;
    let w = x1 - x0;
    vec![
        [x0 - 14., y + 5. + j(1.)],
        [x0 + w * 0.3, y + 1. + j(2.)],
        [x0 + w * 0.7, y - 1. + j(3.)],
        [x1 + 10., y - 4. + j(4.)],
        [x1 + 26., y - 18. + j(5.)],
    ]
}

/// A loop around a word: a little more than one turn with a loose tail.
pub(crate) fn encircle(cx: f32, cy: f32, rx: f32, ry: f32, seed: f32) -> Vec<P> {
    let start = -2.6 + hash(seed) * 0.6;
    let turns = 1.12 + hash(seed + 1.) * 0.12;
    let count = 14;
    let mut pts = Vec::with_capacity(count + 2);
    for i in 0..=count {
        let t = i as f32 / count as f32;
        let a = start + t * turns * std::f32::consts::TAU;
        let r = 1. + noise(t * 3., seed + 4.) * 0.07 + t * 0.06;
        pts.push([cx + a.cos() * rx * r, cy + a.sin() * ry * r]);
    }
    let last = pts[pts.len() - 1];
    let a = start + turns * std::f32::consts::TAU;
    pts.push([
        last[0] + a.cos() * 30. - a.sin() * 26.,
        last[1] + a.sin() * 18. + a.cos() * 22.,
    ]);
    pts
}

/// A random pen gesture inside a box: loops, hooks and ticks.
pub(crate) fn gesture(x: f32, y: f32, w: f32, h: f32, seed: f32, count: usize) -> Vec<P> {
    let mut pts = Vec::with_capacity(count);
    let mut px = x + hash(seed) * w;
    let mut py = y + hash(seed + 0.5) * h;
    for i in 0..count {
        let k = seed + i as f32 * 1.37;
        pts.push([px, py]);
        let a = hash(k + 2.) * std::f32::consts::TAU;
        let r = (0.25 + hash(k + 3.) * 0.55) * w.max(h);
        px = (px + a.cos() * r).clamp(x, x + w);
        py = (py + a.sin() * r * 0.7).clamp(y, y + h);
    }
    pts
}

/// Cursive loops, like a signature or a word scribbled in the margin.
pub(crate) fn cursive(x: f32, y: f32, w: f32, h: f32, seed: f32) -> Vec<P> {
    let loops = 3 + (hash(seed) * 4.) as usize;
    let count = loops * 4 + 2;
    let slant = 0.25 + hash(seed + 1.) * 0.25;
    (0..count)
        .map(|i| {
            let s = i as f32 / (count - 1) as f32;
            let ph = s * loops as f32 * std::f32::consts::TAU;
            let amp = h * (0.35 + 0.25 * noise(s * 3., seed + 2.));
            let r = amp * (0.55 + 0.3 * hash(seed + i as f32));
            let yy = y + h * 0.5 - ph.cos() * amp + noise(s * 2., seed + 5.) * h * 0.2;
            let xx = x + s * w - ph.sin() * r * 0.8 + (y + h * 0.5 - yy) * slant;
            [xx, yy]
        })
        .collect()
}

/// Part of an ellipse from angle `a0` to `a1` with a wobbling radius.
pub(crate) fn arc(cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, a1: f32, seed: f32) -> Vec<P> {
    let count = (((a1 - a0).abs() * 4.) as usize).max(3);
    (0..=count)
        .map(|i| {
            let t = i as f32 / count as f32;
            let a = lerp(a0, a1, t);
            let r = 1. + noise(t * 2.5, seed) * 0.08;
            [cx + a.cos() * rx * r, cy + a.sin() * ry * r]
        })
        .collect()
}

pub(crate) fn zigzag(x: f32, y: f32, w: f32, h: f32, count: usize, seed: f32) -> Vec<P> {
    (0..count)
        .map(|i| {
            let t = i as f32 / (count - 1) as f32;
            let up = if i % 2 == 0 { 0. } else { h };
            [
                x + t * w + (hash(seed + i as f32) - 0.5) * 10.,
                y + up + (hash(seed + i as f32 + 9.) - 0.5) * h * 0.3,
            ]
        })
        .collect()
}

/// A splat: a smooth lobed blob with a few thin arms that end in droplets, plus
/// thrown droplets. `grow` 0..1 opens it.
pub(crate) fn splat(
    cx: f32,
    cy: f32,
    r: f32,
    seed: f32,
    grow: f32,
    color: &'static str,
) -> Svgr<'static> {
    if grow <= 0.01 {
        return Svgr::empty();
    }
    let g = grow.min(1.2);
    let count = 22;
    let arms: Vec<usize> = (0..5)
        .map(|i| (hash(seed + 40. + i as f32) * count as f32) as usize)
        .collect();
    let pts: Vec<P> = (0..count)
        .map(|i| {
            let a = i as f32 / count as f32 * std::f32::consts::TAU
                + (hash(seed + i as f32) - 0.5) * 0.12;
            let arm = arms.contains(&i);
            let rr = if arm {
                r * (1.25 + hash(seed - i as f32) * 0.7)
            } else {
                r * (0.72 + hash(seed + 7. * i as f32) * 0.3)
            };
            [cx + a.cos() * rr * g, cy + a.sin() * rr * g]
        })
        .collect();
    // Smooth closed curve through the midpoints, control points on the samples.
    let mid = |a: P, b: P| [f32::midpoint(a[0], b[0]), f32::midpoint(a[1], b[1])];
    let first = mid(pts[count - 1], pts[0]);
    let mut d = String::new();
    let _ = write!(d, "M{:.1} {:.1}", first[0], first[1]);
    for i in 0..count {
        let m = mid(pts[i], pts[(i + 1) % count]);
        let _ = write!(
            d,
            "Q{:.1} {:.1} {:.1} {:.1}",
            pts[i][0], pts[i][1], m[0], m[1]
        );
    }
    d.push('Z');
    let tips = arms
        .iter()
        .map(|&i| {
            let p = pts[i];
            let rad = r * (0.1 + hash(seed + i as f32 * 3.) * 0.08) * g.sqrt();
            fframes::svgr!(<circle cx={p[0]} cy={p[1]} r={rad} fill={color} />)
        })
        .collect::<Vec<_>>();
    let drops = (0..12)
        .map(|i| {
            let k = seed + i as f32 * 2.17;
            let a = hash(k) * std::f32::consts::TAU;
            let dist = r * (1.1 + hash(k + 1.) * 1.4) * smoothstep(0., 1., g);
            let rad = r * (0.03 + hash(k + 2.).powi(2) * 0.11) * g.sqrt();
            let stretch = 1. + hash(k + 3.).powi(2) * 1.4;
            let deg = a.to_degrees();
            fframes::svgr!(<ellipse cx="0" cy="0" rx={rad * stretch} ry={rad}
                transform={format!("translate({:.1} {:.1}) rotate({deg:.1})", cx + a.cos() * dist, cy + a.sin() * dist)}
                fill={color} />)
        })
        .collect::<Vec<_>>();
    fframes::svgr!(<g><path d={d} fill={color} />{tips}{drops}</g>)
}

/// An ink drip running down from (x, y).
pub(crate) fn drip(x: f32, y: f32, len: f32, w: f32, color: &'static str) -> Svgr<'static> {
    if len < 1. {
        return Svgr::empty();
    }
    let bulb = w * 0.9;
    let d = format!(
        "M{:.1} {:.1} C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1} L{:.1} {:.1} C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}Z",
        x - w,
        y,
        x - w * 0.5,
        y + len * 0.3,
        x - w * 0.35,
        y + len * 0.7,
        x - w * 0.3,
        y + len,
        x + w * 0.3,
        y + len,
        x + w * 0.35,
        y + len * 0.7,
        x + w * 0.5,
        y + len * 0.3,
        x + w,
        y
    );
    fframes::svgr!(<g><path d={d} fill={color} /><circle cx={x} cy={y + len} r={bulb} fill={color} /></g>)
}

/// A four-point sparkle.
pub(crate) fn sparkle(x: f32, y: f32, r: f32, color: &'static str) -> Svgr<'static> {
    if r < 0.5 {
        return Svgr::empty();
    }
    let k = r * 0.16;
    let d = format!(
        "M{x:.1} {:.1} Q{:.1} {:.1} {:.1} {y:.1} Q{:.1} {:.1} {x:.1} {:.1} Q{:.1} {:.1} {:.1} {y:.1} Q{:.1} {:.1} {x:.1} {:.1}Z",
        y - r,
        x + k,
        y - k,
        x + r,
        x + k,
        y + k,
        y + r,
        x - k,
        y + k,
        x - r,
        x - k,
        y - k,
        y - r
    );
    fframes::svgr!(<path d={d} fill={color} />)
}

/// Draw-on progress of a mark that starts at `start` seconds and takes `dur`.
pub(crate) fn reveal(t: f32, start: f32, dur: f32) -> f32 {
    let x = ((t - start) / dur).clamp(0., 1.);
    // Pens accelerate off the start and brake into the end.
    1. - (1. - x).powf(1.6)
}
