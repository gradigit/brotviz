# Brotviz

Brotviz is a macOS terminal music visualizer written in Rust.
It is built for real-time use: low-latency audio analysis, a CPU and Metal visual engine, and terminal renderers that trade speed for detail.

Current focus:
- Ghostty support
- audio-reactive fractal and geometric presets
- playlist manager in-terminal
- transition system (cuts, smooth blends, morph/remix styles)

## What you need

- macOS 13+
- Rust toolchain (`cargo`)
- A terminal that supports the renderer you pick:
  - `half-block` works almost everywhere
  - `braille` gives denser text-mode output
  - `kitty` uses the Kitty graphics protocol for higher fidelity

For `--source system`, Brotviz uses ScreenCaptureKit (no virtual loopback device required).
You must grant Screen Recording permission to your terminal app in:
`System Settings -> Privacy & Security -> Screen Recording`.

## Quick start

List input devices:

```sh
cargo run --bin tui_visualizer -- --list-devices
```

Mic input, Metal engine, fast terminal-safe renderer:

```sh
cargo run --release --bin tui_visualizer -- --source mic --engine metal --renderer half-block
```

System audio (ScreenCaptureKit), Metal engine, Kitty renderer:

```sh
cargo run --release --bin tui_visualizer -- --source system --engine metal --renderer kitty
```

CPU fallback:

```sh
cargo run --release --bin tui_visualizer -- --source mic --engine cpu --renderer half-block
```

## Hotkeys

- `Left` / `Right`: previous or next preset
- `Space`: toggle auto mode
- `1..5`: switch mode (`manual`, `beat`, `energy`, `time`, `adaptive`)
- `Up` / `Down`: intensity
- `S`: shuffle on/off
- `T`: cycle transition mode
- `[` / `]`: step transition selection
- `Z`: cycle fractal zoom mode
- `V`: fractal zoom motion on/off
- `X` / `Shift+X`: zoom speed up/down
- `F`: toggle calm-section fractal bias
- `P`: open playlist manager
- `I`: show/hide HUD
- `?`, `/`, `H`, `F1`, or `Tab`: help overlay
- `Q` or `Esc`: quit

## Renderers and engines

- Engine `metal`: GPU shader path (macOS only)
- Engine `cpu`: pure CPU fallback
- Renderer `half-block`: best compatibility and speed
- Renderer `braille`: text-mode higher apparent resolution
- Renderer `kitty`: best color and detail, depends on terminal graphics support

## Performance and latency

- Always run with `--release` for real usage.
- Tune frame rate with `--fps`.
- If Kitty rendering flickers or stalls in your terminal, try `--sync-updates false`.
- The HUD shows:
  - end-to-end latency stats (`now/avg/p95`)
  - engine, render, and total frame times

Benchmark and latency docs:
- docs/testing.md

Generate deterministic latency fixture:

```sh
cargo run --release --bin gen_test_audio
```

Run benchmark:

```sh
cargo run --release --bin benchmark -- --mode both --frames 120 --w 160 --h 88 --quality balanced
```

Run offline latency report:

```sh
cargo run --release --bin latency_report -- --wav assets/test/latency_pulse_120bpm.wav --fail-over-ms 120
```

## Playlists

Playlists are persisted at:
- `$XDG_CONFIG_HOME/tui_visualizer/playlists.txt`, or
- `~/.config/tui_visualizer/playlists.txt` if `XDG_CONFIG_HOME` is unset.

The built-in `All Presets` playlist is immutable.

## Documentation

- docs/USAGE.md
- docs/ARCHITECTURE.md
- docs/testing.md
