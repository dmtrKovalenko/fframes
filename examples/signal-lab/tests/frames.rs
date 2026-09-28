//! Visual regression: renders a few frames and compares them with the approved PNGs in
//! `_frame_snapshots/`. The first run creates them; `FFRAMES_UPDATE_SNAPSHOTS=1 cargo test`
//! accepts intentional changes. Failing frames leave `.actual.png` and `.diff.png` files.
use fframes::{CpuFrameRenderer, Previewer, RenderOptions, StaticMediaProvider, snapshot};
use signal_lab::{SignalLabMedia, SignalLabVideo};

#[test]
fn key_frames_match_snapshots() {
    let media = SignalLabMedia::prepare().unwrap();
    let video = SignalLabVideo;
    let options = RenderOptions {
        media: Some(&media),
        scale_resolution: 0.5,
        ..Default::default()
    };
    let mut previewer = Previewer::new(&video, &options).unwrap();

    snapshot::assert_frames(
        &mut previewer,
        &mut CpuFrameRenderer::default(),
        &[
            "ProductScene@3s",
            "DataScene@3s",
            "SystemScene@3s",
            "FinalScene@3s",
        ],
        &snapshot::SnapshotOptions::default(),
    );
}

#[test]
fn no_problems_in_any_frame() {
    let media = SignalLabMedia::prepare().unwrap();
    let video = SignalLabVideo;
    let options = RenderOptions {
        media: Some(&media),
        ..Default::default()
    };
    let mut previewer = Previewer::new(&video, &options).unwrap();

    let duration = previewer.timeline().duration_in_frames;
    for frame in 0..duration {
        let report = previewer.inspect(frame).unwrap();
        let problems: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.severity >= fframes::diagnostics::Severity::Warning)
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            problems.is_empty(),
            "frame {frame} ({:.2}s): {problems:?}",
            report.seconds
        );
    }
}
