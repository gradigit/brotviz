use std::time::{Duration, Instant};

use anyhow::Result;
use tui_visualizer::audio::AudioFeatures;
use tui_visualizer::config::{Quality, SwitchMode};
use tui_visualizer::visual::{make_presets, RenderCtx, VisualEngine};

#[cfg(target_os = "macos")]
use tui_visualizer::visual::MetalEngine;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Cpu,
    Metal,
    Both,
}

struct Args {
    mode: Mode,
    frames: usize,
    w: usize,
    h: usize,
    quality: Quality,
    scale: usize,
    safe: bool,
    ci_smoke: bool,
    max_ms: f64,
}

fn parse_args() -> Args {
    let mut args = Args {
        mode: Mode::Cpu,
        frames: 180,
        w: 160,
        h: 88,
        quality: Quality::Balanced,
        scale: 1,
        safe: false,
        ci_smoke: false,
        max_ms: 20.0,
    };

    let argv = std::env::args().skip(1).collect::<Vec<_>>();
    let mut i = 0usize;
    while i < argv.len() {
        let k = argv[i].as_str();
        let v = argv.get(i + 1).map(|s| s.as_str());
        match (k, v) {
            ("--mode", Some("cpu")) => {
                args.mode = Mode::Cpu;
                i += 2;
            }
            ("--mode", Some("metal")) => {
                args.mode = Mode::Metal;
                i += 2;
            }
            ("--mode", Some("both")) => {
                args.mode = Mode::Both;
                i += 2;
            }
            ("--frames", Some(x)) => {
                if let Ok(n) = x.parse::<usize>() {
                    args.frames = n.max(1);
                }
                i += 2;
            }
            ("--w", Some(x)) => {
                if let Ok(n) = x.parse::<usize>() {
                    args.w = n.max(1);
                }
                i += 2;
            }
            ("--h", Some(x)) => {
                if let Ok(n) = x.parse::<usize>() {
                    args.h = n.max(1);
                }
                i += 2;
            }
            ("--scale", Some(x)) => {
                if let Ok(n) = x.parse::<usize>() {
                    args.scale = n.max(1);
                }
                i += 2;
            }
            ("--quality", Some("fast")) => {
                args.quality = Quality::Fast;
                i += 2;
            }
            ("--quality", Some("balanced")) => {
                args.quality = Quality::Balanced;
                i += 2;
            }
            ("--quality", Some("high")) => {
                args.quality = Quality::High;
                i += 2;
            }
            ("--quality", Some("ultra")) => {
                args.quality = Quality::Ultra;
                i += 2;
            }
            ("--safe", Some(x)) => {
                if let Some(v) = parse_bool(x) {
                    args.safe = v;
                }
                i += 2;
            }
            ("--ci-smoke", Some(x)) if !x.starts_with("--") => {
                args.ci_smoke = parse_bool(x).unwrap_or(true);
                i += 2;
            }
            ("--ci-smoke", _) => {
                args.ci_smoke = true;
                i += 1;
            }
            ("--max-ms", Some(x)) => {
                if let Ok(v) = x.parse::<f64>() {
                    args.max_ms = v.max(0.1);
                }
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    args
}

fn parse_bool(s: &str) -> Option<bool> {
    let v = s.trim().to_ascii_lowercase();
    match v.as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn synth_audio(t: f32, step: usize) -> AudioFeatures {
    let bass = ((t * 1.9).sin() * 0.5 + 0.5).powf(1.15);
    let mid = ((t * 2.8 + 0.7).sin() * 0.5 + 0.5).powf(1.08);
    let treb = ((t * 5.2 + 1.3).sin() * 0.5 + 0.5).powf(1.02);

    let hard_hit = step % 24 == 0;
    let soft_hit = step % 12 == 0;

    let onset = if hard_hit {
        0.92
    } else if soft_hit {
        0.58
    } else {
        (treb * 0.35 + mid * 0.25).min(0.5)
    };

    let beat_strength = if hard_hit {
        0.95
    } else if soft_hit {
        0.55
    } else {
        0.0
    };

    AudioFeatures {
        rms: (0.12 + bass * 0.42 + mid * 0.30 + treb * 0.20).clamp(0.0, 1.0),
        bands: [
            (bass * 0.95).clamp(0.0, 1.0),
            bass.clamp(0.0, 1.0),
            (bass * 0.6 + mid * 0.35).clamp(0.0, 1.0),
            mid.clamp(0.0, 1.0),
            (mid * 0.5 + treb * 0.45).clamp(0.0, 1.0),
            treb.clamp(0.0, 1.0),
            (treb * 0.9).clamp(0.0, 1.0),
            (treb * 0.75 + mid * 0.2).clamp(0.0, 1.0),
        ],
        onset,
        beat: hard_hit || soft_hit,
        beat_strength,
        centroid: (0.2 + treb * 0.6 + mid * 0.15).clamp(0.0, 1.0),
        flatness: (0.15 + treb * 0.55).clamp(0.0, 1.0),
    }
}

fn bench_cpu(args: &Args) -> Result<()> {
    let mut presets = make_presets();
    let n = args.w.saturating_mul(args.h).saturating_mul(4);
    let mut total_time = Duration::ZERO;
    let mut total_frames = 0usize;
    let mut black_presets = Vec::<String>::new();
    let mut slow_presets = Vec::<(String, f64)>::new();

    println!(
        "CPU benchmark: presets={} frames/preset={} size={}x{} quality={:?} scale={}",
        presets.len(), args.frames, args.w, args.h, args.quality, args.scale
    );

    for (idx, p) in presets.iter_mut().enumerate() {
        let name = p.name();
        let mut prev = vec![0u8; n];
        let mut out = vec![0u8; n];

        let start = Instant::now();
        let mut lit = 0usize;

        for f in 0..args.frames {
            let t = f as f32 / 60.0;
            let audio = synth_audio(t, f);
            let ctx = RenderCtx {
                now: Instant::now(),
                t,
                dt: 1.0 / 60.0,
                w: args.w,
                h: args.h,
                audio,
                beat_pulse: if audio.beat { (0.6 + audio.beat_strength * 0.4).min(1.0) } else { 0.0 },
                fractal_zoom_mul: 1.0,
                safe: args.safe,
                quality: args.quality,
                scale: args.scale,
            };
            p.render(&ctx, &prev, &mut out);
            if out.chunks_exact(4).any(|px| px[0] != 0 || px[1] != 0 || px[2] != 0) {
                lit += 1;
            }
            std::mem::swap(&mut prev, &mut out);
        }

        let elapsed = start.elapsed();
        total_time += elapsed;
        total_frames += args.frames;
        let ms = elapsed.as_secs_f64() * 1000.0 / args.frames as f64;
        println!("{:>2}. {:<34} {:>8.3} ms/frame  lit={:>3}/{}", idx, name, ms, lit, args.frames);
        if lit == 0 {
            black_presets.push(name.to_string());
        }
        if args.ci_smoke && ms > args.max_ms {
            slow_presets.push((name.to_string(), ms));
        }
    }

    let avg_ms = total_time.as_secs_f64() * 1000.0 / total_frames.max(1) as f64;
    let fps = if avg_ms > 0.0 { 1000.0 / avg_ms } else { 0.0 };
    println!("CPU summary: {:>8.3} ms/frame avg  {:>7.2} FPS", avg_ms, fps);

    if args.ci_smoke {
        if !black_presets.is_empty() || !slow_presets.is_empty() {
            eprintln!("CI smoke: FAIL");
            if !black_presets.is_empty() {
                eprintln!("  black presets: {}", black_presets.join(", "));
            }
            if !slow_presets.is_empty() {
                for (name, ms) in slow_presets {
                    eprintln!("  slow preset: {} ({:.3} ms/frame > {:.3})", name, ms, args.max_ms);
                }
            }
            anyhow::bail!("ci smoke failed");
        }
        println!("CI smoke: PASS (max_ms={:.3})", args.max_ms);
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn bench_metal(args: &Args) -> Result<()> {
    let presets = make_presets();
    let names = presets.iter().map(|p| p.name()).collect::<Vec<_>>();

    let mut total_time = Duration::ZERO;
    let mut total_frames = 0usize;

    println!(
        "Metal benchmark: presets={} frames/preset={} size={}x{} quality={:?} scale={}",
        names.len(), args.frames, args.w, args.h, args.quality, args.scale
    );

    for idx in 0..names.len() {
        let mut eng = MetalEngine::new(names.clone(), idx, false, SwitchMode::Manual, 16, 20.0)?;
        eng.resize(args.w, args.h);
        let name = eng.preset_name();

        let start = Instant::now();
        let mut lit = 0usize;

        for f in 0..args.frames {
            let t = f as f32 / 60.0;
            let audio = synth_audio(t, f);
            let ctx = RenderCtx {
                now: Instant::now(),
                t,
                dt: 1.0 / 60.0,
                w: args.w,
                h: args.h,
                audio,
                beat_pulse: if audio.beat { (0.6 + audio.beat_strength * 0.4).min(1.0) } else { 0.0 },
                fractal_zoom_mul: 1.0,
                safe: args.safe,
                quality: args.quality,
                scale: args.scale,
            };
            let px = eng.render(ctx, args.quality, args.scale);
            if px.chunks_exact(4).any(|p| p[0] != 0 || p[1] != 0 || p[2] != 0) {
                lit += 1;
            }
        }

        let elapsed = start.elapsed();
        total_time += elapsed;
        total_frames += args.frames;
        let ms = elapsed.as_secs_f64() * 1000.0 / args.frames as f64;
        println!("{:>2}. {:<34} {:>8.3} ms/frame  lit={:>3}/{}", idx, name, ms, lit, args.frames);
    }

    let avg_ms = total_time.as_secs_f64() * 1000.0 / total_frames.max(1) as f64;
    let fps = if avg_ms > 0.0 { 1000.0 / avg_ms } else { 0.0 };
    println!("Metal summary: {:>8.3} ms/frame avg  {:>7.2} FPS", avg_ms, fps);

    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn bench_metal(_args: &Args) -> Result<()> {
    anyhow::bail!("Metal benchmark is only supported on macOS")
}

fn main() -> Result<()> {
    let args = parse_args();

    match args.mode {
        Mode::Cpu => {
            bench_cpu(&args)
        }
        Mode::Metal => bench_metal(&args),
        Mode::Both => {
            bench_cpu(&args)?;
            bench_metal(&args)
        }
    }
}
