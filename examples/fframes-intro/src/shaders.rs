//! Every GPU shader of the video, compiled once by the Skia renderer.

use std::sync::LazyLock;

use fframes::Shader;

pub struct Shaders {
    pub grain: Shader,
    pub contour: Shader,
    pub tunnel: Shader,
    pub key: Shader,
    pub develop: Shader,
    pub grid: Shader,
}

pub const TUNNEL_SOURCE: &str = include_str!("shaders/tunnel.sksl");

pub static SHADERS: LazyLock<Shaders> = LazyLock::new(|| Shaders {
    grain: Shader::sksl(include_str!("shaders/grain.sksl")),
    contour: Shader::sksl(include_str!("shaders/contour.sksl")),
    tunnel: Shader::sksl(TUNNEL_SOURCE),
    key: Shader::sksl(include_str!("shaders/key.sksl")),
    develop: Shader::sksl(include_str!("shaders/develop.sksl")),
    grid: Shader::sksl(include_str!("shaders/grid.sksl")),
});
