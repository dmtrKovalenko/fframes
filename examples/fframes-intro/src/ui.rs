//! Palette, type and the small pieces of HUD every scene shares.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use fframes::{FFramesContext, FontQuery, FontStyle, Frame, Svgr};

pub const BG: &str = "#0b0b0b";
pub const BONE: &str = "#ece8e1";
pub const ORANGE: &str = "#fb6a22";
pub const EMBER: &str = "#8a3712";
pub const GREY: &str = "#76726c";
pub const DIM: &str = "#2b2926";
pub const PAPER: &str = "#e8e4db";
pub const INK: &str = "#121110";

pub const DISPLAY: &str = "Archivo Black";
pub const COND: &str = "Anton";
pub const MONO: &str = "IBM Plex Mono";
pub const SERIF: &str = "Instrument Serif";

static WIDTHS: LazyLock<Mutex<HashMap<(String, usize, u16, bool, String), f32>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Width of a string in pixels, measured once per (font, text) and cached.
/// Falls back to an estimate while fonts are not available.
pub fn measure(
    frame: &mut Frame,
    ctx: &FFramesContext,
    family: &str,
    size: usize,
    weight: u16,
    italic: bool,
    text: &str,
) -> f32 {
    let key = (family.to_owned(), size, weight, italic, text.to_owned());
    if let Some(w) = WIDTHS.lock().unwrap().get(&key) {
        return *w;
    }
    let query = FontQuery {
        family,
        size,
        weight,
        style: if italic { FontStyle::Italic } else { FontStyle::Normal },
        ..Default::default()
    };
    // `text_width` wants the text to live as long as the context; results are
    // cached, so every distinct string is leaked at most once.
    let text: &'static str = Box::leak(text.to_owned().into_boxed_str());
    match frame.text_width(ctx, query, text) {
        Some(w) => {
            let w = w as f32;
            WIDTHS.lock().unwrap().insert(key, w);
            w
        }
        None => text.chars().count() as f32 * size as f32 * 0.6,
    }
}

/// Crop-mark corners around a rectangle.
pub fn corners(x: f32, y: f32, w: f32, h: f32, arm: f32, color: &str, width: f32) -> Svgr<'static> {
    let d = format!(
        "M{x} {y1} V{y} H{x1} M{x2} {y} H{x3} V{y1} M{x3} {y2} V{y3} H{x2} M{x1} {y3} H{x} V{y2}",
        x = x,
        y = y,
        x1 = x + arm,
        x2 = x + w - arm,
        x3 = x + w,
        y1 = y + arm,
        y2 = y + h - arm,
        y3 = y + h,
    );
    fframes::svgr!(<path d={d} fill="none" stroke={color.to_owned()} stroke-width={width} />)
}

/// A small mono label, uppercase tracking like engineering drawings.
pub fn label(x: f32, y: f32, text: String, color: &str, size: f32, anchor: &'static str) -> Svgr<'static> {
    fframes::svgr!(
        <text x={x} y={y} font-family={MONO} font-weight="500" font-size={size} letter-spacing="2.5"
              fill={color.to_owned()} text-anchor={anchor}>{text}</text>
    )
}

/// An annotation: a dot on the target, an elbow line and a label.
#[allow(clippy::too_many_arguments)]
pub fn callout(
    tx: f32,
    ty: f32,
    lx: f32,
    ly: f32,
    text: String,
    color: &str,
    progress: f32,
) -> Svgr<'static> {
    if progress <= 0.0 {
        return Svgr::empty();
    }
    let p = progress.clamp(0.0, 1.0);
    let mx = lx;
    let my = ty;
    // draw-on: first leg then second
    let l1 = (mx - tx).abs() + 0.01;
    let l2 = (ly - my).abs() + 0.01;
    let total = l1 + l2;
    let drawn = total * p;
    let path = format!("M{tx} {ty} H{mx} V{ly}");
    let dash = format!("{drawn} {total}");
    let anchor = if lx >= tx { "start" } else { "end" };
    let text_x = if lx >= tx { lx + 10.0 } else { lx - 10.0 };
    let text_opacity = ((p - 0.6) / 0.4).clamp(0.0, 1.0);
    fframes::svgr!(
        <g>
            <circle cx={tx} cy={ty} r="5" fill={color.to_owned()} opacity={p.min(1.0)} />
            <circle cx={tx} cy={ty} r="12" fill="none" stroke={color.to_owned()} stroke-width="1.5" opacity={p * 0.6} />
            <path d={path} fill="none" stroke={color.to_owned()} stroke-width="1.5" stroke-dasharray={dash} />
            <text x={text_x} y={ly + 6.0} font-family={MONO} font-weight="500" font-size="20" letter-spacing="2"
                  fill={color.to_owned()} text-anchor={anchor} opacity={text_opacity}>{text}</text>
        </g>
    )
}

/// Timecode `mm:ss:ff` at 60 fps.
pub fn timecode(seconds: f32) -> String {
    let total_frames = (seconds * 60.0).max(0.0) as u32;
    let ff = total_frames % 60;
    let s = (total_frames / 60) % 60;
    let m = total_frames / 3600;
    format!("00:{m:02}:{s:02}:{ff:02}")
}

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Horizontal strip of text drawn letter by letter with per-letter offsets:
/// `f(i) -> (dx, dy, opacity, scale)` for every character.
#[allow(clippy::too_many_arguments)]
pub fn letters<F>(
    frame: &mut Frame,
    ctx: &FFramesContext,
    x: f32,
    y: f32,
    text: &str,
    family: &'static str,
    size: usize,
    weight: u16,
    fill: &str,
    tracking: f32,
    f: F,
) -> Svgr<'static>
where
    F: Fn(usize) -> (f32, f32, f32, f32),
{
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let prefix: String = chars[..i].iter().collect();
        let px = measure(frame, ctx, family, size, weight, false, &prefix) + tracking * i as f32;
        let (dx, dy, o, s) = f(i);
        if o <= 0.001 {
            continue;
        }
        let cw = measure(frame, ctx, family, size, weight, false, &c.to_string());
        let cx = x + px + cw / 2.0;
        let t = format!(
            "translate({} {}) scale({}) translate({} 0)",
            cx + dx,
            y + dy,
            s.max(0.001),
            -cw / 2.0
        );
        let ch = c.to_string();
        out.push(fframes::svgr!(
            <text transform={t} font-family={family} font-weight={weight} font-size={size} fill={fill.to_owned()} opacity={o}>{ch}</text>
        ));
    }
    fframes::svgr!(<g>{out}</g>)
}

/// A word that slams in with a spring from `(dx, dy)` and leaves a motion
/// trail of ember copies behind it while it moves (the fframes brand motif).
pub struct Slam {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub family: &'static str,
    pub size: f32,
    pub fill: &'static str,
    pub spacing: f32,
    pub anchor: &'static str,
    pub italic: bool,
}

impl Slam {
    pub fn new(x: f32, y: f32, text: impl Into<String>, family: &'static str, size: f32, fill: &'static str) -> Self {
        Slam { x, y, text: text.into(), family, size, fill, spacing: -size * 0.035, anchor: "start", italic: false }
    }
    pub fn anchor(mut self, anchor: &'static str) -> Self {
        self.anchor = anchor;
        self
    }
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self.spacing = 0.0;
        self
    }

    /// `since` is beats since the entrance started.
    pub fn draw(&self, since: f32, dx: f32, dy: f32) -> Svgr<'static> {
        if since < 0.0 {
            return Svgr::empty();
        }
        let off = |t: f32| {
            let s = crate::beat::snap(t.max(0.0));
            ((1.0 - s) * dx, (1.0 - s) * dy)
        };
        let text = |ox: f32, oy: f32, fill: &str, opacity: f32| {
            let t = self.text.clone();
            let style = if self.italic { "italic" } else { "normal" };
            fframes::svgr!(
                <text x={self.x + ox} y={self.y + oy} text-anchor={self.anchor} font-family={self.family} font-style={style}
                      font-size={self.size} letter-spacing={self.spacing} fill={fill.to_owned()} opacity={opacity}>{t}</text>
            )
        };
        let mut layers = Vec::new();
        let trail = [(0.16, EMBER, 0.35), (0.10, "#c4531c", 0.5), (0.05, ORANGE, 0.65)];
        for (lag, color, o) in trail {
            let (ox, oy) = off(since - lag);
            let (cx, cy) = off(since);
            let moving = ((ox - cx).powi(2) + (oy - cy).powi(2)).sqrt();
            if moving > 2.0 && since > lag * 0.5 {
                layers.push(text(ox, oy, color, o * (moving / 40.0).min(1.0)));
            }
        }
        let (ox, oy) = off(since);
        layers.push(text(ox, oy, self.fill, crate::beat::prog(since, 0.0, 0.06)));
        fframes::svgr!(<g>{layers}</g>)
    }
}
