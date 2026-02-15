use std::time::{Duration, Instant};

use tui_visualizer::audio::AudioFeatures;
use tui_visualizer::config::{Quality, SwitchMode};
use tui_visualizer::visual::{make_presets, Preset, PresetEngine, RenderCtx};

fn synth_audio(t: f32, step: usize) -> AudioFeatures {
    let bass = ((t * 2.0).sin() * 0.5 + 0.5).powf(1.1);
    let mid = ((t * 3.0 + 0.4).sin() * 0.5 + 0.5).powf(1.05);
    let treb = (t * 5.0 + 1.3).sin() * 0.5 + 0.5;
    let hard = step % 24 == 0;
    let soft = step % 12 == 0;

    AudioFeatures {
        rms: (0.12 + bass * 0.40 + mid * 0.30 + treb * 0.18).clamp(0.0, 1.0),
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
        onset: if hard { 0.90 } else if soft { 0.58 } else { (treb * 0.35).clamp(0.0, 0.5) },
        beat: hard || soft,
        beat_strength: if hard { 0.95 } else if soft { 0.55 } else { 0.0 },
        centroid: (0.18 + treb * 0.62).clamp(0.0, 1.0),
        flatness: (0.12 + treb * 0.58).clamp(0.0, 1.0),
    }
}

fn has_non_black(buf: &[u8]) -> bool {
    buf.chunks_exact(4)
        .any(|px| px[0] != 0 || px[1] != 0 || px[2] != 0)
}

#[test]
fn preset_count_and_names_are_sane() {
    let presets = make_presets();
    assert!(presets.len() >= 40, "expected at least 40 presets, got {}", presets.len());

    for p in &presets {
        let name = p.name();
        assert!(!name.trim().is_empty(), "preset has empty name");
    }
}

#[test]
fn every_preset_renders_non_black_frames() {
    let mut presets: Vec<Box<dyn Preset>> = make_presets();
    let w = 96usize;
    let h = 64usize;
    let n = w * h * 4;

    for (pi, p) in presets.iter_mut().enumerate() {
        let mut prev = vec![0u8; n];
        let mut out = vec![0u8; n];
        let mut had_non_black = false;

        for f in 0..10 {
            let t = f as f32 * (1.0 / 60.0) + pi as f32 * 0.013;
            let audio = synth_audio(t, f);
            let ctx = RenderCtx {
                now: Instant::now(),
                t,
                dt: 1.0 / 60.0,
                w,
                h,
                audio,
                beat_pulse: if audio.beat { 0.9 } else { 0.0 },
                fractal_zoom_mul: 1.0,
                safe: false,
                quality: Quality::Balanced,
                scale: 1,
            };

            p.render(&ctx, &prev, &mut out);
            assert_eq!(out.len(), n);
            had_non_black |= has_non_black(&out);
            std::mem::swap(&mut prev, &mut out);
        }

        assert!(had_non_black, "preset {} ('{}') stayed fully black", pi, p.name());
    }
}

#[test]
fn adaptive_auto_mode_switches_presets() {
    let presets = make_presets();
    let mut engine = PresetEngine::new(presets, 0, false, SwitchMode::Adaptive, 4, 8.0);
    engine.resize(96, 64);

    let first = engine.preset_name();
    let mut now = Instant::now();
    let mut changed = false;

    for f in 0..360usize {
        now += Duration::from_millis(70);
        let mut a = synth_audio(f as f32 / 60.0, f);
        if f % 16 == 0 {
            a.beat = true;
            a.beat_strength = 0.92;
            a.onset = 0.86;
        }
        engine.update_auto_switch(now, &a);

        let ctx = RenderCtx {
            now,
            t: f as f32 / 60.0,
            dt: 1.0 / 60.0,
            w: 96,
            h: 64,
            audio: a,
            beat_pulse: if a.beat { 0.9 } else { 0.0 },
            fractal_zoom_mul: 1.0,
            safe: false,
            quality: Quality::Fast,
            scale: 1,
        };
        let _ = engine.render(ctx, Quality::Fast, 1);
        if engine.preset_name() != first {
            changed = true;
            break;
        }
    }

    assert!(changed, "adaptive auto-mode did not switch presets");
}
