//! The SVG over the 3D image: letterbox, flashes, readouts, callouts on parts of the robot,
//! and the title.

use fframes::{Svgr, svgr};
use fframes_3d::math::{progress, smoothstep};
use fframes_3d::*;

use crate::robot::{Bone, SCALE};
use crate::story::*;
use crate::{Shot, Show};

const DISPLAY: &str = "Anton";
const MONO: &str = "IBM Plex Mono";
const CYAN: &str = "#22e8ff";
const MAGENTA: &str = "#ff2bd6";
const BONE: &str = "#eef3ff";

/// The first part of `text`, for `amount` from 0 to 1.
fn typed(text: &str, amount: f32) -> String {
    let count = (text.chars().count() as f32 * amount.clamp(0.0, 1.0)).round() as usize;
    text.chars().take(count).collect()
}

/// A dot on a part of the robot, a leader line and a label that types out.
fn callout(
    anchor: Vec3,
    label: &'static str,
    value: &'static str,
    appear: f32,
    right: bool,
) -> Svgr<'static> {
    let line = smoothstep(0.0, 0.35, appear);
    let text = progress(appear, 0.3, 0.9);
    let dir = if right { 1.0 } else { -1.0 };
    let elbow = vec2(anchor.x + dir * 120.0, anchor.y - 90.0);
    let end = vec2(elbow.x + dir * 230.0, elbow.y);
    // The leader line grows out from the part.
    let first = (line * 2.0).min(1.0);
    let second = (line * 2.0 - 1.0).clamp(0.0, 1.0);
    let p1 = vec2(anchor.x, anchor.y).lerp(elbow, first);
    let p2 = elbow.lerp(end, second);
    let d = if second > 0.0 {
        format!(
            "M{} {} L{} {} L{} {}",
            anchor.x, anchor.y, elbow.x, elbow.y, p2.x, p2.y
        )
    } else {
        format!("M{} {} L{} {}", anchor.x, anchor.y, p1.x, p1.y)
    };
    // The labels are at the outer end of the leader line, away from the body.
    let anchor_text = if right { "end" } else { "start" };
    let tx = end.x;
    let pulse = 6.0 + 10.0 * (1.0 - line);
    svgr!(
        <g opacity={smoothstep(0.0, 0.1, appear)}>
            <circle cx={anchor.x} cy={anchor.y} r={pulse} fill="none" stroke={CYAN} stroke-width="2" opacity={0.8 - 0.6 * line} />
            <circle cx={anchor.x} cy={anchor.y} r="4.5" fill={CYAN} />
            <path d={d} fill="none" stroke={CYAN} stroke-width="2" />
            <text x={tx} y={elbow.y - 14.0} text-anchor={anchor_text} font-family={MONO} font-weight="600" font-size="19" letter-spacing="3" fill={CYAN}>{typed(label, text)}</text>
            <text x={tx} y={elbow.y + 34.0} text-anchor={anchor_text} font-family={DISPLAY} font-size="34" letter-spacing="1" fill={BONE}>{typed(value, progress(text, 0.4, 1.0))}</text>
        </g>
    )
}

/// Crop marks around a rectangle centered on `c`.
fn brackets(c: Vec2, half: Vec2, arm: f32, opacity: f32) -> Svgr<'static> {
    let (x0, y0, x1, y1) = (c.x - half.x, c.y - half.y, c.x + half.x, c.y + half.y);
    let d = format!(
        "M{x0} {} V{y0} H{} M{} {y0} H{x1} V{} M{x1} {} V{y1} H{} M{} {y1} H{x0} V{}",
        y0 + arm,
        x0 + arm,
        x1 - arm,
        y0 + arm,
        y1 - arm,
        x1 - arm,
        x0 + arm,
        y1 - arm,
    );
    svgr!(<path d={d} fill="none" stroke={CYAN} stroke-width="3" opacity={opacity} />)
}

/// A small label with a large value below it.
fn readout(x: f32, y: f32, label: &str, value: String, opacity: f32) -> Svgr<'static> {
    let label = label.to_owned();
    svgr!(
        <g opacity={opacity}>
            <text x={x} y={y} font-family={MONO} font-weight="600" font-size="18" letter-spacing="4" fill={CYAN}>{label}</text>
            <text x={x} y={y + 46.0} font-family={DISPLAY} font-size="48" letter-spacing="2" fill={BONE}>{value}</text>
        </g>
    )
}

/// A bar of `count` segments. `fill` (0 to 1) sets the lit part.
fn segments(x: f32, y: f32, count: usize, fill: f32, color: &'static str) -> Svgr<'static> {
    let lit = (fill.clamp(0.0, 1.0) * count as f32).round() as usize;
    let cells: Vec<Svgr> = (0..count)
        .map(|i| {
            let opacity = if i < lit { 1.0 } else { 0.18 };
            svgr!(<rect x={x + i as f32 * 16.0} y={y} width="11" height="22" fill={color} opacity={opacity} />)
        })
        .collect();
    svgr!(<g>{cells}</g>)
}

/// The overlay of the frame at `t`.
pub fn overlay(t: f32, shot: &Shot, show: &Show) -> Svgr<'static> {
    let project = |id: NodeId, local: Vec3| -> Option<Vec3> {
        let world = shot.scene.local_to_world(id, local);
        shot.camera
            .project(world, 1920.0, 1080.0)
            .filter(|p| p.z > 0.0)
    };
    let robot = &show.robot;
    let mut layers: Vec<Svgr> = Vec::new();

    // The vehicle readout.
    if t < LAUNCH + 0.3 {
        let o = smoothstep(0.15, 0.45, t) * (1.0 - smoothstep(LAUNCH, LAUNCH + 0.3, t));
        let kmh = 101.0 + (t * 7.0).sin() * 1.5;
        layers.push(readout(
            96.0,
            150.0,
            "OD-7  //  VEHICLE MODE",
            format!("{kmh:.0} KM/H"),
            o,
        ));
        layers.push(segments(
            96.0,
            220.0,
            24,
            0.55 + 0.1 * (t * 5.0).sin(),
            CYAN,
        ));
    }

    // The transformation counter.
    if (LAUNCH..LAND + 0.4).contains(&t) {
        let o = smoothstep(LAUNCH, LAUNCH + 0.1, t) * (1.0 - smoothstep(LAND, LAND + 0.3, t));
        let m = shot.state.morph;
        layers.push(readout(
            96.0,
            860.0,
            "TRANSFORM SEQUENCE",
            format!("{:03.0}%", m * 100.0),
            o,
        ));
        layers.push(svgr!(<g opacity={o}>{segments(96.0, 930.0, 32, m, CYAN)}</g>));
        let engage = smoothstep(LAUNCH, LAUNCH + 0.05, t)
            * (1.0 - smoothstep(LAUNCH + 0.35, LAUNCH + 0.6, t));
        if engage > 0.0 {
            let spacing = 30.0 + 40.0 * progress(t, LAUNCH, LAUNCH + 0.6);
            layers.push(svgr!(
                <text x="960" y="560" text-anchor="middle" font-family={DISPLAY} font-size="150" letter-spacing={spacing} fill={BONE} opacity={engage * 0.9}>"ENGAGE"</text>
            ));
        }
    }

    // Callouts while the robot stands.
    if (HUD..AIM).contains(&t) {
        let fade = 1.0 - smoothstep(AIM - 0.25, AIM, t);
        let a = |i: f32| progress(t, HUD + 0.3 + i * 0.35, HUD + 1.2 + i * 0.35);
        if let Some(head) = project(robot.bone(Bone::Neck), vec3(0.0, 0.32, 0.0)) {
            let size = shot.camera.pixel_size(
                shot.scene
                    .local_to_world(robot.bone(Bone::Neck), vec3(0.0, 0.32, 0.0)),
                0.7 * SCALE,
                1080.0,
            );
            let grow = 1.0 + 0.6 * (1.0 - smoothstep(HUD, HUD + 0.3, t));
            layers.push(brackets(
                vec2(head.x, head.y),
                Vec2::splat(size * 0.5 * grow),
                size * 0.18,
                smoothstep(HUD, HUD + 0.15, t) * fade,
            ));
        }
        let mut items: Vec<Svgr> = Vec::new();
        let anchors = [
            (
                robot.bone(Bone::Chest),
                vec3(0.0, 0.08, 0.24),
                "ARC CORE",
                "9.6 GW",
                true,
            ),
            (
                robot.bone(Bone::ShoulderR),
                vec3(-0.25, 0.0, 0.0),
                "DRIVE UNITS",
                "4 x 840 KW",
                false,
            ),
            (
                robot.bone(Bone::ElbowL),
                vec3(0.15, -0.25, 0.1),
                "SERVOS",
                "112 ACTIVE",
                true,
            ),
            (
                robot.bone(Bone::AnkleR),
                vec3(0.0, -0.05, 0.3),
                "MASS",
                "4.2 TONNES",
                false,
            ),
        ];
        for (i, (bone, local, label, value, right)) in anchors.into_iter().enumerate() {
            if let Some(p) = project(bone, local) {
                items.push(callout(p, label, value, a(i as f32), right));
            }
        }
        layers.push(svgr!(<g opacity={fade}>{items}</g>));
        let o = smoothstep(HUD, HUD + 0.2, t) * fade;
        layers.push(readout(
            96.0,
            150.0,
            "UNIT OD-7  //  ROBOT MODE",
            "ONLINE".to_owned(),
            o,
        ));
    }

    // The cannon charge.
    if (CANNON..FIRE + 0.3).contains(&t) {
        let o = smoothstep(CANNON, CANNON + 0.2, t) * (1.0 - smoothstep(FIRE, FIRE + 0.2, t));
        let charge = progress(t, CHARGE, FIRE);
        layers.push(readout(
            1500.0,
            150.0,
            "PLASMA ARRAY",
            format!("{:03.0}%", charge * 100.0),
            o,
        ));
        layers.push(svgr!(<g opacity={o}>{segments(1500.0, 220.0, 20, charge, MAGENTA)}</g>));
    }

    // The title.
    if t >= TITLE - 0.05 {
        let x = progress(t, TITLE, TITLE + 0.9);
        let ease = 1.0 - (1.0 - x).powi(3);
        let spacing = 70.0 - 52.0 * ease;
        let split = 26.0 * (1.0 - ease) + 3.0 * (t * 37.0).sin().abs() * (1.0 - x);
        let o = smoothstep(TITLE - 0.05, TITLE + 0.1, t);
        let sub = smoothstep(TITLE + 0.6, TITLE + 1.0, t);
        let credit = smoothstep(TITLE + 0.9, TITLE + 1.3, t);
        layers.push(svgr!(
            <g opacity={o}>
                <rect x="0" y="0" width="1920" height="1080" fill="#03040a" opacity={0.55 * o} />
                <text x={960.0 - split} y="600" text-anchor="middle" font-family={DISPLAY} font-size="250" letter-spacing={spacing} fill={CYAN} opacity="0.75">"OVERDRIVE"</text>
                <text x={960.0 + split} y="600" text-anchor="middle" font-family={DISPLAY} font-size="250" letter-spacing={spacing} fill={MAGENTA} opacity="0.75">"OVERDRIVE"</text>
                <text x="960" y="600" text-anchor="middle" font-family={DISPLAY} font-size="250" letter-spacing={spacing} fill={BONE}>"OVERDRIVE"</text>
                <text x="960" y="690" text-anchor="middle" font-family={MONO} font-weight="600" font-size="24" letter-spacing="10" fill={CYAN} opacity={sub}>"3D SCENES IN FFRAMES"</text>
                <text x="960" y="980" text-anchor="middle" font-family={MONO} font-weight="600" font-size="16" letter-spacing="5" fill={BONE} opacity={credit * 0.7}>"fframes-3d  /  scene graph, PBR, shadows, bloom  /  CPU + Skia GPU"</text>
            </g>
        ));
    }

    // Letterbox and flashes.
    let bars = 62.0 * (1.0 - smoothstep(TITLE, TITLE + 0.4, t));
    let flash = (0.3 * impact(t, FIRE, 16.0) * smoothstep(FIRE - 0.01, FIRE, t))
        .max(0.6 * impact(t, LAND, 9.0) * smoothstep(LAND - 0.01, LAND, t))
        .max(0.8 * impact(t, TITLE, 5.0) * smoothstep(TITLE - 0.01, TITLE, t))
        .max(0.3 * impact(t, LAUNCH, 10.0) * smoothstep(LAUNCH - 0.01, LAUNCH, t));
    let fade_out = smoothstep(DURATION - 0.35, DURATION, t);

    svgr!(
        <g>
            <defs>
                <radialGradient id="vignette" cx="0.5" cy="0.5" r="0.75">
                    <stop offset="0.55" stop-color="#000000" stop-opacity="0" />
                    <stop offset="1" stop-color="#000000" stop-opacity="0.7" />
                </radialGradient>
            </defs>
            <rect x="0" y="0" width="1920" height="1080" fill="url(#vignette)" />
            {layers}
            <rect x="0" y="0" width="1920" height={bars.max(0.01)} fill="#000000" />
            <rect x="0" y={1080.0 - bars.max(0.01)} width="1920" height={bars.max(0.01)} fill="#000000" />
            <rect x="0" y="0" width="1920" height="1080" fill="#ffffff" opacity={flash} />
            <rect x="0" y="0" width="1920" height="1080" fill="#000000" opacity={fade_out} />
        </g>
    )
}
