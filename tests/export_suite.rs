#[allow(dead_code)]
#[path = "../src/bin/export_video.rs"]
mod export_video;

use clap::Parser;
use std::path::PathBuf;

#[test]
fn parse_args_defaults_are_stable() {
    let args = export_video::Cli::try_parse_from(["export_video", "--audio", "input.wav"])
        .expect("parse should succeed");

    assert_eq!(args.audio, PathBuf::from("input.wav"));
    assert_eq!(args.out, PathBuf::from("export.mp4"));
    assert_eq!(args.width, 1280);
    assert_eq!(args.height, 720);
    assert_eq!(args.fps, 60);
    assert_eq!(args.duration, None);
    assert_eq!(args.engine, export_video::EngineArg::Metal);
}

#[test]
fn parse_args_overrides_work() {
    let args = export_video::Cli::try_parse_from([
        "export_video",
        "--audio",
        "song.wav",
        "--out",
        "clips/out.mp4",
        "--width",
        "640",
        "--height",
        "360",
        "--fps",
        "30",
        "--duration",
        "12.5",
        "--preset",
        "mandelbrot",
        "--engine",
        "cpu",
        "--safe",
    ])
    .expect("parse should succeed");

    assert_eq!(args.audio, PathBuf::from("song.wav"));
    assert_eq!(args.out, PathBuf::from("clips/out.mp4"));
    assert_eq!(args.width, 640);
    assert_eq!(args.height, 360);
    assert_eq!(args.fps, 30);
    assert_eq!(args.duration, Some(12.5));
    assert_eq!(args.preset.as_deref(), Some("mandelbrot"));
    assert_eq!(args.engine, export_video::EngineArg::Cpu);
    assert!(args.safe);
}

#[test]
fn parse_rejects_zero_fps() {
    let args = export_video::Cli::try_parse_from([
        "export_video",
        "--audio",
        "song.wav",
        "--fps",
        "0",
    ])
    .expect("parse should succeed");

    let err = export_video::validate_args(&args).expect_err("fps=0 must fail validation");
    assert!(err.to_string().contains("--fps"));
}

#[test]
fn duration_and_frame_math_is_deterministic() {
    assert!((export_video::compute_export_duration(30.0, None) - 30.0).abs() < 1e-6);
    assert!((export_video::compute_export_duration(30.0, Some(12.25)) - 12.25).abs() < 1e-6);
    assert!((export_video::compute_export_duration(5.0, Some(10.0)) - 5.0).abs() < 1e-6);

    assert_eq!(export_video::compute_frame_count(2.0, 60), 120);
    assert_eq!(export_video::compute_frame_count(2.999, 30), 89);
    assert_eq!(export_video::compute_frame_count(0.01, 60), 1);
}

#[test]
fn frame_count_is_repeatable_for_fractional_edges() {
    let cases = [
        (59.0 / 60.0, 60, 59usize),
        (61.0 / 60.0, 60, 61usize),
        (2.9999, 30, 89usize),
        (10.0 / 24.0, 24, 10usize),
    ];

    for _ in 0..64 {
        for (duration_s, fps, expected) in cases {
            assert_eq!(export_video::compute_frame_count(duration_s, fps), expected);
        }
    }
}

#[test]
fn validate_rejects_non_positive_duration_cap() {
    let args = export_video::Cli::try_parse_from([
        "export_video",
        "--audio",
        "song.wav",
        "--duration",
        "0",
    ])
    .expect("parse should succeed");

    let err = export_video::validate_args(&args).expect_err("duration=0 must fail validation");
    assert!(err.to_string().contains("--duration"));
}
