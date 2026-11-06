use std::sync::LazyLock;

use fframes::animation::{AnimationRuntime, Easing};

pub const OVERVIEW: f32 = 0.58;
pub const THANOS: f32 = 1.77;
pub const CANCEL_HOVER: f32 = 2.92;
pub const CONFIRM: f32 = 3.93;
pub const YES: f32 = 5.14;
pub const SNAP: f32 = 6.46;
pub const MAP_RETURN: f32 = 7.88;
pub const COMMENTS: f32 = 9.72;
pub const COMMENT_FOCUS: f32 = 1.48;
pub const DURATION: f32 = 13.20;

pub const CLICKS: [f32; 4] = [OVERVIEW, THANOS, CONFIRM, YES];
pub const COMMENT_COUNT: usize = 19;

const CURSOR: &[(f32, f32, f32)] = &[
    (0.00, 1013.0, 414.0),
    (0.20, 1082.0, 299.0),
    (0.54, 1115.0, 133.0),
    (0.75, 1115.0, 133.0),
    (1.20, 1143.0, 169.0),
    (1.67, 1259.0, 129.0),
    (1.93, 1259.0, 129.0),
    (2.27, 1083.0, 426.0),
    (2.58, 976.0, 681.0),
    (2.70, 976.0, 681.0),
    (CANCEL_HOVER, 818.0, 681.0),
    (3.26, 818.0, 681.0),
    (3.70, 985.0, 681.0),
    (4.08, 985.0, 681.0),
    (4.45, 1030.0, 754.0),
    (5.02, 977.0, 681.0),
    (5.26, 977.0, 681.0),
    (5.58, 1090.0, 760.0),
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
