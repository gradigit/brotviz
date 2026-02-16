use std::time::{Duration, Instant};

use anyhow::Result;
use tui_visualizer::audio::AudioFeatures;
use tui_visualizer::config::{Quality, SwitchMode};
use tui_visualizer::visual::{make_presets, CameraPathMode, PresetEngine, RenderCtx, VisualEngine};

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
    switch_frames: usize,
    camera_frames: usize,
    w: usize,
    h: usize,
    quality: Quality,
    scale: usize,
    safe: bool,
    ci_smoke: bool,
    quick: bool,
    max_ms: f64,
}

fn parse_args() -> Args {
    let mut args = Args {
        mode: Mode::Cpu,
        frames: 180,
        switch_frames: 72,
        camera_frames: 60,
        w: 160,
        h: 88,
        quality: Quality::Balanced,
        scale: 1,
        safe: false,
        ci_smoke: false,
        quick: false,
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
            ("--switch-frames", Some(x)) => {
                if let Ok(n) = x.parse::<usize>() {
                    args.switch_frames = n.max(1);
                }
                i += 2;
            }
            ("--camera-frames", Some(x)) => {
                if let Ok(n) = x.parse::<usize>() {
                    args.camera_frames = n.max(1);
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
            ("--quick", Some(x)) if !x.starts_with("--") => {
                args.quick = parse_bool(x).unwrap_or(true);
                i += 2;
            }
            ("--quick", _) => {
                args.quick = true;
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

    if args.quick {
        args.frames = args.frames.min(60);
        args.switch_frames = args.switch_frames.min(48);
        args.camera_frames = args.camera_frames.min(36);
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

fn section_audio(step: usize, total_steps: usize) -> AudioFeatures {
    let t = step as f32 / 60.0;
    let mut audio = synth_audio(t, step);
    let section = (step.saturating_mul(4)) / total_steps.max(1);
    match section {
        0 => {
            audio.rms = (0.08 + audio.rms * 0.22).clamp(0.0, 1.0);
            audio.onset = (audio.onset * 0.25).clamp(0.0, 1.0);
            audio.beat = step % 48 == 0;
            audio.beat_strength = if audio.beat { 0.35 } else { 0.0 };
        }
        1 => {
            audio.rms = (0.18 + audio.rms * 0.40).clamp(0.0, 1.0);
            audio.onset = (audio.onset * 0.55 + 0.1).clamp(0.0, 1.0);
            audio.beat = step % 24 == 0;
            if audio.beat {
                audio.beat_strength = 0.58;
            }
        }
        2 => {
            audio.rms = (0.28 + audio.rms * 0.55).clamp(0.0, 1.0);
            audio.onset = (audio.onset * 0.7 + 0.16).clamp(0.0, 1.0);
            audio.beat = step % 16 == 0 || audio.beat;
            if audio.beat {
                audio.beat_strength = audio.beat_strength.max(0.72);
            }
        }
        _ => {
            audio.rms = (0.38 + audio.rms * 0.60).clamp(0.0, 1.0);
            audio.onset = (audio.onset * 0.85 + 0.24).clamp(0.0, 1.0);
            audio.beat = step % 12 == 0 || audio.beat;
            if audio.beat {
                audio.beat_strength = audio.beat_strength.max(0.88);
            }
        }
    }
    audio
}

fn set_camera_path_mode(engine: &mut PresetEngine, target: CameraPathMode) {
    for _ in 0..6 {
        if engine.camera_path_mode() == target {
            return;
        }
        engine.step_camera_path_mode(true);
    }
}

fn bench_section_aware_switching(args: &Args) {
    let mut engine = PresetEngine::new(make_presets(), 0, false, SwitchMode::Adaptive, 4, 8.0);
    engine.resize(args.w, args.h);

    let frames = args.switch_frames.max(1);
    let mut now = Instant::now();
    let start = Instant::now();
    let mut lit = 0usize;
    let mut switches = 0usize;
    let mut last_name = engine.preset_name().to_string();

    for f in 0..frames {
        now += Duration::from_millis(40);
        let audio = section_audio(f, frames);
        engine.update_auto_switch(now, &audio);
        let ctx = RenderCtx {
            now,
            t: f as f32 / 60.0,
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
        let px = engine.render(ctx, args.quality, args.scale);
        if px.chunks_exact(4).any(|p| p[0] != 0 || p[1] != 0 || p[2] != 0) {
            lit += 1;
        }
        let name = engine.preset_name();
        if name != last_name {
            switches = switches.saturating_add(1);
            last_name = name.to_string();
        }
    }

    let ms = start.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    println!(
        "CPU section-aware switch: {:>8.3} ms/frame  switches={:>2}  section={}  final={}  lit={:>3}/{}",
        ms,
        switches,
        engine.scene_section_name(),
        engine.preset_name(),
        lit,
        frames
    );
}

fn bench_camera_path_modes(args: &Args) {
    let mut engine = PresetEngine::new(make_presets(), 0, false, SwitchMode::Manual, 16, 20.0);
    engine.resize(args.w, args.h);

    let frames = args.camera_frames.max(1);
    let modes = [
        CameraPathMode::Auto,
        CameraPathMode::Orbit,
        CameraPathMode::Dolly,
        CameraPathMode::Helix,
        CameraPathMode::Spiral,
        CameraPathMode::Drift,
    ];
    println!(
        "CPU camera-path benchmark: modes={} frames/mode={} size={}x{}",
        modes.len(),
        frames,
        args.w,
        args.h
    );
    for (mi, mode) in modes.iter().copied().enumerate() {
        set_camera_path_mode(&mut engine, mode);

        let start = Instant::now();
        let mut lit = 0usize;
        for f in 0..frames {
            let t = f as f32 / 60.0;
            let audio = synth_audio(t + mi as f32 * 0.031, f + mi * 17);
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
            let px = engine.render(ctx, args.quality, args.scale);
            if px.chunks_exact(4).any(|p| p[0] != 0 || p[1] != 0 || p[2] != 0) {
                lit += 1;
            }
        }

        let ms = start.elapsed().as_secs_f64() * 1000.0 / frames as f64;
        println!(
            "  {:<6} {:>8.3} ms/frame  lit={:>3}/{}",
            mode.label(),
            ms,
            lit,
            frames
        );
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
        "CPU benchmark: presets={} frames/preset={} size={}x{} quality={:?} scale={} quick={}",
        presets.len(),
        args.frames,
        args.w,
        args.h,
        args.quality,
        args.scale,
        args.quick
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
    bench_section_aware_switching(args);
    bench_camera_path_modes(args);

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
