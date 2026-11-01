//! The playback control bar drawn over the video. It mirrors the web editor dock: a floating
//! pill with quiet round buttons, one solid primary action and the system slider, using the
//! dark tokens of the `ChatGPT` design system (`@openai/apps-sdk-ui`) and the system font.
//! Over a video the pill is frosted glass (the frame blurred behind a dark tint), so it
//! reads on light and dark footage alike, like the system players.

use fframes_skia_renderer::skia_safe::{
    self, BlurStyle, Canvas, ClipOp, Color, Color4f, Contains, Font, FontMgr, FontStyle,
    MaskFilter, Paint, PaintStyle, Path, PathFillType, Point, RRect, Rect, TileMode, Typeface,
    canvas::SaveLayerRec, font_style, gradient, image_filters,
};

use crate::icons::{self, Icon};

/// What a click on the bar does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Control {
    Back,
    PlayPause,
    Forward,
    Seek,
    Loop,
    Fullscreen,
    /// The bar background, swallows clicks so they do not toggle playback.
    Bar,
}

/// Everything the bar shows, gathered by the app every frame.
pub(crate) struct ControlsState {
    pub visible: bool,
    pub frame: usize,
    pub duration_in_frames: usize,
    pub fps: usize,
    pub playing: bool,
    pub looping: bool,
    pub fullscreen: bool,
    pub hovered: Option<Control>,
    /// The pointer, in physical pixels, while it is inside the window: the time under it
    /// is shown over the slider while it hovers or drags the thumb.
    pub pointer_x: Option<f32>,
}

impl ControlsState {
    fn progress(&self) -> f32 {
        let last_frame = self.duration_in_frames.saturating_sub(1).max(1);
        (self.frame as f32 / last_frame as f32).clamp(0., 1.)
    }
}

// Dark theme tokens
/// The glass tint over the blurred frame.
const GLASS: Color = Color::from_argb(178, 0x1c, 0x1c, 0x1e);
const HAIRLINE: Color = Color::from_argb(36, 255, 255, 255);
/// The lit top edge of the glass.
const GLASS_HIGHLIGHT: Color = Color::from_argb(56, 255, 255, 255);
const SHADOW: Color = Color::from_argb(64, 0, 0, 0);
const TEXT: Color = Color::from_rgb(0xff, 0xff, 0xff);
const TEXT_SECONDARY: Color = Color::from_rgb(0xaf, 0xaf, 0xaf);
const GHOST_HOVER: Color = Color::from_argb(31, 255, 255, 255);
/// A toggle that is on: a solid step up from hover, so the two never read alike.
const GHOST_PRESSED: Color = Color::from_argb(56, 255, 255, 255);
const GHOST_PRESSED_HOVER: Color = Color::from_argb(72, 255, 255, 255);
const CHIP: Color = Color::from_argb(235, 0x1c, 0x1c, 0x1e);
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
/// How much of the frame behind the bar is blurred into the glass.
const GLASS_BLUR: f32 = 14.;
const CHIP_FONT_SIZE: f32 = 12.;
const CHIP_PADDING_X: f32 = 8.;
const CHIP_HEIGHT: f32 = 24.;
const CHIP_GAP: f32 = 10.;
/// Vertical area around the track that grabs the pointer.
const TRACK_HIT_HEIGHT: f32 = 24.;

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
}

struct IconPaths {
    play: Path,
    pause: Path,
    back: Path,
    forward: Path,
    looping: Path,
    expand: Path,
    collapse: Path,
}

/// Where everything sits for a given window size, in physical pixels.
pub(crate) struct ControlsLayout {
    scale: f32,
    bar: Rect,
    back: Rect,
    play: Rect,
    forward: Rect,
    current_time: Point,
    track: Rect,
    total_time: Point,
    looping: Rect,
    fullscreen: Rect,
}

impl ControlsLayout {
    pub(crate) fn new(
        controls: &Controls,
        duration_label: &str,
        width: f32,
        height: f32,
        scale: f32,
    ) -> Self {
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

        Self {
            regular: typeface(font_style::Weight::NORMAL),
            medium: typeface(font_style::Weight::MEDIUM),
            icons: IconPaths {
                play: icon(icons::PLAY),
                pause: icon(icons::PAUSE),
                back: icon(icons::BACK),
                forward: icon(icons::FORWARD),
                looping: icon(icons::LOOP),
                expand: icon(icons::EXPAND),
                collapse: icon(icons::COLLAPSE),
            },
        }
    }

    fn font(&self, medium: bool, scale: f32) -> Font {
        self.font_sized(medium, FONT_SIZE * scale)
    }

    fn font_sized(&self, medium: bool, size: f32) -> Font {
        let typeface = if medium { &self.medium } else { &self.regular };
        let mut font = Font::from_typeface(typeface, size);
        font.set_subpixel(true);
        font.set_edging(skia_safe::font::Edging::AntiAlias);
        font
    }

    pub(crate) fn draw(&self, canvas: &Canvas, layout: &ControlsLayout, state: &ControlsState) {
        if !state.visible {
            return;
        }

        let scale = layout.scale;
        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        // Floating glass pill: a soft shadow, the frame blurred behind a dark tint, a
        // hairline edge lit at the top
        let radius = layout.bar.height() / 2.;
        let bar = RRect::new_rect_xy(layout.bar, radius, radius);
        let mut shadow = paint.clone();
        shadow.set_color(SHADOW);
        shadow.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, 10. * scale, false));
        canvas.draw_rrect(bar.with_offset((0., 6. * scale)), &shadow);

        canvas.save();
        canvas.clip_rrect(bar, ClipOp::Intersect, true);
        if let Some(blur) = image_filters::blur(
            (GLASS_BLUR * scale, GLASS_BLUR * scale),
            TileMode::Clamp,
            None,
            None,
        ) {
            let layer = SaveLayerRec::default().bounds(&layout.bar).backdrop(&blur);
            canvas.save_layer(&layer);
            canvas.restore();
        }
        paint.set_color(GLASS);
        canvas.draw_rect(layout.bar, &paint);
        canvas.restore();

        let mut border = paint.clone();
        border.set_style(PaintStyle::Stroke);
        border.set_stroke_width(scale.max(1.));
        border.set_color(HAIRLINE);
        let inset = bar.with_inset((scale / 2., scale / 2.));
        canvas.draw_rrect(inset, &border);
        let highlight_colors = [
            Color4f::from(GLASS_HIGHLIGHT),
            Color4f::from(Color::TRANSPARENT),
        ];
        let highlight = gradient::Gradient::new(
            gradient::Colors::new_evenly_spaced(&highlight_colors, TileMode::Clamp, None),
            gradient::Interpolation::default(),
        );
        if let Some(highlight) = gradient::shaders::linear_gradient(
            (
                Point::new(0., layout.bar.top),
                Point::new(0., layout.bar.center_y()),
            ),
            &highlight,
            None,
        ) {
            border.set_color(Color::WHITE);
            border.set_shader(highlight);
            canvas.draw_rrect(inset, &border);
        }

        let hovered = |control| state.hovered == Some(control);

        self.ghost_button(
            canvas,
            layout.back,
            &self.icons.back,
            hovered(Control::Back),
            false,
            scale,
        );
        self.primary_button(
            canvas,
            layout.play,
            if state.playing {
                &self.icons.pause
            } else {
                &self.icons.play
            },
            hovered(Control::PlayPause),
            scale,
        );
        self.ghost_button(
            canvas,
            layout.forward,
            &self.icons.forward,
            hovered(Control::Forward),
            false,
            scale,
        );
        self.ghost_button(
            canvas,
            layout.looping,
            &self.icons.looping,
            hovered(Control::Loop),
            state.looping,
            scale,
        );
        self.ghost_button(
            canvas,
            layout.fullscreen,
            if state.fullscreen {
                &self.icons.collapse
            } else {
                &self.icons.expand
            },
            hovered(Control::Fullscreen),
            false,
            scale,
        );

        let mut text = paint.clone();
        text.set_color(TEXT);
        draw_tabular(
            canvas,
            &format_time(state.frame, state.fps),
            layout.current_time,
            &self.font(true, scale),
            &text,
        );
        text.set_color(TEXT_SECONDARY);
        draw_tabular(
            canvas,
            &format_time(state.duration_in_frames, state.fps),
            layout.total_time,
            &self.font(false, scale),
            &text,
        );

        // System slider: soft track, then the played range and the thumb ring in the brand color
        if layout.track.width() > 0. {
            let track_radius = layout.track.height() / 2.;
            paint.set_color(TRACK);
            canvas.draw_rrect(
                RRect::new_rect_xy(layout.track, track_radius, track_radius),
                &paint,
            );

            let thumb_x = layout.track.left + layout.track.width() * state.progress();
            let mut range = layout.track;
            range.right = thumb_x;
            paint.set_color(BRAND);
            canvas.draw_rrect(
                RRect::new_rect_xy(range, track_radius, track_radius),
                &paint,
            );

            let thumb_radius = THUMB_SIZE * scale / 2.;
            let center = Point::new(thumb_x, layout.track.center_y());
            if hovered(Control::Seek) {
                paint.set_color(GHOST_HOVER);
                canvas.draw_circle(center, thumb_radius * 1.8, &paint);
            }
            paint.set_color(BRAND);
            canvas.draw_circle(center, thumb_radius, &paint);
            paint.set_color(THUMB);
            canvas.draw_circle(center, thumb_radius - 2. * scale, &paint);

            // The time under the pointer, in a chip above the slider while it is scrubbed
            if let (true, Some(pointer_x)) = (hovered(Control::Seek), state.pointer_x) {
                let last_frame = state.duration_in_frames.saturating_sub(1);
                let frame = (layout.progress_at(pointer_x) * last_frame as f32).round() as usize;
                self.scrub_chip(canvas, layout, pointer_x, &format_time(frame, state.fps));
            }
        }
    }

    fn scrub_chip(&self, canvas: &Canvas, layout: &ControlsLayout, x: f32, label: &str) {
        let scale = layout.scale;
        let font = self.font_sized(true, CHIP_FONT_SIZE * scale);
        let width = tabular_width(&font, label) + CHIP_PADDING_X * scale * 2.;
        let height = CHIP_HEIGHT * scale;
        let x = x.clamp(layout.track.left, layout.track.right);
        let left = (x - width / 2.).clamp(layout.bar.left, layout.bar.right - width);
        let top = layout.bar.top - CHIP_GAP * scale - height;
        let chip = Rect::from_xywh(left, top, width, height);
        let rounded = RRect::new_rect_xy(chip, 6. * scale, 6. * scale);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(SHADOW);
        paint.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, 6. * scale, false));
        canvas.draw_rrect(rounded.with_offset((0., 3. * scale)), &paint);
        paint.set_mask_filter(None);
        paint.set_color(CHIP);
        canvas.draw_rrect(rounded, &paint);
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(scale.max(1.));
        paint.set_color(HAIRLINE);
        canvas.draw_rrect(rounded.with_inset((scale / 2., scale / 2.)), &paint);

        let mut text = Paint::default();
        text.set_anti_alias(true);
        text.set_color(TEXT);
        let baseline = chip.center_y() + font.metrics().1.cap_height / 2.;
        draw_tabular(
            canvas,
            label,
            Point::new(left + CHIP_PADDING_X * scale, baseline),
            &font,
            &text,
        );
    }

    fn ghost_button(
        &self,
        canvas: &Canvas,
        rect: Rect,
        icon: &Path,
        hovered: bool,
        pressed: bool,
        scale: f32,
    ) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        let background = match (hovered, pressed) {
            (true, true) => Some(GHOST_PRESSED_HOVER),
            (false, true) => Some(GHOST_PRESSED),
            (true, false) => Some(GHOST_HOVER),
            (false, false) => None,
        };
        if let Some(background) = background {
            paint.set_color(background);
            canvas.draw_circle(rect.center(), rect.width() / 2., &paint);
        }

        paint.set_color(if hovered || pressed {
            TEXT
        } else {
            TEXT_SECONDARY
        });
        draw_icon(canvas, icon, rect, scale, &paint);
    }

    fn primary_button(&self, canvas: &Canvas, rect: Rect, icon: &Path, hovered: bool, scale: f32) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(if hovered { BRAND_HOVER } else { BRAND });
        canvas.draw_circle(rect.center(), rect.width() / 2., &paint);

        paint.set_color(TEXT_ON_BRAND);
        draw_icon(canvas, icon, rect, scale, &paint);
    }
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

fn tabular_width(font: &Font, text: &str) -> f32 {
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

fn draw_tabular(canvas: &Canvas, text: &str, origin: Point, font: &Font, paint: &Paint) {
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

    #[test]
    fn draws_the_primary_action_and_progress_in_the_brand_color() {
        let controls = Controls::new();
        let (width, height) = (960, 200);
        let layout = ControlsLayout::new(&controls, "00:30", width as f32, height as f32, 1.);
        let mut surface = surfaces::raster_n32_premul((width, height)).expect("raster surface");
        let state = ControlsState {
            visible: true,
            frame: 450,
            duration_in_frames: 900,
            fps: 30,
            playing: false,
            looping: true,
            fullscreen: false,
            hovered: None,
            pointer_x: None,
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
