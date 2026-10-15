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

struct MarkedVideo;

impl Video for MarkedVideo {
    const FPS: usize = 10;
    const WIDTH: usize = 40;
    const HEIGHT: usize = 20;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> crate::Duration<'_> {
        crate::Duration::Seconds(2.0)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn markers(&self) -> crate::Markers<'_> {
        crate::Markers::from([("title-hit", crate::Duration::Seconds(0.5))])
    }

    fn render_frame<'a>(&'a self, _: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let marker = ctx.marker("title-hit").expect("title-hit");
        assert_eq!(marker.frame, 5);
        crate::svgr!(<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"></svg>)
    }
}

#[test]
fn timeline_resolves_and_reports_markers() {
    let video = MarkedVideo;
    let mut previewer = Previewer::new(&video, &RenderOptions::default()).unwrap();
    assert_eq!(previewer.timeline().resolve_frame("title-hit").unwrap(), 5);
    assert_eq!(
        previewer.timeline().resolve_range("title-hit").unwrap(),
        5..6
    );

    let report = previewer.timeline_report();
    assert_eq!(report.markers.len(), 1);
    assert_eq!(report.markers[0].name, "title-hit");
    assert_eq!(report.markers[0].frame, 5);

    let mut renderer = CpuFrameRenderer::default();
    previewer.render(5, &mut renderer).unwrap();
}
