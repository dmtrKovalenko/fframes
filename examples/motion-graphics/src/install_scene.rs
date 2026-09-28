use fframes::{
    AudioMap, Color, FFramesContext, FontQuery, Frame, Svgr, Transform, animation::Easing,
};
use std::sync::OnceLock;

use crate::{MotionGraphicsMedia, SPRING_SNAPPY};

const SHELL_CMD: &str = "curl -L https://dmtrkovalenko.dev/install-fff-mcp.sh | bash";
const GITHUB_URL: &str = "https://github.com/dmtrKovalenko/fff.nvim";

/// Scene showing "install fff" title with a terminal box and GitHub link.
pub struct InstallSceneVideo<'a> {
    pub media: &'a MotionGraphicsMedia,
    resolved_cmd_font_size: OnceLock<usize>,
}

impl std::fmt::Debug for InstallSceneVideo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallSceneVideo").finish()
    }
}

impl<'a> InstallSceneVideo<'a> {
    pub fn new(media: &'a MotionGraphicsMedia) -> Self {
        Self {
            media,
            resolved_cmd_font_size: OnceLock::new(),
        }
    }
}

const MAX_CMD_FONT_SIZE: usize = 42;
const MIN_CMD_FONT_SIZE: usize = 16;

/// Largest monospace font size (down to [`MIN_CMD_FONT_SIZE`]) at which
/// `text` fits into `max_width`.  `None` when the font can not be measured
/// yet (fonts are resolved lazily in the editor), so the caller must not
/// cache the result.
fn resolve_monospace_font_size<'a>(
    frame: &mut Frame,
    ctx: &FFramesContext<'a, '_>,
    text: &'a str,
    max_width: usize,
) -> Option<usize> {
    let mut size = MAX_CMD_FONT_SIZE;
    loop {
        let query = FontQuery {
            family: "JetBrains Mono",
            size,
            weight: 400,
            ..Default::default()
        };
        match frame.text_width(ctx, query, text)? {
            w if w <= max_width => break Some(size),
            _ if size <= MIN_CMD_FONT_SIZE => break Some(MIN_CMD_FONT_SIZE),
            _ => size -= 2,
        }
    }
}

impl fframes::Video for InstallSceneVideo<'_> {
    const FPS: usize = 60;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Seconds(10.0)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        // Terminal box has ~80px padding on each side inside 1520px wide box.
        // The size is only cached once the font could actually be measured.
        let cmd_font_size = match self.resolved_cmd_font_size.get() {
            Some(size) => *size,
            None => match resolve_monospace_font_size(&mut frame, ctx, SHELL_CMD, 1360) {
                Some(size) => *self.resolved_cmd_font_size.get_or_init(|| size),
                None => MIN_CMD_FONT_SIZE,
            },
        };

        // === Title "INSTALL FFF" ===
        // Scale punch
        let title_scale = frame.animate(&fframes::timeline!(
            at 0.0 => 0.6, animate 1.5_f32 => 1.0, SPRING_SNAPPY,
        ));
        let title_opacity = frame.animate(&fframes::timeline!(
            at 0.0 => 0.3, animate 0.0_f32 => 1.0, Easing::EaseOut,
        ));
        // Title moves up to make room
        let title_y = frame.animate(&fframes::timeline!(
            at 0.0 => 0.5, animate 440.0_f32 => 440.0, Easing::Linear,
            at 0.5 => 1.0, animate 440.0_f32 => 260.0, Easing::EaseInOut,
        ));

        // === Terminal box ===
        let box_opacity = frame.animate(&fframes::timeline!(
            at 0.7 => 1.2, animate 0.0_f32 => 1.0, Easing::EaseOut,
        ));
        let box_slide_y = frame.animate(&fframes::timeline!(
            at 0.7 => 1.3, animate 40.0_f32 => 0.0, SPRING_SNAPPY,
        ));

        // Terminal box dimensions
        let box_w: f32 = 1520.0;
        let box_h: f32 = 160.0;
        let box_x: f32 = (1920.0 - box_w) / 2.0;
        let box_y: f32 = 420.0;
        let box_r: f32 = 16.0;

        // Prompt "$ " prefix
        let prompt_x = box_x + 50.0;
        let text_y = box_y + box_h / 2.0 + 6.0;

        // Typing animation: reveal characters over time
        let total_chars = SHELL_CMD.len() as f32;
        let typed_chars = frame.animate(&fframes::timeline!(
            at 1.0 => 2.2, animate 0.0_f32 => total_chars, Easing::Linear,
        ));
        let visible_cmd = &SHELL_CMD[..typed_chars as usize];

        // Cursor blink (visible during and after typing)
        let cursor_visible = frame.animate(&fframes::timeline!(
            at 1.0 => 1.1, animate 0.0_f32 => 1.0, Easing::Linear,
        ));
        // Blink cursor after typing is done using frame number
        let blink = if typed_chars >= total_chars {
            if (frame.index / 30).is_multiple_of(2) {
                1.0_f32
            } else {
                0.0
            }
        } else {
            cursor_visible
        };

        // === GitHub URL ===
        let url_opacity = frame.animate(&fframes::timeline!(
            at 2.5 => 3.0, animate 0.0_f32 => 0.7, Easing::EaseOut,
        ));
        let url_slide_y = frame.animate(&fframes::timeline!(
            at 2.5 => 3.0, animate 30.0_f32 => 0.0, SPRING_SNAPPY,
        ));

        // === Accent line below title ===
        let accent_width = frame.animate(&fframes::timeline!(
            at 0.4 => 1.0, animate 0.5_f32 => 600.0, Easing::EaseInOut,
        ));

        // === Fade out ===
        let fade_out = frame.animate(&fframes::timeline!(
            at 9.2 => 9.8, animate 1.0_f32 => 0.0, Easing::EaseIn,
        ));

        // Cursor x position (after visible text)
        let cursor_query = FontQuery {
            family: "JetBrains Mono",
            size: cmd_font_size,
            weight: 400,
            ..Default::default()
        };
        let prefix_width = frame.text_width(ctx, cursor_query, "$ ").unwrap_or(40) as f32;
        // The typed command starts right after the measured prefix so the
        // cursor below lines up with its end.
        let text_x = prompt_x + prefix_width;
        let typed_width = if visible_cmd.is_empty() {
            0.0
        } else {
            frame
                .text_width(ctx, cursor_query, visible_cmd)
                .unwrap_or(0) as f32
        };
        let cursor_x = prompt_x + prefix_width + typed_width + 2.0;

        fframes::svgr!(
            <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 1920 1080"
                width={Self::WIDTH}
                height={Self::HEIGHT}
            >
                <rect width={Self::WIDTH} height={Self::HEIGHT} fill="black" />

                <g opacity={fade_out}>
                    // "INSTALL FFF" title
                    <text
                        x="960"
                        y={title_y}
                        opacity={title_opacity}
                        font-family="Bebas Neue"
                        font-size="200"
                        fill="white"
                        text-anchor="middle"
                        dominant-baseline="central"
                        letter-spacing="16"
                        transform={Transform {
                            translate_x: 960.0 * (1.0 - title_scale as f64),
                            translate_y: title_y as f64 * (1.0 - title_scale as f64),
                            scale: fframes::Scale { x: title_scale as f64, y: title_scale as f64 },
                            ..Default::default()
                        }}
                    >
                        "INSTALL FFF"
                    </text>

                    // Accent line
                    <rect
                        x={960.0 - accent_width / 2.0}
                        y="380"
                        width={accent_width}
                        height="2"
                        fill="#ff8c00"
                        opacity="0.6"
                        rx="1"
                    />

                    // Terminal box
                    <g
                        opacity={box_opacity}
                        transform={Transform {
                            translate_y: box_slide_y as f64,
                            ..Default::default()
                        }}
                    >
                        // Box background
                        <rect
                            x={box_x}
                            y={box_y}
                            width={box_w}
                            height={box_h}
                            rx={box_r}
                            ry={box_r}
                            fill="#1a1a2e"
                            stroke="#553300"
                            stroke-width="1.5"
                        />

                        // Terminal dots
                        <circle cx={box_x + 32.0} cy={box_y + 26.0} r="9" fill="#ff5f57" />
                        <circle cx={box_x + 58.0} cy={box_y + 26.0} r="9" fill="#febc2e" />
                        <circle cx={box_x + 84.0} cy={box_y + 26.0} r="9" fill="#28c840" />

                        // "$ " prompt
                        <text
                            x={prompt_x}
                            y={text_y}
                            font-family="JetBrains Mono"
                            font-size={cmd_font_size}
                            fill="#ff8c00"
                            dominant-baseline="central"
                        >
                            "$ "
                        </text>

                        // Command text (typed)
                        <text
                            x={text_x}
                            y={text_y}
                            font-family="JetBrains Mono"
                            font-size={cmd_font_size}
                            fill="#e0e0ff"
                            dominant-baseline="central"
                        >
                            {visible_cmd}
                        </text>

                        // Cursor
                        <rect
                            x={cursor_x}
                            y={text_y - cmd_font_size as f32 * 0.45}
                            width={cmd_font_size as f32 * 0.55}
                            height={cmd_font_size as f32 * 0.9}
                            fill="#e0e0ff"
                            opacity={blink}
                        />
                    </g>

                    // GitHub URL
                    <text
                        x="960"
                        y={680.0 + url_slide_y}
                        opacity={url_opacity}
                        font-family="JetBrains Mono"
                        font-size="32"
                        fill="white"
                        text-anchor="middle"
                        letter-spacing="1"
                    >
                        {GITHUB_URL}
                    </text>
                </g>
            </svg>
        )
    }
}
