//! Camera gestures and transition windows measured in source frames.

use fframes::Svgr;

fn progress(index: usize, start: usize, end: usize) -> f32 {
    ((index as f32 - start as f32) / (end - start) as f32).clamp(0.0, 1.0)
}

pub(crate) fn apply<'a>(index: usize, name: &str, composition: Svgr<'a>) -> Svgr<'a> {
    let (mut scale, mut rotation, mut skew, mut dy, mut opacity) = (1.0, 0.0, 0.0, 0.0, 1.0);
    let mut dx = 0.0;
    match name {
        "Glass" => scale = 1.0 + 3.0 * (1.0 - progress(index, 26, 35)).powi(3),
        "Halftone" => scale = 1.0 + progress(index, 253, 256).powi(3) * 1.3,
        "Flow" => scale = 1.0 + progress(index, 443, 447).powi(3) * 8.0,
        "ShaderModeEditor" => {
            scale = 0.85;
            skew = -3.0;
            rotation = -0.6;
        }
        "ShaderModeZoom" => {
            scale = 1.14;
            dx = -60.0 * (1.0 - progress(index, 586, 610));
            skew = -3.0;
        }
        "ShaderModeWide" => {
            scale = 0.86;
            skew = -2.0;
        }
        "Eclipse" => {
            let p = progress(index, 836, 849).powi(2);
            scale = 1.0 - p * 0.97;
            rotation = p * 14.0;
            dy = p * 170.0;
            opacity = progress(index, 654, 680).powi(2) * (1.0 - p);
        }
        _ => {}
    }
    let flash = if (137..148).contains(&index) {
        (-((index as f32 - 141.0) / 2.7).powi(2)).exp()
    } else if (445..450).contains(&index) {
        (-((index as f32 - 446.0) / 1.3).powi(2)).exp() * 0.45
    } else {
        0.0
    };
    let glitch = if (608..610).contains(&index) {
        0.18
    } else if (636..640).contains(&index) {
        progress(index + 1, 636, 640)
    } else {
        0.0
    };
    let bars: Vec<_> = if glitch > 0.0 {
        (0..28)
            .map(|row| {
                let hash = ((row * 73 + index * 19) % 101) as f32 / 101.0;
                let y = row as f32 * 1080.0 / 28.0;
                let height = 1080.0 / 28.0 * (glitch * 1.5 - hash * 0.45).clamp(0.02, 1.0);
                fframes::svgr!(<rect x="0" y={y} width="1920" height={height} fill="#000" />)
            })
            .collect()
    } else {
        Vec::new()
    };
    fframes::svgr!(
        <g>
            <defs><clipPath id="shot-canvas"><rect width="1920" height="1080" /></clipPath></defs>
            <g clip-path="url(#shot-canvas)">
                <g opacity={opacity} transform={format!("translate({} {}) rotate({rotation}) skewX({skew}) scale({scale}) translate(-960 -540)",960.0+dx,540.0+dy)}>
                    {composition}
                </g>
                {bars}
                <rect width="1920" height="1080" fill="#fffcf2" opacity={flash} />
            </g>
        </g>
    )
}
