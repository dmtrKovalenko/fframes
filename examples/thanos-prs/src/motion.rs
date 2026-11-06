use std::sync::LazyLock;

use fframes::animation::{AnimationRuntime, Easing};

pub const PICKUP: f32 = 0.34;
pub const DROP: f32 = 1.34;
pub const OVERVIEW: f32 = 2.18;
pub const THANOS: f32 = 3.37;
pub const CANCEL_HOVER: f32 = 4.52;
pub const CONFIRM: f32 = 5.53;
pub const YES: f32 = 6.74;
pub const SNAP: f32 = 8.06;
pub const MAP_RETURN: f32 = 9.48;
pub const COMMENTS: f32 = 11.32;
pub const COMMENT_FOCUS: f32 = 1.48;
pub const DURATION: f32 = 14.80;

pub const CLICKS: [f32; 4] = [OVERVIEW, THANOS, CONFIRM, YES];
pub const COMMENT_COUNT: usize = 19;

const CURSOR: &[(f32, f32, f32)] = &[
    (0.00, 1034.0, 299.0),
    (0.27, 772.0, 413.0),
    (PICKUP, 772.0, 413.0),
    (0.53, 780.0, 431.0),
    (1.19, 790.0, 539.0),
    (DROP, 790.0, 539.0),
    (1.53, 829.0, 532.0),
    (1.83, 1081.0, 322.0),
    (2.14, 1046.0, 130.0),
    (2.35, 1046.0, 130.0),
    (2.80, 1143.0, 169.0),
    (3.27, 1208.0, 128.0),
    (3.53, 1208.0, 128.0),
    (3.87, 1083.0, 426.0),
    (4.18, 1038.0, 693.0),
    (4.30, 1038.0, 693.0),
    (CANCEL_HOVER, 872.0, 691.0),
    (4.86, 872.0, 691.0),
    (5.30, 1038.0, 693.0),
    (5.68, 1038.0, 693.0),
    (6.05, 1030.0, 754.0),
    (6.62, 1038.0, 693.0),
    (6.86, 1038.0, 693.0),
    (7.18, 1090.0, 760.0),
];

static EASE_IN_OUT: LazyLock<AnimationRuntime> =
    LazyLock::new(|| AnimationRuntime::new(1.0, &Easing::EaseInOut));
static EASE_OUT: LazyLock<AnimationRuntime> =
    LazyLock::new(|| AnimationRuntime::new(1.0, &Easing::EaseOut));

pub fn phase(t: f32, start: f32, end: f32) -> f32 {
    ((t - start) / (end - start)).clamp(0.0, 1.0)
}

pub fn ease_out(t: f32) -> f32 {
    EASE_OUT.solve(&t)
}

pub fn ramp(t: f32, start: f32, end: f32) -> f32 {
    EASE_IN_OUT.solve(&phase(t, start, end))
}

pub fn overview(t: f32) -> f32 {
    browse(t).max(ramp(t, MAP_RETURN, MAP_RETURN + 0.38))
}

pub fn browse(t: f32) -> f32 {
    phase(t, OVERVIEW, OVERVIEW + 0.42) * (1.0 - phase(t, THANOS + 0.14, THANOS + 0.52))
}

pub fn cursor(t: f32) -> (f32, f32) {
    for points in CURSOR.windows(2) {
        let [(begin, x1, y1), (end, x2, y2)] = [points[0], points[1]];
        if t <= end {
            let p = ramp(t, begin, end);
            let arc = p * (1.0 - p) * 0.23;
            return (
                x1 + (x2 - x1) * p - (y2 - y1) * arc,
                y1 + (y2 - y1) * p + (x2 - x1) * arc,
            );
        }
    }
    (1090.0, 760.0)
}

pub fn hover(t: f32, left: f32, top: f32, width: f32, height: f32) -> f32 {
    let (x, y) = cursor(t - 0.018);
    let edge = (x - left)
        .min(left + width - x)
        .min(y - top)
        .min(top + height - y);
    ramp(edge, -4.0, 12.0)
}

pub fn press(t: f32, click: f32) -> f32 {
    let age = t - click;
    ramp(age, -0.025, 0.014) * (1.0 - ramp(age, 0.015, 0.13))
}

pub fn comment_entry(index: usize) -> f32 {
    let next = index.saturating_sub(1) as f32;
    if index == 0 {
        0.0
    } else {
        0.58 + 0.066 * next - 0.0011 * next * next
    }
}

pub fn random(mut n: u32) -> f32 {
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb_352d);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846c_a68b);
    (n ^ (n >> 16)) as f32 / u32::MAX as f32
}
