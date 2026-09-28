use fframes::Shader;
use fframes_skia_renderer::render::compile_shader;

use crate::{AURORA_SKSL, TORUS_SHADERTOY};

#[test]
fn shaders_compile() {
    compile_shader(&Shader::sksl(AURORA_SKSL)).unwrap();
    compile_shader(&Shader::shadertoy(TORUS_SHADERTOY)).unwrap();
}
