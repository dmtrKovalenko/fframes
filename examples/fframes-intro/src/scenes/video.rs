//! Two podcast feeds decoded by ffmpeg, frame by frame. On the
//! last bar the green screen of the right feed is keyed out on the GPU and the
//! shader of the next scene shows through.

use fframes::{
    Color, Duration, FFramesContext, FFramesSyncedVideoFrame, Frame, Scene, ShaderUniforms, Svgr,
    SyncVideoFrameInput,
};

use crate::beat::*;
use crate::shaders::SHADERS;
use crate::ui::*;

beat_scene!(VideoScene, Some(66.0), Some(72.0));

pub const LEFT: (f32, f32, f32, f32) = (150.0, 300.0, 790.0, 444.0);
pub const RIGHT: (f32, f32, f32, f32) = (980.0, 300.0, 790.0, 444.0);

pub fn clip_input() -> SyncVideoFrameInput<'static> {
    SyncVideoFrameInput { start_from: 0.0, looping: true, editor_fallback_image: None }
}

/// The keyed guest from the right feed: footage (0) or halftone print (1).
pub fn keyed_guest(
    frame: &Frame,
    ctx: &FFramesContext,
    rect: (f32, f32, f32, f32),
    halftone: f32,
    alpha: f32,
) -> Svgr<'static> {
    let Some(video) = frame.get_synced_video_frame(ctx, "right_clip.mp4", &clip_input()) else {
        return Svgr::empty();
    };
    let image = video.into_image();
    let layer = SHADERS.key.draw(
        frame,
        ShaderUniforms::new()
            .image("iChannel0", &image)
            .float2("uImg", 1280.0, 720.0)
            .float("uHalftone", halftone)
            .float("uCell", 9.0)
            .float("uAlpha", alpha)
            .color("uInk", Color::hex(ORANGE))
            .color("uPaper", Color::hex(BONE)),
    );
    let (x, y, w, h) = rect;
    fframes::svgr!(<image href={layer.href()} x={x} y={y} width={w} height={h} />)
}

pub fn tunnel(frame: &Frame, speed: f32, glow: f32) -> Svgr<'static> {
    let layer = SHADERS.tunnel.draw(
        frame,
        ShaderUniforms::new()
            .float("uT", crate::beat::gsec(frame))
            .float("uSpeed", speed)
            .float("uGlow", glow)
            .color("uHot", Color::hex(ORANGE))
            .color("uBone", Color::hex(BONE)),
    );
    fframes::svgr!(<image href={layer.href()} x="0" y="0" width="1920" height="1080" />)
}

fn film(x: f32, y: f32, w: f32, h: f32) -> Svgr<'static> {
    let holes: Vec<Svgr> = (0..22)
        .flat_map(|i| {
            let hx = x + 10.0 + i as f32 * (w - 20.0) / 21.0;
            [
                fframes::svgr!(<rect x={hx} y={y - 26.0} width="14" height="10" rx="2" fill="#2d2a27" />),
                fframes::svgr!(<rect x={hx} y={y + h + 16.0} width="14" height="10" rx="2" fill="#2d2a27" />),
            ]
        })
        .collect();
    fframes::svgr!(<g>{holes}</g>)
}

impl Scene for VideoScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        let left = frame
            .get_synced_video_frame(ctx, "left_clip.mp4", &clip_input())
            .map(|f| f.into_image());
        let right = frame
            .get_synced_video_frame(ctx, "right_clip.mp4", &clip_input())
            .map(|f| f.into_image());
        let enter_l = expo_out(prog(lb, 0.0, 0.7));
        let enter_r = expo_out(prog(lb, 0.5, 1.2));
        let keyed = lb >= 4.0;
        let tc = timecode(frame.seconds() + 28.0);
        let tc2 = timecode(frame.seconds() + 246.0);

        let (lx, ly, lw, lh) = LEFT;
        let (rx, ry, rw, rh) = RIGHT;
        let left_img = left.map(|i| fframes::svgr!(<image href={i.href()} x={lx} y={ly} width={lw} height={lh} />)).unwrap_or_default();
        let right_img = if keyed {
            let flicker = if lb < 4.25 { (hash(lb * 211.0) > 0.5) as i32 as f32 } else { 1.0 };
            fframes::svgr!(
                <g>
                    <clipPath id="vid-right"><rect x={rx} y={ry} width={rw} height={rh} /></clipPath>
                    <g clip-path="url(#vid-right)">{tunnel(&frame, 3.0, 1.0)}</g>
                    <g opacity={flicker}>{keyed_guest(&frame, ctx, RIGHT, 0.0, 1.0)}</g>
                </g>
            )
        } else {
            right.map(|i| fframes::svgr!(<image href={i.href()} x={rx} y={ry} width={rw} height={rh} />)).unwrap_or_default()
        };
        let exit = expo_in(prog(lb, 5.75, 6.0));

        fframes::svgr!(
            <g>
                <g opacity={1.0 - exit}>
                    <text x="150" y="220" font-family={DISPLAY} font-size="120" letter-spacing="-4" fill={BONE}>"VIDEO"</text>
                    {label(154.0, 110.0, "03".to_owned(), ORANGE, 26.0, "start")}
                    <g opacity={enter_l} transform={format!("translate(0 {})", (1.0 - enter_l) * 80.0)}>
                        {film(lx, ly, lw, lh)}
                        {left_img}
                        {label(lx, ly + lh + 70.0, format!("LEFT.MP4  {tc}"), GREY, 18.0, "start")}
                    </g>
                    {label(1770.0, 220.0, "DECODED BY FFMPEG · IN SYNC TO THE FRAME".to_owned(), GREY, 18.0, "end")}
                </g>
                <g opacity={enter_r} transform={format!("translate(0 {})", (1.0 - enter_r) * 80.0)}>
                    {film(rx, ry, rw, rh)}
                    {right_img}
                    {label(rx, ry + rh + 70.0, format!("RIGHT.MP4  {tc2}"), GREY, 18.0, "start")}
                    {label(rx + rw, ry + rh + 70.0, if keyed { "CHROMA KEY · SKSL".to_owned() } else { String::new() }, ORANGE, 18.0, "end")}
                </g>
            </g>
        )
    }
}
