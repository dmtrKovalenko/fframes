use fframes::{Frame, Svgr};

fn center(text: &'static str, y: usize, size: usize, color: &'static str) -> Svgr<'static> {
    fframes::svgr!(
        <text x="960" y={y} text-anchor="middle" font-family="DM Sans" font-weight="500"
              font-size={size} letter-spacing="-1.8" fill={color}>{text}</text>
    )
}

pub(crate) fn render(name: &str, frame: &Frame) -> Svgr<'static> {
    match name {
        "Glass" => fframes::svgr!(
            <g>
                <g transform="translate(1.2 1.4)">{center("FFRAMES HAS SHADERS", 638, 68, "#ff7c51")}</g>
                <g transform="translate(-1 -1)">{center("FFRAMES HAS SHADERS", 638, 68, "#6989ff")}</g>
                {center("FFRAMES HAS SHADERS", 638, 68, "#f8f8f8")}
            </g>
        ),
        "Gradient" => center("SkSL + SHADERTOY", 576, 64, "#fcf5ff"),
        "OpenSource" => fframes::svgr!(
            <g>
                <text x="152" y="278" font-family="JetBrains Mono" font-size="26"
                      letter-spacing="3" fill="#565967">"YOURS TO CREATE."</text>
                <text x="142" y="478" font-family="DM Sans" font-weight="500"
                      font-size="174" letter-spacing="-7" fill="#171922">"open"</text>
                <text x="142" y="645" font-family="DM Sans" font-weight="500"
                      font-size="174" letter-spacing="-7" fill="#171922">"source"</text>
                <path d="M154 722H222" stroke="#79758b" stroke-width="2" />
                <text x="152" y="785" font-family="DM Sans" font-size="32"
                      fill="#565967">"Made with fframes."</text>
            </g>
        ),
        "ShaderMode" | "SpectralCut" => {
            fframes::svgr!(
                <g>
                    <text x="960" y="381" text-anchor="middle" font-family="JetBrains Mono"
                          font-size="27" letter-spacing="4" fill="#c9c4df">"FFRAMES"</text>
                    <text x="960" y="554" text-anchor="middle" font-family="DM Sans" font-weight="500"
                          font-size="152" letter-spacing="-6" fill="#f6f4ff">"shader mode"</text>
                    <text x="960" y="637" text-anchor="middle" font-family="DM Sans"
                          font-size="32" letter-spacing="0.5" fill="#c9c4df">"Your code. In motion."</text>
                </g>
            )
        }
        "Code" => code(frame),
        _ => Svgr::empty(),
    }
}

fn code(frame: &Frame) -> Svgr<'static> {
    let opacity = ((frame.seconds() - 0.2) / 0.3).clamp(0.0, 1.0);
    fframes::svgr!(
        <g>
            <text x="76" y="82" font-family="JetBrains Mono" font-size="23" fill="#eeecf8">"fframes / shader mode"</text>
            <text x="1844" y="82" text-anchor="end" font-family="JetBrains Mono" font-size="23" fill="#c1becd">"Skia GPU"</text>
            <g opacity={opacity}>
                <rect x="64" y="715" width="1792" height="301" rx="10" fill="#08080d" fill-opacity="0.88" stroke="#45414f" />
                <path d="M64 765H1856 M970 765V1016" stroke="#39353e" />
                <text x="95" y="748" font-family="JetBrains Mono" font-size="20" fill="#cfc9da">"light.sksl"</text>
                <text x="1006" y="748" font-family="JetBrains Mono" font-size="20" fill="#cfc9da">"src/lib.rs"</text>
                <text x="95" y="818" font-family="JetBrains Mono" font-size="25" fill="#bea4f1">"uniform float iTime;"</text>
                <text x="95" y="864" font-family="JetBrains Mono" font-size="25" fill="#e5e1ed">"half4 main(float2 coord) {"</text>
                <text x="95" y="910" font-family="JetBrains Mono" font-size="25" fill="#afd8d7">"  return half4(light(coord, iTime), 1);"</text>
                <text x="95" y="956" font-family="JetBrains Mono" font-size="25" fill="#e5e1ed">"}"</text>
                <text x="1006" y="818" font-family="JetBrains Mono" font-size="25" fill="#e5e1ed">"let shader = Shader::sksl(source);"</text>
                <text x="1006" y="864" font-family="JetBrains Mono" font-size="25" fill="#afd8d7">"let layer = shader.draw(&frame, uniforms);"</text>
                <text x="1006" y="910" font-family="JetBrains Mono" font-size="25" fill="#e2c19d">{"svgr!(<image href={layer.href()}"}</text>
                <text x="1006" y="956" font-family="JetBrains Mono" font-size="25" fill="#e2c19d">{"  width=\"1920\" height=\"1080\" />)"}</text>
            </g>
        </g>
    )
}
