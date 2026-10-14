use std::sync::Mutex;

use crate::media::ImageData;
use crate::{
    AudioMap, Color, CpuFrameRenderer, FFramesContext, Frame, Previewer, RenderOptions, Svgr, Video,
};

/// Renders a red square into an svgr image, puts that image into a second svgr image, and
/// the second one into the frame.
#[derive(Default)]
struct Nested {
    images: Mutex<Vec<ImageData<'static>>>,
}

impl Video for Nested {
    const FPS: usize = 10;
    const WIDTH: usize = 40;
    const HEIGHT: usize = 40;
    const BACKGROUND_COLOR: Color = Color::rgb(0, 0, 255);

    fn duration(&self) -> crate::Duration<'_> {
        crate::Duration::Frames(2)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, _: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let Some(red) = ctx.render_svgr(
            crate::svgr!(
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10">
                    <rect x="0" y="0" width="10" height="10" fill="#ff0000" />
                </svg>
            ),
            8,
            8,
        ) else {
            return crate::svgr!(<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"></svg>);
        };
        let Some(outer) = ctx.render_svgr(
            crate::svgr!(
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" width="20" height="20">
                    <rect x="0" y="0" width="20" height="20" fill="#00ff00" />
                    <image href={red.href()} x="0" y="0" width="10" height="10" />
                </svg>
            ),
            20,
            20,
        ) else {
            return crate::svgr!(<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"></svg>);
        };
        self.images.lock().unwrap().push(red);
        crate::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40" width="40" height="40">
                <image href={outer.href()} x="0" y="0" width="40" height="40" />
            </svg>
        )
    }
}

// The images of `<image href>` resolve in trees made at compile time.
#[cfg(feature = "compile-time-svgtree")]
#[test]
fn svgr_images_nest_and_render_with_the_backend() {
    let video = Nested::default();
    let mut previewer = Previewer::new(&video, &RenderOptions::default()).unwrap();
    let mut renderer = CpuFrameRenderer::default();
    let frame = previewer.render(0, &mut renderer).unwrap();
    let pixel = |x: usize, y: usize| &frame.pixels[(y * 40 + x) * 4..][..4];
    // the red image fills the top left quarter of the green image, which fills the frame
    assert_eq!(pixel(10, 10), [255, 0, 0, 255]);
    assert_eq!(pixel(30, 30), [0, 255, 0, 255]);

    previewer.render(1, &mut renderer).unwrap();
    let images = video.images.lock().unwrap();
    assert_eq!(images.len(), 2);
    // a tree without `{}` renders once
    assert!(std::sync::Arc::ptr_eq(&images[0].href(), &images[1].href()));
    assert_eq!(&images[0].image.data[..4], &[255, 0, 0, 255]);
}

#[test]
fn svgr_images_need_a_render() {
    let svgr = crate::svgr!(<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"></svg>);
    assert!(crate::svgr_image::render_svgr(svgr, 4, 4).is_none());
}

#[test]
fn a_backend_without_a_rasterizer_gives_no_images() {
    let video = Nested::default();
    let mut previewer = Previewer::new(&video, &RenderOptions::default())
        .unwrap()
        .with_svgr_rasterizer(None);
    let frame = previewer
        .render(0, &mut CpuFrameRenderer::default())
        .unwrap();
    // `render_frame` falls back to an empty frame: the background
    assert_eq!(&frame.pixels[..4], &[0, 0, 255, 255]);
}
