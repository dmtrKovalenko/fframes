//! The playback control bar drawn over the video. It mirrors the web editor dock: a floating
//! pill with quiet round buttons, one solid primary action and the system slider, using the
//! dark tokens of the `ChatGPT` design system (`@openai/apps-sdk-ui`) and the system font.
//!
//! The studio draws its transport with the same buttons; the pill itself is the focus
//! view's dock. Nothing on it pops: every state is drawn from a [`Sample`] of the player's
//! [`Motion`], so the dock rises in and drops out, hover fills fade, a pressed button gives
//! and the play glyph turns into the pause glyph.

use fframes_skia_renderer::skia_safe::{
    self, BlurStyle, Canvas, Color, Contains, Font, FontMgr, FontStyle, MaskFilter, Paint,
    PaintCap, PaintJoin, PaintStyle, Path, PathBuilder, PathFillType, Point, RRect, Rect, Typeface,
    font_style,
};

use crate::icons::{self, Icon};
use crate::motion::{Easing, Sample};

/// What a click on the bar does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Control {
    Back,
    PlayPause,
    Forward,
    Seek,
    Timeline,
    Scene(usize),
    /// The focus view: the video alone, with the dock.
    Focus,
    Loop,
    Fullscreen,
    /// The bar background, swallows clicks so they do not toggle playback.
    Bar,
}

/// Everything the bar shows, gathered by the app every frame.
pub(crate) struct ControlsState {
    pub frame: usize,
    pub duration_in_frames: usize,
    pub fps: usize,
    pub fullscreen: bool,
    /// Where every motion of the chrome is right now: the view, the dock, playback,
    /// looping, hover and press of every button.
    pub motion: Sample,
}

impl ControlsState {
    fn progress(&self) -> f32 {
        let last_frame = self.duration_in_frames.saturating_sub(1).max(1);
        (self.frame as f32 / last_frame as f32).clamp(0., 1.)
    }
}

// Dark theme tokens
const SURFACE_ELEVATED: Color = Color::from_argb(240, 0x30, 0x30, 0x30);
const HAIRLINE: Color = Color::from_argb(26, 255, 255, 255);
const TEXT: Color = Color::from_rgb(0xff, 0xff, 0xff);
const TEXT_SECONDARY: Color = Color::from_rgb(0xaf, 0xaf, 0xaf);
const GHOST_HOVER: Color = Color::from_argb(31, 255, 255, 255);
const GHOST_ACTIVE: Color = Color::from_argb(41, 255, 255, 255);
// fframes brand orange: only on the primary action and the playback progress
const BRAND: Color = Color::from_rgb(0xfb, 0x6a, 0x22);
const BRAND_HOVER: Color = Color::from_rgb(0xe2, 0x55, 0x07);
const TEXT_ON_BRAND: Color = Color::from_rgb(0xff, 0xff, 0xff);
const TRACK: Color = Color::from_argb(31, 255, 255, 255);
const THUMB: Color = Color::from_rgb(0x0d, 0x0d, 0x0d);

// Sizes in logical pixels
const BAR_HEIGHT: f32 = 52.;
const BAR_MAX_WIDTH: f32 = 720.;
const BAR_MARGIN: f32 = 16.;
const BAR_PADDING: f32 = 6.;
const BUTTON: f32 = 36.;
const PRIMARY_BUTTON: f32 = 40.;
const ICON: f32 = 20.;
const GAP: f32 = 4.;
const LABEL_PADDING: f32 = 12.;
const FONT_SIZE: f32 = 14.;
const TRACK_HEIGHT: f32 = 4.;
const THUMB_SIZE: f32 = 14.;
/// Vertical area around the track that grabs the pointer.
const TRACK_HIT_HEIGHT: f32 = 24.;
/// The dock rises this far into place as it appears, and drops back out.
const BAR_RISE: f32 = 10.;
/// How much a button gives while it is held.
const PRESS_SCALE: f32 = 0.08;
/// The track thickens by this much under the pointer.
const TRACK_GROW: f32 = 1.;
/// The glyph flashed in the middle of the picture: its plate and the glyph on it.
const FLASH: Color = Color::from_argb(178, 0x1c, 0x1c, 0x1e);
const FLASH_PLATE: f32 = 88.;
const FLASH_ICON: f32 = 40.;
const FLASH_GROW: f32 = 0.25;

/// Same preference order as the `--font-sans` stack of the design system.
const SYSTEM_FONTS: &[&str] = &[
    ".AppleSystemUIFont",
    "SF Pro Text",
    "Segoe UI",
    "Noto Sans",
    "Helvetica Neue",
    "Helvetica",
    "Arial",
];

pub(crate) struct Controls {
    regular: Typeface,
    medium: Typeface,
    icons: IconPaths,
    pub(crate) frame_mark: skia_safe::svg::Dom,
}

struct IconPaths {
    back: Path,
    forward: Path,
    looping: Path,
    focus: Path,
    expand: Path,
    collapse: Path,
}

/// Where everything sits for a given window size, in physical pixels.
pub(crate) struct ControlsLayout {
    scale: f32,
    pub(crate) bar: Rect,
    pub(crate) back: Rect,
    pub(crate) play: Rect,
    pub(crate) forward: Rect,
    current_time: Point,
    pub(crate) track: Rect,
    total_time: Point,
    pub(crate) looping: Rect,
    pub(crate) fullscreen: Rect,
}

impl ControlsLayout {
    pub(crate) fn new(
        controls: &Controls,
        duration_label: &str,
        width: f32,
        height: f32,
        scale: f32,
    ) -> Self {
        // Keep both time labels and icon hitboxes apart in a narrow window.
        let scale = scale.min(width / 400.);
        let px = |logical: f32| logical * scale;
        let font = controls.font(true, scale);
        let time_width = tabular_width(&font, duration_label);

        let bar_width = (width - px(BAR_MARGIN) * 2.).min(px(BAR_MAX_WIDTH)).max(0.);
        let bar = Rect::from_xywh(
            (width - bar_width) / 2.,
            height - px(BAR_MARGIN) - px(BAR_HEIGHT),
            bar_width,
            px(BAR_HEIGHT),
        );
        let center_y = bar.center_y();
        let square = |left: f32, size: f32| {
            Rect::from_xywh(left, center_y - px(size) / 2., px(size), px(size))
        };

        let back = square(bar.left + px(BAR_PADDING), BUTTON);
        let play = square(back.right + px(GAP), PRIMARY_BUTTON);
        let forward = square(play.right + px(GAP), BUTTON);
        let fullscreen = square(bar.right - px(BAR_PADDING) - px(BUTTON), BUTTON);
        let looping = square(fullscreen.left - px(GAP) - px(BUTTON), BUTTON);

        let baseline = center_y + font.metrics().1.cap_height / 2.;
        let current_time = Point::new(forward.right + px(LABEL_PADDING), baseline);
        let total_time = Point::new(looping.left - px(LABEL_PADDING) - time_width, baseline);
        let track_left = current_time.x + time_width + px(LABEL_PADDING);
        let track = Rect::from_ltrb(
            track_left,
            center_y - px(TRACK_HEIGHT) / 2.,
            (total_time.x - px(LABEL_PADDING)).max(track_left),
            center_y + px(TRACK_HEIGHT) / 2.,
        );

        Self {
            scale,
            bar,
            back,
            play,
            forward,
            current_time,
            track,
            total_time,
            looping,
            fullscreen,
        }
    }

    pub(crate) fn hit(&self, x: f32, y: f32) -> Option<Control> {
        let point = Point::new(x, y);
        let track_hit = Rect::from_ltrb(
            self.track.left - self.scale * THUMB_SIZE / 2.,
            self.track.center_y() - self.scale * TRACK_HIT_HEIGHT / 2.,
            self.track.right + self.scale * THUMB_SIZE / 2.,
            self.track.center_y() + self.scale * TRACK_HIT_HEIGHT / 2.,
        );

        [
            (self.back, Control::Back),
            (self.play, Control::PlayPause),
            (self.forward, Control::Forward),
            (self.looping, Control::Loop),
            (self.fullscreen, Control::Fullscreen),
            (track_hit, Control::Seek),
            (self.bar, Control::Bar),
        ]
        .into_iter()
        .find(|(rect, _)| rect.contains(point))
        .map(|(_, control)| control)
    }

    /// Playback progress in `0.0..=1.0` for a pointer at `x`.
    pub(crate) fn progress_at(&self, x: f32) -> f32 {
        ((x - self.track.left) / self.track.width().max(1.)).clamp(0., 1.)
    }
}

impl Controls {
    pub(crate) fn new() -> Self {
        let font_mgr = FontMgr::new();
        let typeface = |weight: font_style::Weight| {
            let style = FontStyle::new(
                weight,
                font_style::Width::NORMAL,
                font_style::Slant::Upright,
            );
            SYSTEM_FONTS
                .iter()
                .find_map(|family| font_mgr.match_family_style(family, style))
                .or_else(|| font_mgr.legacy_make_typeface(None, style))
                .expect("no system font available")
        };

        let icon = |icon: Icon| {
            let path = Path::from_svg(icon.path).expect("valid icon path");
            if icon.even_odd {
                path.with_fill_type(PathFillType::EvenOdd)
            } else {
                path
            }
        };

        let frame_mark =
            skia_safe::svg::Dom::from_str(include_str!("frame-f.svg"), FontMgr::empty())
                .expect("valid fframes mark SVG");

        Self {
            regular: typeface(font_style::Weight::NORMAL),
            medium: typeface(font_style::Weight::MEDIUM),
            frame_mark,
            icons: IconPaths {
                back: icon(icons::BACK),
                forward: icon(icons::FORWARD),
                looping: icon(icons::LOOP),
                focus: icon(icons::FOCUS),
                expand: icon(icons::EXPAND),
                collapse: icon(icons::COLLAPSE),
            },
        }
    }

    pub(crate) fn font(&self, medium: bool, scale: f32) -> Font {
        let typeface = if medium { &self.medium } else { &self.regular };
        let mut font = Font::from_typeface(typeface, FONT_SIZE * scale);
        font.set_subpixel(true);
        font.set_edging(skia_safe::font::Edging::AntiAlias);
        font
    }

    pub(crate) fn draw(&self, canvas: &Canvas, layout: &ControlsLayout, state: &ControlsState) {
        // The dock rises as the studio panel leaves, and is gone before it is back
        let reveal = state.motion.dock * (1. - state.motion.studio);
        if reveal <= 0. {
            return;
        }

        let scale = layout.scale;
        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        // The dock rises into place as it fades in, and drops back out
        canvas.save();
        canvas.translate((0., (1. - reveal) * BAR_RISE * scale));

        // Floating pill: elevation shadow, surface and hairline border
        let radius = layout.bar.height() / 2.;
        let bar = RRect::new_rect_xy(layout.bar, radius, radius);
        let mut shadow = paint.clone();
        shadow.set_color(fade(Color::from_argb(77, 0, 0, 0), reveal));
        shadow.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, 8. * scale, false));
        canvas.draw_rrect(bar.with_offset((0., 8. * scale)), &shadow);

        paint.set_color(fade(SURFACE_ELEVATED, reveal));
        canvas.draw_rrect(bar, &paint);

        let mut border = paint.clone();
        border.set_style(PaintStyle::Stroke);
        border.set_stroke_width(scale.max(1.));
        border.set_color(fade(HAIRLINE, reveal));
        canvas.draw_rrect(bar, &border);

        for (control, rect) in [
            (Control::Back, layout.back),
            (Control::PlayPause, layout.play),
            (Control::Forward, layout.forward),
            (Control::Loop, layout.looping),
            (Control::Fullscreen, layout.fullscreen),
        ] {
            self.draw_button(canvas, control, rect, state, reveal, scale);
        }

        let mut text = paint.clone();
        text.set_color(fade(TEXT, reveal));
        draw_tabular(
            canvas,
            &format_time(state.frame, state.fps),
            layout.current_time,
            &self.font(true, scale),
            &text,
        );
        text.set_color(fade(TEXT_SECONDARY, reveal));
        draw_tabular(
            canvas,
            &format_time(state.duration_in_frames, state.fps),
            layout.total_time,
            &self.font(false, scale),
            &text,
        );

        // System slider: soft track, then the played range and the thumb ring in the brand
        // color. Under the pointer the track thickens and a halo grows around the thumb.
        if layout.track.width() > 0. {
            let hover = state.motion.hover(Control::Seek);
            let track = layout.track.with_outset((0., hover * TRACK_GROW * scale));
            let track_radius = track.height() / 2.;
            paint.set_color(fade(TRACK, reveal));
            canvas.draw_rrect(
                RRect::new_rect_xy(track, track_radius, track_radius),
                &paint,
            );

            let thumb_x = track.left + track.width() * state.progress();
            let mut range = track;
            range.right = thumb_x;
            paint.set_color(fade(BRAND, reveal));
            canvas.draw_rrect(
                RRect::new_rect_xy(range, track_radius, track_radius),
                &paint,
            );

            let thumb_radius = THUMB_SIZE * scale / 2.;
            let center = Point::new(thumb_x, track.center_y());
            if hover > 0. {
                paint.set_color(fade(GHOST_HOVER, reveal * hover));
                canvas.draw_circle(center, thumb_radius * (1. + 0.8 * hover), &paint);
            }
            paint.set_color(fade(BRAND, reveal));
            canvas.draw_circle(center, thumb_radius, &paint);
            paint.set_color(fade(THUMB, reveal));
            canvas.draw_circle(center, thumb_radius - 2. * scale, &paint);
        }

        canvas.restore();
    }

    /// A button of the dock or the studio transport, drawn from the motion in `state`:
    /// its hover fill, how far it is pressed in and, for the primary one, how far the play
    /// glyph has turned into pause. `reveal` fades the whole button with its surface.
    pub(crate) fn draw_button(
        &self,
        canvas: &Canvas,
        control: Control,
        rect: Rect,
        state: &ControlsState,
        reveal: f32,
        scale: f32,
    ) {
        let motion = &state.motion;
        let hover = motion.hover(control);
        let press = motion.press(control);
        let icon = match control {
            Control::PlayPause => {
                return self.primary_button(
                    canvas,
                    rect,
                    motion.glyph,
                    hover,
                    press,
                    reveal,
                    scale,
                );
            }
            Control::Back => &self.icons.back,
            Control::Forward => &self.icons.forward,
            Control::Loop => &self.icons.looping,
            Control::Focus => &self.icons.focus,
            Control::Fullscreen => {
                if state.fullscreen {
                    &self.icons.collapse
                } else {
                    &self.icons.expand
                }
            }
            _ => return,
        };
        let on = if control == Control::Loop {
            motion.looping
        } else {
            0.
        };
        self.ghost_button(canvas, rect, icon, hover, on, press, reveal, scale);
    }

    /// A quiet round button. `hover`, `on` (a toggle that is on) and `press` are each
    /// `0.0..=1.0`; the fill steps up from hover to on so the two never read alike.
    #[allow(clippy::too_many_arguments)]
    fn ghost_button(
        &self,
        canvas: &Canvas,
        rect: Rect,
        icon: &Path,
        hover: f32,
        on: f32,
        press: f32,
        reveal: f32,
        scale: f32,
    ) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        canvas.save();
        press_into(canvas, rect, press);

        let background = mix(
            mix(TRANSPARENT, GHOST_HOVER, hover),
            mix(GHOST_HOVER, GHOST_ACTIVE, hover),
            on,
        );
        if background.a() > 0 {
            paint.set_color(fade(background, reveal));
            canvas.draw_circle(rect.center(), rect.width() / 2., &paint);
        }

        paint.set_color(fade(mix(TEXT_SECONDARY, TEXT, hover.max(on)), reveal));
        draw_icon(canvas, icon, rect, scale, &paint);
        canvas.restore();
    }

    /// The solid brand button with the play glyph, `glyph` turns it into pause.
    #[allow(clippy::too_many_arguments)]
    fn primary_button(
        &self,
        canvas: &Canvas,
        rect: Rect,
        glyph: f32,
        hover: f32,
        press: f32,
        reveal: f32,
        scale: f32,
    ) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        canvas.save();
        press_into(canvas, rect, press);
        paint.set_color(fade(mix(BRAND, BRAND_HOVER, hover), reveal));
        canvas.draw_circle(rect.center(), rect.width() / 2., &paint);

        paint.set_color(fade(TEXT_ON_BRAND, reveal));
        draw_play_pause(canvas, rect, ICON * scale, glyph, &paint);
        canvas.restore();
    }

    /// Playback toggled on the video itself: its glyph on a dark plate in the middle of
    /// the picture, growing as it fades, so the eye needs no bar to see what happened.
    pub(crate) fn flash(
        &self,
        canvas: &Canvas,
        center: Point,
        scale: f32,
        progress: f32,
        playing: bool,
    ) {
        let grow = 1. + FLASH_GROW * Easing::ARRIVAL.apply(progress);
        let alpha = 1. - Easing::EASE_OUT.apply((progress - 0.2) / 0.8);
        let size = FLASH_PLATE * scale * grow;
        let plate = Rect::from_xywh(center.x - size / 2., center.y - size / 2., size, size);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(fade(FLASH, alpha));
        canvas.draw_circle(plate.center(), size / 2., &paint);
        paint.set_color(fade(TEXT, alpha));
        draw_play_pause(
            canvas,
            plate,
            FLASH_ICON * scale * grow,
            f32::from(u8::from(!playing)),
            &paint,
        );
    }
}

const TRANSPARENT: Color = Color::from_argb(0, 255, 255, 255);

/// `color` at `alpha` times its own alpha.
pub(crate) fn fade(color: Color, alpha: f32) -> Color {
    let a = (f32::from(color.a()) * alpha.clamp(0., 1.)).round() as u8;
    Color::from_argb(a, color.r(), color.g(), color.b())
}

/// `a` at `t == 0`, `b` at `t == 1`, with the alpha mixed too (a fully transparent end
/// keeps the other end's hue, so fading in never passes through black).
pub(crate) fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0., 1.);
    let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    let (red_a, green_a, blue_a) = if a.a() == 0 {
        (b.r(), b.g(), b.b())
    } else {
        (a.r(), a.g(), a.b())
    };
    let (red_b, green_b, blue_b) = if b.a() == 0 {
        (a.r(), a.g(), a.b())
    } else {
        (b.r(), b.g(), b.b())
    };
    Color::from_argb(
        channel(a.a(), b.a()),
        channel(red_a, red_b),
        channel(green_a, green_b),
        channel(blue_a, blue_b),
    )
}

/// Shrinks what is drawn next around the middle of `rect` by how far it is pressed.
fn press_into(canvas: &Canvas, rect: Rect, press: f32) {
    if press <= 0. {
        return;
    }
    let s = 1. - PRESS_SCALE * press;
    let center = rect.center();
    canvas.translate((center.x, center.y));
    canvas.scale((s, s));
    canvas.translate((-center.x, -center.y));
}

/// The play triangle and the pause bars are one shape: two quads on the 24 grid whose
/// corners travel between the two, so the glyph turns into the other instead of being
/// swapped. `t` is 0 for play, 1 for pause.
fn draw_play_pause(canvas: &Canvas, rect: Rect, size: f32, t: f32, paint: &Paint) {
    const PLAY: [[f32; 2]; 8] = [
        [7., 5.9],
        [12.4, 8.92],
        [12.4, 15.08],
        [7., 18.1],
        [12.4, 8.92],
        [17.9, 11.85],
        [17.9, 12.15],
        [12.4, 15.08],
    ];
    const PAUSE: [[f32; 2]; 8] = [
        [6.5, 6.5],
        [10.5, 6.5],
        [10.5, 17.5],
        [6.5, 17.5],
        [13.5, 6.5],
        [17.5, 6.5],
        [17.5, 17.5],
        [13.5, 17.5],
    ];
    let t = t.clamp(0., 1.);
    let point = |i: usize| {
        Point::new(
            PLAY[i][0] + (PAUSE[i][0] - PLAY[i][0]) * t,
            PLAY[i][1] + (PAUSE[i][1] - PLAY[i][1]) * t,
        )
    };

    let mut path = PathBuilder::new();
    for quad in [0, 4] {
        path.move_to(point(quad));
        for i in 1..4 {
            path.line_to(point(quad + i));
        }
        path.close();
    }
    let path = path.detach();

    // Stroked and filled in one pass: the round join rounds the corners and the union
    // of both contours closes the seam between the two halves of the triangle
    let mut rounded = paint.clone();
    rounded.set_style(PaintStyle::StrokeAndFill);
    rounded.set_stroke_join(PaintJoin::Round);
    rounded.set_stroke_cap(PaintCap::Round);
    rounded.set_stroke_width(2.);

    canvas.save();
    canvas.translate((rect.center_x() - size / 2., rect.center_y() - size / 2.));
    canvas.scale((size / 24., size / 24.));
    canvas.draw_path(&path, &rounded);
    canvas.restore();
}

/// Icons are drawn on a 24x24 grid, centered in `rect` at the icon size.
fn draw_icon(canvas: &Canvas, icon: &Path, rect: Rect, scale: f32, paint: &Paint) {
    let size = ICON * scale;
    canvas.save();
    canvas.translate((rect.center_x() - size / 2., rect.center_y() - size / 2.));
    canvas.scale((size / 24., size / 24.));
    canvas.draw_path(icon, paint);
    canvas.restore();
}

/// Width of the widest digit, so the time labels do not jitter while playing.
fn digit_width(font: &Font) -> f32 {
    ('0'..='9')
        .map(|digit| font.measure_str(digit.to_string(), None).0)
        .fold(0., f32::max)
}

pub(crate) fn tabular_width(font: &Font, text: &str) -> f32 {
    let digit = digit_width(font);
    text.chars()
        .map(|char| {
            if char.is_ascii_digit() {
                digit
            } else {
                font.measure_str(char.to_string(), None).0
            }
        })
        .sum()
}

pub(crate) fn draw_tabular(
    canvas: &Canvas,
    text: &str,
    origin: Point,
    font: &Font,
    paint: &Paint,
) -> f32 {
    let digit = digit_width(font);
    let mut x = origin.x;
    for char in text.chars() {
        let glyph = char.to_string();
        let width = font.measure_str(&glyph, None).0;
        if char.is_ascii_digit() {
            canvas.draw_str(&glyph, (x + (digit - width) / 2., origin.y), font, paint);
            x += digit;
        } else {
            canvas.draw_str(&glyph, (x, origin.y), font, paint);
            x += width;
        }
    }
    x
}

/// `mm:ss`, or `h:mm:ss` from an hour on, like the web editor.
pub(crate) fn format_time(frame: usize, fps: usize) -> String {
    let seconds = frame / fps.max(1);
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match hours {
        0 => format!("{minutes:02}:{seconds:02}"),
        _ => format!("{hours}:{minutes:02}:{seconds:02}"),
    }
}

#[cfg(test)]
mod tests {
    use fframes_skia_renderer::skia_safe::{AlphaType, ColorType, ImageInfo, surfaces};

    use super::*;
    use crate::motion::Intent;

    #[test]
    fn draws_the_primary_action_and_progress_in_the_brand_color() {
        let controls = Controls::new();
        let (width, height) = (960, 200);
        let layout = ControlsLayout::new(&controls, "00:30", width as f32, height as f32, 1.);
        let mut surface = surfaces::raster_n32_premul((width, height)).expect("raster surface");
        let state = ControlsState {
            frame: 450,
            duration_in_frames: 900,
            fps: 30,
            fullscreen: false,
            motion: Sample::settled(Intent {
                studio: false,
                dock: true,
                playing: false,
                looping: true,
                hovered: None,
                pressed: None,
            }),
        };
        controls.draw(surface.canvas(), &layout, &state);

        let mut pixel = |x: f32, y: f32| {
            let mut rgba = [0u8; 4];
            let info = ImageInfo::new((1, 1), ColorType::RGBA8888, AlphaType::Unpremul, None);
            assert!(surface.read_pixels(&info, &mut rgba, 4, (x as i32, y as i32)));
            Color::from_argb(rgba[3], rgba[0], rgba[1], rgba[2])
        };

        // the play button background, clear of the icon
        assert_eq!(pixel(layout.play.left + 5., layout.play.center_y()), BRAND);
        // the played part of the seek slider
        let played = layout.track.left + layout.track.width() * 0.25;
        assert_eq!(pixel(played, layout.track.center_y()), BRAND);
    }

    #[test]
    fn formats_time_like_the_web_editor() {
        assert_eq!(format_time(0, 30), "00:00");
        assert_eq!(format_time(30 * 75, 30), "01:15");
        assert_eq!(format_time(60 * 3725, 60), "1:02:05");
        assert_eq!(format_time(10, 0), "00:10");
    }

    #[test]
    fn hit_tests_the_controls() {
        let controls = Controls::new();
        let layout = ControlsLayout::new(&controls, "00:30", 1280., 720., 2.);
        let center = |rect: Rect| (rect.center_x(), rect.center_y());

        for (rect, control) in [
            (layout.back, Control::Back),
            (layout.play, Control::PlayPause),
            (layout.forward, Control::Forward),
            (layout.track, Control::Seek),
            (layout.looping, Control::Loop),
            (layout.fullscreen, Control::Fullscreen),
        ] {
            let (x, y) = center(rect);
            assert_eq!(layout.hit(x, y), Some(control));
        }

        assert_eq!(
            layout.hit(layout.bar.left + 2., layout.bar.top + 2.),
            Some(Control::Bar)
        );
        assert_eq!(layout.hit(640., 100.), None);

        assert_eq!(layout.progress_at(layout.track.left - 50.), 0.);
        assert!((layout.progress_at(layout.track.center_x()) - 0.5).abs() < 1e-4);
        assert_eq!(layout.progress_at(layout.track.right + 50.), 1.);
    }

    #[test]
    fn bar_fits_small_windows() {
        let controls = Controls::new();
        let layout = ControlsLayout::new(&controls, "1:02:05", 320., 240., 1.);

        assert!(layout.bar.left >= 0. && layout.bar.right <= 320.);
        assert!(layout.track.width() >= 0.);
    }
}
