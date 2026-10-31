use std::path::PathBuf;
use std::process::Command;

use crate::{
    AudioMap, Clip, Color, CpuFrameRenderer, FFramesContext, Frame, MediaDirectory, Previewer,
    RenderOptions, Scene, Scenes, Svgr, Video,
};

/// A 3 s video at 10 fps: red for the first second, green for the second, blue for the
/// third. Requires the ffmpeg CLI with libx264, like the video decoder tests.
struct ColorsFixture(PathBuf);

impl ColorsFixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "fframes-clip-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut command = Command::new("ffmpeg");
        command.args(["-hide_banner", "-loglevel", "error", "-y"]);
        for color in ["red", "lime", "blue"] {
            command.args([
                "-f",
                "lavfi",
                "-i",
                &format!("color={color}:s=16x16:r=10:d=1"),
            ]);
        }
        let output = command
            .args([
                "-filter_complex",
                "concat=n=3:v=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-g",
                "10",
                "-an",
            ])
            .arg(dir.join("colors.mp4"))
            .output()
            .expect("ffmpeg with libx264 is required for clip tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Self(dir)
    }
}

impl Drop for ColorsFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Draws `left` on the left half and `right` on the right half of a 40x20 frame, `right`
/// decoded at the size it is shown.
#[derive(Debug)]
struct TwoClips<'a> {
    left: Clip<'a>,
    right: Clip<'a>,
}

impl Scene for TwoClips<'_> {
    fn duration(&self) -> crate::Duration<'_> {
        self.left.duration()
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let left = self.left.image(&frame, ctx).map(
            |image| crate::svgr!(<image href={image.href()} x="0" y="0" width="20" height="20" />),
        );
        let right = self.right.image_at_width(&frame, ctx, 20).map(
            |image| crate::svgr!(<image href={image.href()} x="20" y="0" width="20" height="20" />),
        );
        crate::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
                {left}
                {right}
            </svg>
        )
    }
}

struct ClipVideo<'a>(TwoClips<'a>);

impl Video for ClipVideo<'_> {
    const FPS: usize = 10;
    const WIDTH: usize = 40;
    const HEIGHT: usize = 20;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> crate::Duration<'_> {
        crate::Duration::Auto
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn define_scenes(&self) -> Scenes<'_> {
        Scenes::from(vec![&self.0 as &dyn Scene])
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        ctx.render_scenes(&frame)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Shown {
    Red,
    Green,
    Blue,
    Nothing,
}

fn shown([r, g, b, _]: [u8; 4]) -> Shown {
    match (r > 160, g > 160, b > 160) {
        (true, false, false) => Shown::Red,
        (false, true, false) => Shown::Green,
        (false, false, true) => Shown::Blue,
        (false, false, false) => Shown::Nothing,
        _ => panic!("unexpected color {r} {g} {b}"),
    }
}

/// What the left and the right half show on each of `frames`.
fn render(left: Clip, right: Clip, frames: &[usize]) -> Vec<(Shown, Shown)> {
    let fixture = ColorsFixture::new();
    let folder = MediaDirectory::read_folder(&fixture.0).unwrap();
    let media = folder.process_media_source().unwrap();
    let video = ClipVideo(TwoClips { left, right });
    let options = RenderOptions {
        media: Some(&media),
        ..Default::default()
    };
    let mut previewer = Previewer::new(&video, &options).unwrap();
    let mut renderer = CpuFrameRenderer::default();

    frames
        .iter()
        .map(|&index| {
            let frame = previewer.render(index, &mut renderer).unwrap();
            let pixel = |x: usize| {
                let i = (10 * 40 + x) * 4;
                frame.pixels[i..i + 4].try_into().unwrap()
            };
            (shown(pixel(10)), shown(pixel(30)))
        })
        .collect()
}

#[test]
fn two_clips_of_one_file_play_their_own_parts() {
    use Shown::*;
    let shown = render(
        Clip::new("colors.mp4", 1.0..2.0),
        Clip::new("colors.mp4", 2.0..2.5),
        &[0, 4, 5, 9],
    );
    assert_eq!(
        shown,
        [
            (Green, Blue),
            (Green, Blue),
            // the right clip is 0.5 s long
            (Green, Nothing),
            (Green, Nothing),
        ]
    );
}

#[test]
fn speed_and_reverse_play_the_source_range() {
    use Shown::*;
    let whole = Clip::new("colors.mp4", 0.0..3.0).speed(3.0);
    let shown = render(whole.clone(), whole.reverse(), &[0, 3, 4, 6, 7, 9]);
    assert_eq!(
        shown,
        [
            // source 0.0 and 2.7
            (Red, Blue),
            // 0.9 and 1.8
            (Red, Green),
            // 1.2 and 1.5
            (Green, Green),
            // 1.8 and 0.9
            (Green, Red),
            // 2.1 and 0.6
            (Blue, Red),
            // 2.7 and 0.0
            (Blue, Red),
        ]
    );
}

#[test]
fn a_clip_starts_at_its_second_of_the_scene() {
    use Shown::*;
    let shown = render(
        Clip::new("colors.mp4", 0.0..1.0),
        Clip::new("colors.mp4", 2.0..2.3).at(0.5),
        &[0, 4, 5, 7, 8],
    );
    assert_eq!(
        shown,
        [
            (Red, Nothing),
            (Red, Nothing),
            (Red, Blue),
            (Red, Blue),
            (Red, Nothing),
        ]
    );
}
