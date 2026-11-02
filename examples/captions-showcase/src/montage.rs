//! A frame-exact edit. All footage is resolved and decoded by `FFrames` media loading.
use std::sync::LazyLock;

use fframes::{
    FFramesContext, FFramesSyncedVideoFrame, Frame, MediaProvider, Shader, ShaderUniforms, Svgr,
    SyncVideoFrameInput, svgr,
};

static FILM: LazyLock<Shader> =
    LazyLock::new(|| Shader::sksl(include_str!("shaders/mission-film.sksl")));

/// Timeline and source coordinates use the composition's 30 fps clock, [in, out).
struct Shot {
    timeline_in: usize,
    timeline_out: usize,
    media: &'static str,
    source_in: usize,
}

impl Shot {
    const fn new(
        timeline_in: usize,
        timeline_out: usize,
        media: &'static str,
        source_in: usize,
    ) -> Self {
        Self {
            timeline_in,
            timeline_out,
            media,
            source_in,
        }
    }
}

// Cuts land on spoken ideas. Repeated clips use distinct source intervals.
const SHOTS: [Shot; 16] = [
    Shot::new(0, 38, "apollo-launch.mp4", 80),
    Shot::new(38, 52, "apollo-ignition.mp4", 0),
    Shot::new(52, 87, "moon-walk.mp4", 0),
    Shot::new(87, 139, "shuttle-onboard.mp4", 0),
    Shot::new(139, 197, "shuttle-launch.mp4", 0),
    Shot::new(197, 224, "rocket-engines.mp4", 0),
    Shot::new(224, 260, "artemis-launch.mp4", 0),
    Shot::new(260, 333, "earth-orbit.mp4", 0),
    Shot::new(333, 375, "moon-ladder.mp4", 0),
    Shot::new(375, 425, "apollo-launch.mp4", 39),
    Shot::new(425, 472, "artemis-engines.mp4", 0),
    Shot::new(472, 531, "artemis-launch.mp4", 39),
    Shot::new(531, 593, "cosmos.mp4", 180),
    Shot::new(593, 675, "shuttle-launch.mp4", 60),
    Shot::new(675, 713, "moon-ladder.mp4", 70),
    Shot::new(713, 780, "earth-orbit.mp4", 111),
];

/// The lens follows the subject, not the image rectangle. Coordinates are in the
/// prepared source; zoom is relative to a full-frame cover fit. Archival footage
/// remains archival footage: these crops do not manufacture additional detail.
#[derive(Clone, Copy)]
struct Camera {
    zoom: [f32; 2],
    center: [[f32; 2]; 2],
    roll: [f32; 2],
    grade: f32,
}

impl Camera {
    const fn new(zoom: [f32; 2], center: [[f32; 2]; 2], roll: [f32; 2], grade: f32) -> Self {
        Self {
            zoom,
            center,
            roll,
            grade,
        }
    }
}

const CAMERAS: [Camera; 16] = [
    Camera::new([1.10, 1.07], [[0.53, 0.50], [0.525, 0.50]], [0.0, 0.0], 0.0),
    Camera::new([1.07, 1.12], [[0.50, 0.50], [0.495, 0.51]], [0.0, 0.0], 0.0),
    Camera::new([1.07, 1.10], [[0.50, 0.49], [0.505, 0.49]], [0.0, 0.0], 1.0),
    Camera::new(
        [1.11, 1.15],
        [[0.50, 0.50], [0.495, 0.495]],
        [0.0, 0.0],
        2.0,
    ),
    Camera::new([1.39, 1.47], [[0.53, 0.61], [0.535, 0.62]], [0.0, 0.0], 0.0),
    Camera::new([1.06, 1.10], [[0.50, 0.51], [0.505, 0.52]], [0.0, 0.0], 0.0),
    // The wide Artemis view keeps the entire source mark within the frame.
    Camera::new([1.04, 1.06], [[0.50, 0.50], [0.50, 0.50]], [0.0, 0.0], 2.0),
    Camera::new([1.08, 1.05], [[0.50, 0.50], [0.50, 0.50]], [0.0, 0.0], 3.0),
    Camera::new(
        [1.07, 1.11],
        [[0.50, 0.50], [0.505, 0.495]],
        [0.0, 0.0],
        1.0,
    ),
    Camera::new([1.10, 1.07], [[0.53, 0.485], [0.52, 0.49]], [0.0, 0.0], 0.0),
    // These closer views follow the engine bells and ascending vehicle. Their
    // crop bounds keep the corner mark wholly outside, never sliced at an edge.
    Camera::new([1.18, 1.25], [[0.50, 0.56], [0.50, 0.57]], [0.0, 0.0], 2.0),
    Camera::new([1.32, 1.38], [[0.57, 0.43], [0.57, 0.40]], [0.0, 0.0], 2.0),
    Camera::new([1.06, 1.10], [[0.50, 0.50], [0.50, 0.49]], [0.0, 0.0], 3.0),
    Camera::new([1.57, 1.65], [[0.54, 0.62], [0.54, 0.615]], [0.0, 0.0], 0.0),
    Camera::new([1.14, 1.09], [[0.53, 0.50], [0.52, 0.50]], [0.0, 0.0], 1.0),
    Camera::new([1.09, 1.055], [[0.50, 0.50], [0.50, 0.50]], [0.0, 0.0], 3.0),
];

/// Check required assets and source ranges before initializing the renderer.
pub fn validate<'a>(media: &'a impl MediaProvider<'a>, seconds: f32) -> Result<(), String> {
    if seconds > 26.0 {
        return Err(
            "this shot score covers 26 seconds; extend montage.rs for a longer track".into(),
        );
    }
    let mut previous_end = 0;
    for shot in &SHOTS {
        if shot.timeline_in != previous_end || shot.timeline_out <= shot.timeline_in {
            return Err(format!(
                "invalid montage interval at frame {}",
                shot.timeline_in
            ));
        }
        previous_end = shot.timeline_out;
        let source = media
            .resolve_video(shot.media)
            .ok_or_else(|| format!("montage source is missing: {}", shot.media))?;
        let duration = if let Some(metadata) = source.metadata {
            metadata.duration
        } else {
            // Runtime providers defer metadata to their decoder. Probe that same FFrames
            // media API here, before allocating a renderer or discovering EOF mid-export.
            // SAFETY: the decoder owns the initialized libav contexts and buffers; the
            // provider's path lives through this call, and no raw pointers escape it.
            let decoder = unsafe { fframes::media::FFmpegDecoder::new(&source.path, 30, 1) }
                .map_err(|error| format!("cannot read {}: {error:?}", shot.media))?;
            decoder.get_raw_frame().stream_duration_in_seconds()
        };
        let source_out = shot.source_in + shot.timeline_out - shot.timeline_in;
        if source_out as f64 / 30.0 > f64::from(duration) + 0.001 {
            return Err(format!(
                "{} ends before required source frame {source_out}",
                shot.media
            ));
        }
    }
    Ok(())
}

pub fn render<'a>(frame: &Frame, ctx: &FFramesContext<'a, '_>, shaders: bool) -> Svgr<'a> {
    let Some((shot_index, shot)) = SHOTS.iter().enumerate().find(|(_, shot)| {
        shot.timeline_in <= frame.global_index && frame.global_index < shot.timeline_out
    }) else {
        return missing_frame("outside the montage", frame.global_index);
    };
    let age = (frame.global_index - shot.timeline_in) as f32 / 30.0;
    let progress = (frame.global_index - shot.timeline_in) as f32
        / (shot.timeline_out - shot.timeline_in) as f32;
    // A restrained linear drift keeps the documentary image level and measured.
    // The underlying launch or orbital footage supplies the primary movement.
    let camera = CAMERAS[shot_index];
    let lerp = |values: [f32; 2]| values[0] + (values[1] - values[0]) * progress;
    let zoom = lerp(camera.zoom);
    let center = [
        lerp([camera.center[0][0], camera.center[1][0]]),
        lerp([camera.center[0][1], camera.center[1][1]]),
    ];
    let roll = lerp(camera.roll);
    let mut source_frame = frame.clone();
    source_frame.index = shot.source_in + frame.global_index - shot.timeline_in;
    // `start_from` is a timeline delay, not source trim. The source index above is explicit.
    let Some(source) = source_frame.get_synced_video_frame(
        ctx,
        shot.media,
        &SyncVideoFrameInput {
            start_from: 0.0,
            looping: false,
            editor_fallback_image: None,
            speed: 1.0,
        },
    ) else {
        return missing_frame(shot.media, source_frame.index);
    };
    let source_size = [source.width() as f32, source.height() as f32];
    let image = source.into_image();
    let picture = if shaders {
        // Three major spoken beats receive a subtle neutral exposure response.
        // The picture shader keeps one source sample, including these frames.
        let optical = if matches!(shot_index, 2 | 6 | 15) {
            (1.0 - age / (4.0 / 30.0)).clamp(0.0, 1.0).powi(2)
        } else {
            0.0
        };
        let layer = FILM.draw(
            frame,
            ShaderUniforms::new()
                .image("uSource", &image)
                .float2("uSourceSize", source_size[0], source_size[1])
                .float2("uCenter", center[0], center[1])
                .float("uZoom", zoom)
                .float("uRoll", roll)
                .float("uGrade", camera.grade)
                .float("uOptical", optical),
        );
        svgr!(<image href={layer.href()} width="1920" height="1080" />)
    } else {
        // Geometry remains the same in the CPU SVG fallback; only the optical
        // film treatment needs a shader-capable renderer.
        let dx = 960.0 - center[0] * 1920.0 * zoom;
        let dy = 540.0 - center[1] * 1080.0 * zoom;
        svgr!(<g transform={format!("translate(960 540) rotate({}) translate(-960 -540) translate({dx} {dy}) scale({zoom})", roll.to_degrees())}>
            <image href={image.href()} width="1920" height="1080" preserveAspectRatio="xMidYMid slice" />
        </g>)
    };
    svgr!(<g>
        <defs>
            <linearGradient id="montage-shade" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stop-color="#02050b" stop-opacity="0.025" />
                <stop offset="0.32" stop-color="#02050b" stop-opacity="0.05" />
                <stop offset="0.64" stop-color="#02050b" stop-opacity="0.14" />
                <stop offset="1" stop-color="#02050b" stop-opacity="0.22" />
            </linearGradient>
            <radialGradient id="montage-focus" cx="0.5" cy="0.56" r="0.66">
                <stop offset="0" stop-color="#02060d" stop-opacity="0.11" />
                <stop offset="0.80" stop-color="#02060d" stop-opacity="0.025" />
                <stop offset="1" stop-color="#02060d" stop-opacity="0" />
            </radialGradient>
        </defs>
        {picture}
        <rect width="1920" height="1080" fill="url(#montage-shade)" />
        <rect width="1920" height="1080" fill="url(#montage-focus)" />
    </g>)
}

fn missing_frame(media: &str, index: usize) -> Svgr<'static> {
    // An unexpected decode failure remains visible instead of silently producing black.
    let message = format!("Missing footage: {media}, source frame {index}");
    svgr!(<g>
        <rect width="1920" height="1080" fill="#580a23" />
        <text x="80" y="140" font-family="DM Sans" font-size="36" fill="white">{message}</text>
    </g>)
}
