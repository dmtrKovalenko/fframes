use crate::diagnostics::Diagnostic;
use crate::{
    AudioMap, Color, CpuFrameRenderer, FFramesContext, FFramesRendererError, Frame, Previewer,
    RenderOptions, Scene, Svgr, Video,
};

#[derive(Debug)]
struct Intro;

impl Scene for Intro {
    fn duration(&self) -> crate::Duration<'_> {
        crate::Duration::Frames(10)
    }

    fn render_frame(&self, frame: Frame, _: &FFramesContext) -> Svgr<'_> {
        assert!(frame.index != 5, "boom at five");
        crate::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
                <rect x="0" y="0" width="20" height="20" fill="#ff0000" />
            </svg>
        )
    }
}

#[derive(Debug)]
struct Outro;

impl Scene for Outro {
    fn duration(&self) -> crate::Duration<'_> {
        crate::Duration::Frames(10)
    }

    fn render_frame(&self, _: Frame, ctx: &FFramesContext) -> Svgr<'_> {
        let _ = ctx.get_image("missing.png");
        crate::svgr!(<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"></svg>)
    }
}

struct TwoScenes;

impl Video for TwoScenes {
    const FPS: usize = 10;
    const WIDTH: usize = 40;
    const HEIGHT: usize = 20;
    const BACKGROUND_COLOR: Color = Color::rgb(0, 0, 255);

    fn duration(&self) -> crate::Duration<'_> {
        crate::Duration::Auto
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn define_scenes(&self) -> crate::Scenes<'_> {
        crate::Scenes::from(vec![&Intro as &dyn Scene, &Outro as &dyn Scene])
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        ctx.render_scenes(&frame)
    }
}

#[test]
fn previews_paint_the_background_and_fit_the_output_size() {
    let video = TwoScenes;
    let mut previewer = Previewer::new(&video, &RenderOptions::default()).unwrap();
    let mut renderer = CpuFrameRenderer::default();

    let frame = previewer.render(0, &mut renderer).unwrap();
    assert_eq!((frame.width, frame.height), (40, 20));
    // Left half is the red rect, right half the video's blue background (not transparent).
    assert_eq!(&frame.pixels[..4], &[255, 0, 0, 255]);
    let right = (10 * 40 + 35) * 4;
    assert_eq!(&frame.pixels[right..right + 4], &[0, 0, 255, 255]);

    previewer.set_scale(0.5);
    let small = previewer.render(0, &mut renderer).unwrap();
    assert_eq!((small.width, small.height), (20, 10));
    assert_eq!(&small.pixels[..4], &[255, 0, 0, 255]);
    let right = (5 * 20 + 18) * 4;
    assert_eq!(&small.pixels[right..right + 4], &[0, 0, 255, 255]);
}

#[test]
fn panics_report_frame_time_and_scene() {
    let video = TwoScenes;
    let mut previewer = Previewer::new(&video, &RenderOptions::default()).unwrap();

    match previewer.svg_tree(5) {
        Err(FFramesRendererError::FramePanicked(panic)) => {
            assert_eq!(panic.frame, 5);
            assert_eq!(panic.seconds, 0.5);
            assert_eq!(panic.scenes, vec!["Intro".to_owned()]);
            assert!(panic.message.contains("boom at five"));
        }
        other => panic!("expected a frame panic, got {:?}", other.map(|_| ())),
    }

    let report = previewer.inspect(5).unwrap();
    assert!(report.diagnostics.iter().any(
        |d| matches!(&d.diagnostic, Diagnostic::Panic { message } if message.contains("boom"))
    ));
}

#[test]
fn inspect_reports_missing_media_and_empty_frames() {
    let video = TwoScenes;
    let mut previewer = Previewer::new(&video, &RenderOptions::default()).unwrap();
    let frame = previewer.timeline().resolve_frame("Outro@2").unwrap();
    assert_eq!(frame, 12);

    let report = previewer.inspect(frame).unwrap();
    assert_eq!(report.scenes, vec!["Outro".to_owned()]);
    let kinds: Vec<&Diagnostic> = report.diagnostics.iter().map(|d| &d.diagnostic).collect();
    assert!(
        kinds
            .iter()
            .any(|d| matches!(d, Diagnostic::MissingMedia { name, .. } if name == "missing.png"))
    );
    assert!(kinds.iter().any(|d| matches!(d, Diagnostic::EmptyFrame)));
}

struct Labelled;

impl Video for Labelled {
    const FPS: usize = 10;
    const WIDTH: usize = 200;
    const HEIGHT: usize = 100;
    const BACKGROUND_COLOR: Color = Color::rgb(255, 255, 255);

    fn duration(&self) -> crate::Duration<'_> {
        crate::Duration::Frames(1)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, _: Frame, _: &FFramesContext<'a, '_>) -> Svgr<'a> {
        crate::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
                <g id="group" transform="translate(100 0)">
                    <rect id="box" x="10" y="20" width="30" height="40" fill="#336699" />
                </g>
                <path id="triangle" d="M10 90 L50 50 L60 90 Z" fill="#cc3300" />
                <rect x="0" y="0" width="5" height="5" fill="#000000" />
            </svg>
        )
    }
}

#[test]
fn elements_with_ids_are_listed_and_labelled() {
    use crate::renderer::element_labels::{draw_labels, labelled_elements};

    let video = Labelled;
    let mut previewer = Previewer::new(&video, &RenderOptions::default()).unwrap();
    let mut renderer = CpuFrameRenderer::default();
    let tree = previewer.svg_tree(0).unwrap();
    let elements = labelled_elements(&tree);

    let ids: Vec<&str> = elements.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["group", "box", "triangle"]);
    let rect = &elements[1];
    assert_eq!(rect.kind, "path");
    // Bounding boxes are in the video's coordinates, transforms applied.
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (110., 20., 30., 40.)
    );
    assert_eq!(elements[0].kind, "group");

    let frame = previewer.render(0, &mut renderer).unwrap();
    let labelled =
        draw_labels(&frame, &tree, &elements, previewer.font_db(), "sans-serif").unwrap();
    assert_eq!(
        (labelled.width, labelled.height),
        (frame.width, frame.height)
    );
    assert_ne!(labelled.pixels, frame.pixels);
}

#[test]
fn frames_crop_to_a_clamped_rectangle() {
    let video = Labelled;
    let mut previewer = Previewer::new(&video, &RenderOptions::default()).unwrap();
    let frame = previewer
        .render(0, &mut CpuFrameRenderer::default())
        .unwrap();

    let crop = frame.crop(110, 20, 30, 40);
    assert_eq!((crop.width, crop.height), (30, 40));
    // The crop is exactly the blue rect.
    assert!(
        crop.pixels
            .chunks(4)
            .all(|px| px == [0x33, 0x66, 0x99, 255])
    );

    let corner = frame.crop(190, 90, 50, 50);
    assert_eq!((corner.width, corner.height), (10, 10));
}
