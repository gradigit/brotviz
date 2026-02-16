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

System audio + synced lyrics file + opt-in local system-data typography feed:

```sh
cargo run --release --bin tui_visualizer -- \
  --source system --engine metal --renderer kitty \
  --lyrics-file assets/samples/yankee_doodle_choral.lrc \
  --system-data creep
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
- `C`: cycle camera path mode
- `,` / `.`: camera path speed down/up
- `S`: shuffle on/off
- `T`: cycle transition mode
- `[` / `]`: step transition selection
- `Z`: cycle fractal zoom mode
- `V`: fractal zoom motion on/off
- `X` / `Shift+X`: zoom speed up/down
- `F`: toggle calm-section fractal bias
- `Y`: toggle typography on/off
- `Shift+Y`: cycle typography style (`line`, `word`, `glyph`, `matrix`)
- `L`: toggle latency auto-calibration
- `-` / `=`: latency offset down/up (ms)
- `0`: reset manual latency offset
- `P`: open playlist manager
- `M`: open theme selector menu
- `O`: open preset graph selector menu
- `K`: open lyrics selector menu
- `U`: open typography selector menu
- `;`: cycle system-data feed (`off -> subtle -> creep`)
- `I`: show/hide HUD
- `G`: stage mode toggle (persisted in prefs)
- `?`, `/`, `H`, `F1`, or `Tab`: help overlay
- `Q` or `Esc`: quit

Typography is rendered inside the visualizer frame (not only HUD text):
- Typography is currently **experimental (WIP)**.
- `line`: scrolling `BROTVIZ` ribbon pulses
- `word`: beat-synced center word pulses
- `glyph`: orbiting neon glyph swarm
- `matrix`: reactive alphanumeric rain

Lyrics + system-data typography inputs:
- `--lyrics-file` injects synced lyric lines into HUD + in-frame typography
- `--system-data subtle|creep` injects local-only tokens (`USER/HOST/CWD/home names`) for psychedelic overlays
- No network calls are made for these feeds; data stays local in-process

## Renderers and engines

- Engine `metal`: GPU shader path (macOS only)
- Engine `cpu`: pure CPU fallback
- Renderer `half-block`: best compatibility and speed
- Renderer `braille`: text-mode higher apparent resolution
- Renderer `kitty`: best color and detail, depends on terminal graphics support
- Startup capability probing (`--auto-probe true`) will auto-fallback:
  - `renderer kitty` -> `half-block` if Kitty graphics capability is missing
  - `engine metal` -> `cpu` if Metal is unavailable

## Performance and latency

- Always run with `--release` for real usage.
- Tune frame rate with `--fps`.
- If Kitty rendering flickers or stalls in your terminal, try `--sync-updates false`.
- The HUD shows:
  - end-to-end latency stats (`now/avg/p95`)
  - latency calibration status (`manual/auto/effective` offset)
  - capability probe status and fallback reason
  - engine, render, and total frame times

Latency calibration flags:
- `--latency-calibration` enables dynamic offset estimation
- `--latency-offset-ms <f32>` adds manual phase offset (also adjustable with hotkeys)

Optional runtime integration files:
- `--theme-pack <path>`
- `--control-matrix <path>`
- `--preset-graph <path>`
- `--lyrics-file <path>` (`.lrc` preferred, plain text also supported)
- `--lyrics-loop true|false`
- `--lyrics-offset-ms <f32>`
- `--system-data off|subtle|creep` (local-only tokens for typography overlays)

If parse fails, Brotviz continues and surfaces warnings in HUD/help.

## Starter theme/graph packs

Bundled samples are in:
- `assets/theme/`
- `assets/graph/`

Included theme packs:
- `psychedelic-journey.theme` (broad colorful journey)
- `fractal-infinite-dive.theme` (fractal-heavy set)
- `lowlight-ambient.theme` (smoother, subtler tracks)
- `percussive-glitch-punch.theme` (hard-beat/glitch emphasis)

Included preset graphs:
- `narrative-arc.graph`
- `fractal-descent.graph`
- `percussive-cutup.graph`

Sample audio + lyrics pair (for typography review):
- `assets/samples/yankee_doodle_choral.ogg`
- `assets/samples/yankee_doodle_choral.lrc`

Playback + visualizer review example:

```sh
afplay assets/samples/yankee_doodle_choral.ogg &
cargo run --release --bin tui_visualizer -- \
  --source system --engine metal --renderer kitty \
  --lyrics-file assets/samples/yankee_doodle_choral.lrc \
  --lyrics-loop true \
  --system-data subtle
```

You can load them via CLI flags or select them live in-app:
- `M` opens theme selection
- `O` opens graph selection
- `U` opens typography selection
- In selector popups: `Up/Down` move, `Enter/Space` apply, `Tab/Left/Right` switch selector group.

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

Offline video export (WAV -> MP4 via `ffmpeg`):

```sh
cargo run --release --bin export_video -- \
  --audio assets/test/latency_pulse_120bpm.wav \
  --out out/latency_export.mp4 \
  --width 1280 --height 720 --fps 60 \
  --preset "Mandelbrot" \
  --engine metal \
  --duration 12
```

Notes:
- `ffmpeg` must be available in your `PATH`.
- `--engine metal` falls back to CPU automatically if Metal is unavailable.
- Preset selection accepts either an index (`--preset 3`) or a case-insensitive substring.

## Playlists

Playlists are persisted at:
- `$XDG_CONFIG_HOME/tui_visualizer/playlists.txt`, or
- `~/.config/tui_visualizer/playlists.txt` if `XDG_CONFIG_HOME` is unset.

The built-in `All Presets` playlist is immutable.

Stage mode preference is persisted at:
- `$XDG_CONFIG_HOME/tui_visualizer/prefs.txt`, or
- `~/.config/tui_visualizer/prefs.txt`

## Documentation

- docs/USAGE.md
- docs/ARCHITECTURE.md
- docs/testing.md

## Repo hygiene

The following are treated as local working artifacts and are intentionally not tracked:
- `TODO.md`
- `docs/implementation/`
- `research/`
