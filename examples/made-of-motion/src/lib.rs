//! A 489-frame analog / pixel collage film for fframes. Authored at 24 fps for
//! native preview; the documented `FFmpeg` export restores the source's 24000/1001
//! timestamps and copies its AAC packets without remixing or re-encoding them.
mod flight;
mod hero;
mod ink;
mod ink_capture;
mod objects;
mod opening;
mod opening_capture;
mod restoration_capture;
mod vector_ink;

use std::sync::Arc;

use fframes::{
    AudioMap, AudioTimestamp, AudioTrack, Color, Duration, FFramesContext, FFramesSyncedVideoFrame,
    Frame, Scene, Scenes, Shader, ShaderUniforms, Svgr, SyncVideoFrameInput, Transform, Video,
    animation::Easing, include_media_dir,
};

include_media_dir!(pub struct MotionMedia, "examples/made-of-motion/media");

const QUESTION: &str = "how do you turn a few lines of code into a feeling?";

#[derive(Clone, Copy, Debug)]
enum Kind {
    Question,
    Signal,
    Portrait,
    Thermal,
    Ring,
    Code,
    Carousel,
    Motion,
    Feeling,
    Scatter,
    Hand,
    Pulse(usize),
    Signature,
}

// Source frame boundaries, end exclusive. Keep these aligned to the music edit.
const EDIT: [(&str, usize, Kind); 18] = [
    ("Question", 48, Kind::Question),
    ("Signal", 93, Kind::Signal),
    ("Portrait", 153, Kind::Portrait),
    ("ThermalCut", 157, Kind::Thermal),
    ("Orbit", 200, Kind::Ring),
    ("Code", 220, Kind::Code),
    ("CarouselA", 237, Kind::Carousel),
    ("Motion", 253, Kind::Motion),
    ("CarouselB", 265, Kind::Carousel),
    ("Feeling", 286, Kind::Feeling),
    ("OrbitReturn", 302, Kind::Ring),
    ("Scatter", 331, Kind::Scatter),
    ("Human", 382, Kind::Hand),
    ("M", 389, Kind::Pulse(0)),
    ("O", 397, Kind::Pulse(1)),
    ("V", 403, Kind::Pulse(2)),
    ("E", 413, Kind::Pulse(3)),
    ("Fframes", 489, Kind::Signature),
];

#[derive(Debug)]
struct Studio {
    paper: Shader,
    signal: Shader,
    ink: ink::Ink,
    print: Shader,
    object: Shader,
    performance: Shader,
    hand: Shader,
    thermal_cut: Shader,
    objects: Vec<Vec<objects::Object>>,
    flight: flight::Flight,
    heroes: hero::Heroes,
}

impl Studio {
    fn background(&self, frame: &Frame, mode: f32, vignette: f32) -> Svgr<'static> {
        let layer = self.paper.draw(
            frame,
            ShaderUniforms::new()
                .float("uMode", mode)
                .float("uVignette", vignette),
        );
        fframes::svgr!(<image href={layer.href()} width="1440" height="1080" />)
    }

    fn signal(&self, frame: &Frame) -> Svgr<'static> {
        let pose = match frame.index {
            0 => [1240., 540., 170., 670., 0., 0.42],
            1 => [550., 540., 680., 660., 0.17, 0.42],
            2 => [350., 540., 740., 950., -0.68, 0.38],
            n => opening_capture::STAR[(n - 3).min(37)],
        };
        let layer = self.signal.draw(
            frame,
            ShaderUniforms::new()
                .float4("uPose", pose[0], pose[1], pose[2], pose[3])
                .float("uAngle", pose[4])
                .float("uPower", pose[5]),
        );
        fframes::svgr!(<image href={layer.href()} width="1440" height="1080" />)
    }

    fn print(&self, frame: &Frame, strength: f32) -> Svgr<'static> {
        let layer = self
            .print
            .draw(frame, ShaderUniforms::new().float("uStrength", strength));
        fframes::svgr!(<image href={layer.href()} width="1440" height="1080" />)
    }

    fn objects(&self, frame: &Frame, ctx: &FFramesContext<'_, '_>) -> Svgr<'static> {
        let group = self.objects[frame.global_index.min(488)]
            .iter()
            .map(|o| o.draw(frame, ctx, &self.object))
            .collect::<Vec<_>>();
        fframes::svgr!(<g>{group}</g>)
    }

    fn performance(
        &self,
        frame: &Frame,
        ctx: &FFramesContext<'_, '_>,
        file: &str,
    ) -> Svgr<'static> {
        let Some(video) = frame.get_synced_video_frame(ctx, file, &SyncVideoFrameInput::default())
        else {
            return self.background(frame, 1., 0.);
        };
        let source = video.into_image();
        let layer = self
            .performance
            .draw(frame, ShaderUniforms::new().image("uSource", &source));
        fframes::svgr!(<image href={layer.href()} width="1440" height="1080" />)
    }

    fn hand(&self, frame: &Frame, ctx: &FFramesContext<'_, '_>) -> Svgr<'static> {
        let Some(field) = ctx.get_image("hand-field.png") else {
            return self.background(frame, 1., 0.);
        };
        let [ax, ay, x, y, w, h] = restoration_capture::HAND[frame.index.min(50)];
        let layer = self.hand.draw(
            frame,
            ShaderUniforms::new()
                .image("uField", field)
                .float2("uAtlas", ax, ay)
                .float4("uRect", x, y, w, h),
        );
        fframes::svgr!(<image href={layer.href()} width="1440" height="1080" />)
    }

    fn thermal_cut(&self, frame: &Frame, ctx: &FFramesContext<'_, '_>) -> Svgr<'static> {
        let layer = self.thermal_cut.draw(frame, ShaderUniforms::new());
        fframes::svgr!(<g>
            <image href={layer.href()} width="1440" height="1080" />
            <g opacity={frame.animate(fframes::timeline!(
                at 0.0 => 0.125, animate 0.0_f32 => 0.94, Easing::EaseOut,
            ))}>{self.objects(frame,ctx)}</g>
        </g>)
    }
}

#[derive(Debug)]
struct Shot {
    name: &'static str,
    length: usize,
    kind: Kind,
    studio: Arc<Studio>,
}

impl Scene for Shot {
    fn name(&self) -> &'static str {
        self.name
    }
    fn duration(&self) -> Duration<'_> {
        Duration::Frames(self.length)
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let s = &self.studio;
        let content = match self.kind {
            Kind::Question => {
                fframes::svgr!(<g>{s.background(&frame,0.,0.)}
                    {if frame.index==0 {Svgr::empty()} else {opening::draw(&mut frame,ctx)}}
                    {s.ink.draw_question(&frame)}</g>)
            }
            Kind::Signal => {
                let pulse = frame.index == 0;
                let color = if pulse { "#f4eee0" } else { "#161616" };
                fframes::svgr!(<g>
                    {s.signal(&frame)}
                    <g opacity={frame.animate(fframes::timeline!(at 1.25 => 1.375, animate 1.0_f32 => 0.0, Easing::EaseIn))}>
                        {s.objects(&frame,ctx)}
                    </g>
                    {s.ink.draw(&frame, ctx)}
                    <text x="720" y="554" font-family="Inter 24pt" font-weight="700" font-size="54"
                        letter-spacing="-1.25" fill={color} text-anchor="middle">
                        "how do "<tspan fill={if frame.index>=39 {"#fff4df"} else {color}}>"you"</tspan>
                        " turn a few lines of code into a feeling?"
                    </text>
                </g>)
            }
            Kind::Portrait => {
                let copy = match frame.index {
                    0..=34 => "you don’t.",
                    35..=38 => "you give",
                    39..=42 => "you give it",
                    _ => "you give it fframes.",
                };
                let color = if frame.index >= 51 {
                    "#181714"
                } else {
                    "#f3eee1"
                };
                fframes::svgr!(<g>{s.performance(&frame,ctx,"portrait-clean.mp4")}
                    {s.ink.draw(&frame, ctx)}
                    {label(copy,208.,554.,46.,color,false)}
                </g>)
            }
            Kind::Thermal => s.thermal_cut(&frame, ctx),
            Kind::Ring | Kind::Carousel | Kind::Scatter => {
                let scatter = matches!(self.kind, Kind::Scatter);
                fframes::svgr!(<g>
                    {s.background(&frame,0.,0.08)}
                    {s.objects(&frame,ctx)}
                    {s.ink.impacts(&frame)}
                    {s.ink.draw(&frame, ctx)}
                    {if scatter && frame.index>23 { label("from your mind",720.,552.,32.,"#1d1916",true) } else { Svgr::empty() }}
                </g>)
            }
            Kind::Code => s.heroes.draw(&frame, ctx, s, hero::Card::Code),
            Kind::Feeling => s.heroes.draw(&frame, ctx, s, hero::Card::Feeling),
            Kind::Motion => s.heroes.draw(&frame, ctx, s, hero::Card::Motion),
            Kind::Hand => {
                let performance = if frame.index == 0 {
                    let stack = (0..12)
                        .map(|i| {
                            let x = 720. + (i as f32 * 0.55).cos() * 130.;
                            objects::hero(i, x, 150. + i as f32 * 65., 170., i as f32 * 0.13)
                                .draw(&frame, ctx, &s.object)
                        })
                        .collect::<Vec<_>>();
                    fframes::svgr!(<g>{s.background(&frame,1.,0.)}
                        <ellipse cx="720" cy="535" rx="76" ry="184" fill="url(#stack-light)"/>{stack}
                    </g>)
                } else {
                    s.hand(&frame, ctx)
                };
                let copy = match frame.index {
                    0..=2 => "from",
                    3..=5 => "from your",
                    _ => "from your mind",
                };
                fframes::svgr!(<g>{performance}{s.ink.draw(&frame,ctx)}
                    {label(copy,208.,554.,42.,"#f1ecdf",false)}
                    <g opacity={frame.animate(fframes::timeline!(at 0.416_667 => 0.625, animate 0.0_f32 => 1.0, Easing::EaseOut))}>
                        {label("to every frame.",1050.,554.,42.,"#f1ecdf",false)}
                    </g>
                </g>)
            }
            Kind::Pulse(index) => pulse(&frame, ctx, s, index),
            Kind::Signature => signature(&frame, s),
        };
        let live = matches!(self.kind, Kind::Portrait | Kind::Thermal | Kind::Hand)
            || (matches!(self.kind, Kind::Question) && frame.index == 0);
        let light = matches!(self.kind, Kind::Code | Kind::Motion | Kind::Feeling);
        fframes::svgr!(<g>
            {content}
            {if live { Svgr::empty() } else { ink::dust(&frame,light) }}
            {if live { Svgr::empty() } else { s.print(&frame,0.045) }}
        </g>)
    }
}

fn label(copy: &str, x: f32, y: f32, size: f32, color: &str, center: bool) -> Svgr<'static> {
    let anchor = if center { "middle" } else { "start" };
    fframes::svgr!(<text x={x} y={y} font-family="Inter 24pt" font-weight="700" font-size={size}
        letter-spacing="-1.25" fill={color.to_owned()} text-anchor={anchor}>{copy.to_owned()}</text>)
}

fn pulse(frame: &Frame, ctx: &FFramesContext<'_, '_>, s: &Studio, index: usize) -> Svgr<'static> {
    let mode = match index {
        1 => 1.,
        3 if frame.index < 3 => 1.,
        2 => 2.,
        _ => 0.,
    };
    let color = if index == 0 { "#27221c" } else { "#ede9df" };
    let icons = [3, 0, 7, 4];
    let object = objects::hero(
        icons[index],
        720.,
        540.,
        370.,
        frame.global_index as f32 * 0.13,
    )
    .draw(frame, ctx, &s.object);
    let letters = ["M", "O", "V", "E"]
        .iter()
        .enumerate()
        .take(index + 1)
        .map(|(i, ch)| label(ch, 250. + i as f32 * 312., 552., 32., color, true))
        .collect::<Vec<_>>();
    fframes::svgr!(<g>{s.background(frame,mode,if index==3 {1.35} else {0.5})}{object}{letters}</g>)
}

fn signature(frame: &Frame, s: &Studio) -> Svgr<'static> {
    let poses = s.flight.letters[frame.index.min(75)];
    let letters = ["f", "f", "r", "a", "m", "e", "s"];
    let orbit=letters.into_iter().zip(poses).map(|(ch,pose)| {
        let [x,y]=pose.position;
        let angle=pose.angle;
        let scale=pose.scale;
        fframes::svgr!(<g transform={format!("translate({x} {y}) rotate({angle}) scale({scale})")}>
            <text x="0" y="0" text-anchor="middle" font-family="Instrument Serif" font-style="italic"
                  font-size="54" fill="#35221b" stroke="#35221b" stroke-width="0.35">{ch}</text>
        </g>)
    }).collect::<Vec<_>>();
    fframes::svgr!(<g>
        {s.background(frame,0.,0.025)}
        {s.ink.draw_signature(frame, &poses)}
        <g opacity={frame.animate(fframes::timeline!(at 2.625 => 2.666_667, animate 1.0_f32 => 0.0, Easing::EaseOut))}>{orbit}</g>
        <g opacity={frame.animate(fframes::timeline!(at 2.625 => 2.666_667, animate 0.0_f32 => 1.0, Easing::EaseOut))}>
            <g transform="translate(720 540)">
                <g transform={frame.animate(fframes::timeline!(
                    at 2.666_667 => 2.833_333,
                    animate Transform::scale(2.8) => Transform::scale(1.),
                    Easing::CubicBezier(0.12,0.92,0.20,1.),
                ))}>
                    <text x="0" y="39" text-anchor="middle" font-family="Instrument Serif" font-style="italic"
                          font-size="194" letter-spacing="-5" fill="#24201b">"fframes"</text>
                </g>
            </g>
            <g opacity={frame.animate(fframes::timeline!(at 2.833_333 => 3.083_333, animate 0.0_f32 => 1.0, Easing::EaseOut))}
                transform={frame.animate(fframes::timeline!(at 2.833_333 => 3.083_333, animate Transform::translate(0,12) => Transform::translate(0,0), Easing::EaseOut))}>
                <text x="720" y="643" text-anchor="middle" font-family="JetBrains Mono" font-size="19"
                      letter-spacing="1" fill="#4b4136">"video, written in code."</text>
                <circle cx="720" cy="703" r="4.5" fill="#eb6c35"/>
            </g>
        </g>
    </g>)
}

/// The complete source-timed film, ready for Metal/Vulkan native preview.
#[derive(Debug)]
pub struct MadeOfMotion {
    shots: Vec<Shot>,
    ink_only: bool,
}

impl MadeOfMotion {
    pub fn new() -> Self {
        let (mut objects, impacts) = objects::choreography();
        let flight = flight::Flight::new();
        let ink = ink::Ink::new(&impacts);
        let heroes = hero::Heroes::new(&mut objects);
        let studio = Arc::new(Studio {
            paper: Shader::sksl(include_str!("shaders/paper.sksl")),
            signal: Shader::sksl(include_str!("shaders/signal.sksl")),
            ink,
            print: Shader::sksl(include_str!("shaders/print.sksl")),
            object: Shader::sksl(include_str!("shaders/object.sksl")),
            performance: Shader::sksl(include_str!("shaders/performance.sksl")),
            hand: Shader::sksl(include_str!("shaders/hand.sksl")),
            thermal_cut: Shader::sksl(include_str!("shaders/thermal_cut.sksl")),
            objects,
            flight,
            heroes,
        });
        let mut start = 0;
        let shots = EDIT
            .into_iter()
            .map(|(name, end, kind)| {
                let length = end - start;
                start = end;
                Shot {
                    name,
                    length,
                    kind,
                    studio: Arc::clone(&studio),
                }
            })
            .collect();
        Self {
            shots,
            ink_only: false,
        }
    }

    /// Show the source-timed vector exposures on neutral paper for inspection.
    pub fn with_ink_only(mut self, enabled: bool) -> Self {
        self.ink_only = enabled;
        self
    }
}

impl Default for MadeOfMotion {
    fn default() -> Self {
        Self::new()
    }
}

impl Video for MadeOfMotion {
    const FPS: usize = 24;
    const WIDTH: usize = 1440;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK;
    fn duration(&self) -> Duration<'_> {
        Duration::Frames(489)
    }
    fn define_scenes(&self) -> Scenes<'_> {
        Scenes::from(
            self.shots
                .iter()
                .map(|s| s as &dyn Scene)
                .collect::<Vec<_>>(),
        )
    }
    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([AudioTrack::new(
            "soundtrack.wav",
            AudioTimestamp::Second(0.)..AudioTimestamp::Eof,
        )])
    }
    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        fframes::svgr!(<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="1080" viewBox="0 0 1440 1080">
            <defs>
            <radialGradient id="stack-light"><stop offset="0" stop-color="#e8e8df"/>
                <stop offset="1" stop-color="#e8e8df" stop-opacity="0"/>
            </radialGradient></defs>
            {if self.ink_only {
                fframes::svgr!(<g><rect width="1440" height="1080" fill="#e4e4e4"/>
                    {self.shots[0].studio.ink.draw(&frame, ctx)}</g>)
            } else { ctx.render_scenes(&frame) }}
        </svg>)
    }
}
