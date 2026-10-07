//! Frame-exact vector ink plus procedural collision illumination.
use crate::{objects::Impact, vector_ink::VectorInk};
use fframes::{FFramesContext, Frame, Shader, ShaderUniforms, Svgr, Transform, animation::Easing};

pub(super) fn dust(frame: &Frame, light: bool) -> Svgr<'static> {
    let t = frame.global_index as f32 / 24.;
    let fill = if light { "#ded7c9" } else { "#2c2825" };
    let particles = (0..10)
        .map(|i| {
            let p = i as f32;
            let x = 135. + (p * 431. + t * (11. + p)).rem_euclid(1170.);
            let y = 110. + (p * 197. - t * (5. + p * 2.)).rem_euclid(860.);
            let r = if i % 3 == 0 { 1.7 } else { 0.8 };
            fframes::svgr!(<circle cx={x} cy={y} r={r} fill={fill} opacity="0.32" />)
        })
        .collect::<Vec<_>>();
    fframes::svgr!(<g>{particles}</g>)
}

#[derive(Debug)]
pub(super) struct Ink {
    impact_shader: Shader,
    impacts: Vec<Impact>,
    vectors: VectorInk,
}

impl Ink {
    pub(super) fn new(impacts: &[Impact]) -> Self {
        Self {
            impact_shader: Shader::sksl(include_str!("shaders/impact.sksl")),
            impacts: impacts.to_vec(),
            vectors: VectorInk::new(),
        }
    }

    pub(super) fn impacts(&self, frame: &Frame) -> Svgr<'static> {
        let bursts=self.impacts.iter().filter_map(|hit| {
            let age=frame.global_index as f32-hit.at as f32;
            if !(0.0..7.0).contains(&age) { return None; }
            let layer=self.impact_shader.draw(frame,ShaderUniforms::new()
                .float("uAge",age).float("uSeed",hit.at as f32)
                .float2("uDirection",hit.direction[0],hit.direction[1]));
            let [x,y]=hit.position;
            Some(fframes::svgr!(<image href={layer.href()} x={x-170.} y={y-170.} width="340" height="340"/>))
        }).collect::<Vec<_>>();
        fframes::svgr!(<g>{bursts}</g>)
    }

    pub(super) fn draw(&self, frame: &Frame, _ctx: &FFramesContext<'_, '_>) -> Svgr<'static> {
        self.vectors.draw(frame)
    }

    /// Retarget the reference's "you're" loop to "code" in the opening copy.
    /// The fixed font places their word centres at x=793 and x=900 respectively;
    /// code is 7/8 as wide. Ease into that mapping before the loop closes, then
    /// release it with the original pen's exit. Ink inspection keeps source positions.
    pub(super) fn draw_question(&self, frame: &Frame) -> Svgr<'static> {
        fframes::svgr!(<g transform={frame.animate(fframes::timeline!(
            at 1.125 => 1.291_667,
                animate Transform::default() => Transform { translate_x: 206.125, translate_y: -3., scale: (0.875, 1.).into(), ..Default::default() },
                Easing::EaseInOut,
            at 1.458_333 => 1.541_667,
                animate Transform { translate_x: 206.125, translate_y: -3., scale: (0.875, 1.).into(), ..Default::default() } => Transform::default(),
                Easing::EaseIn,
        ))}>
            {self.vectors.draw(frame)}
        </g>)
    }

    pub(super) fn draw_signature(
        &self,
        frame: &Frame,
        letters: &[crate::flight::Letter; 7],
    ) -> Svgr<'static> {
        self.vectors.draw_signature(frame, letters)
    }
}
