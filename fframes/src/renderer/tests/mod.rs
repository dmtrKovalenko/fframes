#[cfg(feature = "cpu_renderer")]
mod alpha_render_tests;
// Video frames are drawn through `<image href>`, which needs the compile-time tree.
#[cfg(all(feature = "cpu_renderer", feature = "compile-time-svgtree"))]
mod clip_tests;
mod preview_tests;
#[cfg(feature = "cpu_renderer")]
mod svgr_image_tests;
mod yuv420_conversion_tests;
#[cfg(all(feature = "h264", feature = "cpu_renderer"))]
mod yuv_matrix_render_tests;
