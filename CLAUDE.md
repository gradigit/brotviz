# Brotviz (tui_visualizer) — Claude Session Guide

## Project summary

- Rust terminal visualizer for macOS (Ghostty-focused)
- Audio sources: microphone (`cpal`) and system audio (`ScreenCaptureKit`)
- Visual engines: CPU + Metal
- Renderers: `half-block`, `braille`, `sextant`, `ascii`, `kitty`

## Core run commands

```sh
cargo run --release --bin tui_visualizer -- --source mic --engine metal --renderer half-block
cargo run --release --bin tui_visualizer -- --source system --engine metal --renderer kitty
cargo run --bin tui_visualizer -- --list-devices
```

## Supporting binaries

```sh
cargo run --release --bin benchmark -- --mode both --frames 120 --w 160 --h 88 --quality balanced
cargo run --release --bin gen_test_audio
cargo run --release --bin latency_report -- --wav assets/test/latency_pulse_120bpm.wav --fail-over-ms 120
cargo run --release --bin export_video -- --audio assets/test/latency_pulse_120bpm.wav --out out/demo.mp4 --width 1280 --height 720 --fps 60 --duration 12
```

## Key files

- `src/app.rs`: main loop, key handling, HUD/help/menus, selector and playlist UI, runtime tuning
- `src/audio.rs`: mic + system capture and feature extraction
- `src/visual/mod.rs`: CPU preset engine and trait surface
- `src/visual/metal.rs`: Metal engine, shader source, transition/camera/fractal logic
- `src/visual/presets.rs`: procedural preset algorithms and fractal paths
- `src/render/`: renderer backends and overlay drawing
- `src/config.rs`: CLI flags/options
- `docs/USAGE.md`: runtime behavior + hotkeys

## Current phase

- Stability and UX polish on top of the feature-complete visualizer stack
- Current focus:
  - hotkey/input reliability in Ghostty/stage-mode workflows
  - renderer consistency for promo/demo captures

## Known gotchas

- `--source system` requires Screen Recording permission for terminal app
- `kitty` renderer depends on terminal graphics support and can fail on some terminals/IO modes
- Generated media in `out/` and `promo/` can be large; avoid accidental staging
- Prefer explicit binary in commands when needed: `--bin tui_visualizer`

## Immediate next step

- Validate all interactive hotkeys in a real Ghostty session after recent input-loop retry fix in `src/app.rs`
