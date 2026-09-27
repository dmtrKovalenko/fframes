use fframes::Shader;
use fframes_skia_renderer::render::compile_shader;

use crate::TRIANGLE_SKSL;

#[test]
fn shader_compiles() {
    compile_shader(&Shader::sksl(TRIANGLE_SKSL)).unwrap();
}
