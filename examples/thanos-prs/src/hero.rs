use fframes::media::ImageData;
use fframes::{FFramesContext, Frame, Shader, ShaderUniforms, Svgr, svgr};

use crate::motion::*;

const WIDTH: f32 = 633.0;
const HEIGHT: f32 = 1165.0;
const TOP: f32 = -795.0;

#[derive(Debug)]
pub struct Pose {
    x: f32,
    y: f32,
    scale: f32,
    angle: f32,
    cock: f32,
    flick: f32,
    charge: f32,
    light: f32,
}

impl Pose {
    pub fn at(t: f32) -> Self {
        let rise = ease_out(phase(t, YES + 0.02, YES + 0.72));
        let cock = ramp(t, SNAP - 0.70, SNAP - 0.12);
        let flick = ramp(t, SNAP - 0.016, SNAP + 0.078);
        let recoil = flick * (1.0 - ramp(t, SNAP + 0.08, SNAP + 0.26));
        let exit = ramp(t, SNAP + 0.095, SNAP + 0.48);
        let charge = ramp(t, SNAP - 0.56, SNAP - 0.02) * (1.0 - ramp(t, SNAP, SNAP + 0.17));
        let light = if t >= SNAP {
            (-(t - SNAP) * 13.0).exp()
        } else {
            0.0
        };
        Self {
            x: 1260.0 + (1.0 - rise) * 598.0 - cock * 39.0 + recoil * 43.0 + exit * 592.0,
            y: 1102.0 + (1.0 - rise) * 558.0 - cock * 35.0 + recoil * 39.0 + exit * 358.0,
            scale: 1.0 + rise * 0.025 + cock * 0.035 + recoil * 0.04,
            angle: -18.5 + (1.0 - rise) * 25.0 - cock * 5.0 + recoil * 9.0 + exit * 22.0,
            cock,
            flick,
            charge,
            light,
        }
    }

    pub fn tip(&self) -> (f32, f32) {
        let x = WIDTH * (0.757 - 0.5);
        let y = TOP + HEIGHT * 0.105;
        let (sine, cosine) = self.angle.to_radians().sin_cos();
        (
            self.x + (x * cosine - y * sine) * self.scale,
            self.y + (x * sine + y * cosine) * self.scale,
        )
    }

    fn blur(&self, t: f32) -> (f32, f32) {
        let before = Self::at(t - 0.006);
        let after = Self::at(t + 0.006);
        let (sine, cosine) = self.angle.to_radians().sin_cos();
        let x = after.x - before.x;
        let y = after.y - before.y;
        (
            ((x * cosine + y * sine) / WIDTH).clamp(-0.048, 0.048),
            ((-x * sine + y * cosine) / HEIGHT).clamp(-0.048, 0.048),
        )
    }
}

pub fn gauntlet(
    frame: &Frame,
    ctx: &FFramesContext<'_, '_>,
    material: &Shader,
    charge_effect: &Shader,
    pose: &Pose,
) -> Svgr<'static> {
    let t = frame.seconds();
    let opacity = ramp(t, YES + 0.025, YES + 0.27) * (1.0 - ramp(t, SNAP + 0.21, SNAP + 0.46));
    if opacity <= 0.0 {
        return Svgr::empty();
    }
    let Some(art) = ctx.get_image("gauntlet.png") else {
        return Svgr::empty();
    };
    let (blur_x, blur_y) = pose.blur(t);
    let layer = material.draw(
        frame,
        ShaderUniforms::new()
            .float2("uImageSize", 451.0, 830.0)
            .float("uCock", pose.cock)
            .float("uFlick", pose.flick)
            .float("uCharge", pose.charge)
            .float("uFlash", pose.light)
            .float2("uBlur", blur_x, blur_y)
            .image("uImage", art),
    );
    let transform = format!(
        "translate({} {}) rotate({}) scale({})",
        pose.x, pose.y, pose.angle, pose.scale
    );
    let (tip_x, tip_y) = pose.tip();
    let sparks = charge_effect.draw(
        frame,
        ShaderUniforms::new()
            .float("uCharge", pose.charge * 0.45)
            .float("uSnap", pose.light)
            .float2("uTip", 150.0, 150.0),
    );
    svgr!(
        <g opacity={opacity}>
            <rect width="1440" height="1280" fill="#0a090c" opacity="0.32"/>
            <ellipse cx={tip_x} cy={tip_y + 201.0} rx="539" ry="480" fill="url(#gold-aura)"
                     opacity={0.58 + pose.charge * 0.32}/>
            <g transform={transform}>
                <image x={-WIDTH / 2.0} y={TOP} width={WIDTH} height={HEIGHT} href={layer.href()}/>
            </g>
            <image x={tip_x - 150.0} y={tip_y - 150.0} width="300" height="300" href={sparks.href()}/>
        </g>
    )
}

pub fn comments(ctx: &FFramesContext<'_, '_>, t: f32) -> Svgr<'static> {
    let age = t - COMMENTS;
    if age <= 0.0 {
        return Svgr::empty();
    }
    let Some(screenshot) = ctx.get_image("comment.png") else {
        return Svgr::empty();
    };
    let shade = ramp(age, 0.53, 1.08);
    let focus = ease_out(phase(age, COMMENT_FOCUS - 0.10, COMMENT_FOCUS + 0.45));
    let push = ramp(age, COMMENT_FOCUS + 0.31, COMMENT_FOCUS + 1.50);
    let background = (1..COMMENT_COUNT)
        .map(|i| {
            let seed = i as u32;
            let start = comment_entry(i);
            let enter = ease_out(phase(age, start, start + 0.36));
            if enter <= 0.0 {
                return Svgr::empty();
            }
            let left = i % 2 != 0;
            let side = if left { -1.0 } else { 1.0 };
            let row = (i - 1) / 2;
            let travel = ease_out(phase(age, start + 0.21, start + 1.55));
            let depth = 0.84 + enter * 0.19 + focus * 0.055;
            let width = (625.0 + 110.0 * random(seed * 41 + 8)) * depth;
            let x = if left { 25.0 } else { 755.0 };
            let x = x
                + 79.0 * random(seed + 14)
                + side * ((1.0 - enter) * 650.0 + focus * (90.0 + 23.0 * row as f32));
            let y = 108.0 + 104.0 * row as f32 + 39.0 * random(seed * 17)
                - (1.0 - enter) * (317.0 + 30.0 * row as f32)
                + travel * (108.0 + 40.0 * random(seed + 310));
            let alpha = enter * (0.92 - focus * 0.43);
            let tilt = (random(seed + 6) - 0.5) * 11.0 + side * (1.0 - enter) * 24.0;
            comment_card(screenshot, width, x, y, tilt, alpha)
        })
        .collect::<Svgr>();

    let first = ease_out(phase(age, 0.0, 0.29));
    let slide = 1.0 - first;
    let width = 792.0 + focus * 390.0;
    let x = 541.0 + 809.0 * slide - focus * 412.0;
    let y = 222.0 - slide * 40.0 + focus * 109.0;
    let image_width = width + (focus * 0.79 + push * 0.21) * 510.0;
    let height = image_width * 576.0 / 1686.0;
    let foreground = svgr!(
        <g transform={format!("translate({x} {y})")} opacity={first}>
            <defs>
                <clipPath id="focused-comment"><rect width={width} height={height} rx="11"/></clipPath>
            </defs>
            <rect x="-4" y="13" width={width + 8.0} height={height + 7.0} rx="15"
                  fill="#030406" opacity="0.78"/>
            <rect x="-1" y="-1" width={width + 2.0} height={height + 2.0} rx="12"
                  fill="#0e1218" stroke="#666776"/>
            <image width={image_width} height={height} href={screenshot.href()} clip-path="url(#focused-comment)"/>
        </g>
    );
    let credit = ramp(age, COMMENT_FOCUS + 0.46, COMMENT_FOCUS + 1.0);
    let mark = ctx.get_image("fframes-mark.png").map_or_else(
        Svgr::empty,
        |mark| svgr!(<image x="655" y="933" width="27" height="31" href={mark.href()}/>),
    );
    svgr!(
        <g>
            <rect width="1440" height="1280" fill="#09090c" opacity={shade * 0.87}/>
            {background}
            <ellipse cx="720" cy="607" rx="750" ry="429" fill="url(#comment-shadow)" opacity={focus * 0.77}/>
            {foreground}
            <g opacity={credit}>
                {mark}
                <text x="693" y="957" font-size="24" font-weight="700" fill="#dddde6">"fframes"</text>
            </g>
        </g>
    )
}

fn comment_card(
    screenshot: &ImageData<'_>,
    width: f32,
    x: f32,
    y: f32,
    tilt: f32,
    alpha: f32,
) -> Svgr<'static> {
    let height = width * 576.0 / 1686.0;
    svgr!(
        <g transform={format!("translate({x} {y}) rotate({tilt})")} opacity={alpha}>
            <rect x="-1" y="10" width={width + 2.0} height={height + 3.0} rx="15"
                  fill="#030406" opacity="0.75"/>
            <rect x="-1" y="-1" width={width + 2.0} height={height + 2.0} rx="11"
                  fill="#0e1218" stroke="#666776"/>
            <image width={width} height={height} href={screenshot.href()}/>
        </g>
    )
}
