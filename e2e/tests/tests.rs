use e2e_test_video::test_video::TestVideo;
pub use fframes_renderer::{fframes_logger, render, render_backend, RenderOptions};
use rayon::prelude::*;
use std::env::consts::{ARCH, OS};
use std::{fs, process::Command};

#[test]
fn e2e_rendering() {
    println!("Running e2e rendering tests for {OS}-{ARCH}");

    render(
        TestVideo {
            slug: "This frame index:".to_owned(),
        },
        "out.mp4",
        RenderOptions {
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            render_backend: render_backend::CpuRenderingBackend {
                cache_capacity: 5,
                concurrency: 1,
                ..Default::default()
            },
           
            media_dir: std::env::current_dir()
                .unwrap()
                .join("media")
                .to_str()
                .unwrap(),
            ..Default::default()
        },
    )
    .unwrap();

    let base_frames_path = std::env::current_dir().unwrap().join("frames");

    let frames_base_dir = base_frames_path.join("base").join(format!("{OS}-{ARCH}"));
    let frames_results_dir = base_frames_path.join("results");
    let frames_diff_dir = base_frames_path.join("diff");

    if !frames_results_dir.exists() {
        std::fs::create_dir(&frames_results_dir)
            .expect("failed to create frames results directory");
    }

    if !frames_diff_dir.exists() {
        std::fs::create_dir(&frames_diff_dir).expect("failed to create frames results directory");
    } else {
        std::fs::remove_dir_all(&frames_diff_dir).expect("failed to remove frames diff directory");
        std::fs::create_dir(&frames_diff_dir).expect("failed to create frames results directory");
    }

    Command::new("ffmpeg")
        .args([
            "-i",
            "out.mp4",
            format!("{}/out-%03d.png", frames_results_dir.display()).as_str(),
        ])
        .output()
        .expect("failed to extract images from video with ffmpeg");

    let frames_entries = fs::read_dir(&frames_base_dir)
        .unwrap()
        .filter_map(|path_buf| {
            path_buf.ok().and_then(|path_buf| {
                let path = path_buf.path();
                if path.is_dir() {
                    None
                } else {
                    Some(path)
                }
            })
        })
        .collect::<Vec<_>>();

    let odiff_path = std::fs::canonicalize(
        std::env::current_dir()
            .unwrap()
            .join("../node_modules/odiff-bin/bin/odiff"),
    )
    .unwrap();

    let failed_count = frames_entries
        .into_par_iter()
        .map(|result_frame| {
            let file_name = result_frame.file_name().unwrap().to_str().unwrap();

            let base_frame = frames_base_dir.join(file_name);
            let diff_path = frames_diff_dir.join(file_name);

            let diff_result = Command::new(odiff_path.clone())
                .args([
                    "--parsable-stdout",
                    base_frame.to_str().unwrap(),
                    result_frame.to_str().unwrap(),
                    diff_path.to_str().unwrap(),
                    "-t",
                    "0.7",
                ])
                .output()
                .expect("failed to get a diff");

            if diff_result.status.success() {
                0
            } else {
                1
            }
        })
        .sum::<u32>();

    if failed_count > 0 {
        panic!(
            "{} frames are visually different, check frames/diff folder for details",
            failed_count
        );
    }
}
