# Brotviz Usage Guide

## Basic commands

Homebrew install (released binary, macOS Apple Silicon):

```sh
brew tap gradigit/tap
brew install brotviz
```

Run installed binary:

```sh
brotviz --source mic --engine metal --renderer half-block
```

Mic input:

```sh
cargo run --release --bin tui_visualizer -- --source mic --engine metal --renderer half-block
```

System audio input:

```sh
cargo run --release --bin tui_visualizer -- --source system --engine metal --renderer half-block
```

Kitty renderer:

```sh
cargo run --release --bin tui_visualizer -- --source system --engine metal --renderer kitty
```

System audio + lyrics + local system-data typography:

```sh
cargo run --release --bin tui_visualizer -- \
  --source system --engine metal --renderer kitty \
  --lyrics-file assets/samples/yankee_doodle_choral.lrc \
  --system-data creep
```

CPU engine fallback:

```sh
cargo run --release --bin tui_visualizer -- --source mic --engine cpu --renderer braille
```

Offline exporter (WAV -> MP4):

```sh
cargo run --release --bin export_video -- \
  --audio assets/test/latency_pulse_120bpm.wav \
  --out out/demo.mp4 \
  --width 1280 --height 720 --fps 60 \
  --preset "Mandelbrot" \
  --engine metal \
  --duration 15
```

List devices:

```sh
cargo run --bin tui_visualizer -- --list-devices
```

## Important flags

- `--source mic|system`
- `--engine cpu|metal`
- `--renderer half-block|braille|kitty`
- `--fps <N>`
- `--quality fast|balanced|high|ultra`
- `--adaptive-quality true|false`
- `--switch manual|beat|energy|time|adaptive`
- `--shuffle true|false`
- `--preset <index-or-substring>`
- `--stage-mode true|false`
- `--auto-probe true|false`
- `--latency-calibration true|false`
- `--latency-offset-ms <f32>`
- `--theme-pack <path>`
- `--control-matrix <path>`
- `--preset-graph <path>`
- `--lyrics-file <path>`
- `--lyrics-loop true|false`
- `--lyrics-offset-ms <f32>`
- `--system-data off|subtle|creep`
- `--sync-updates true|false`
- `--safe true|false`

Exporter-specific flags:
- `--audio <wav>`
- `--out <mp4-path>` (default: `export.mp4`)
- `--width <N>`
- `--height <N>`
- `--fps <N>`
- `--duration <seconds>` (optional cap)
- `--preset <index-or-substring>`
- `--engine cpu|metal` (`metal` auto-falls back to CPU when unavailable)

## Hotkeys

- `Left` / `Right`: preset step
- `Space`: toggle auto
- `1..5`: switch mode
- `S`: shuffle
- `T`: transition mode
- `[` / `]`: transition effect step
- `C`: camera path mode
- `,` / `.`: camera path speed down/up
- `Up` / `Down`: intensity
- `Z`: fractal zoom mode
- `V`: zoom motion on/off
- `X` / `Shift+X`: zoom speed
- `F`: fractal bias
- `Y`: typography layer mode
- Typography features are currently experimental (WIP).
- `L`: latency auto-calibration
- `-` / `=`: latency offset down/up
- `0`: reset manual latency offset
- `P`: playlist manager
- `K`: lyrics selector popup
- `I`: HUD on/off
- `G`: stage mode toggle (persisted)
- `;`: cycle system-data feed (`off -> subtle -> creep`)
- `?`, `/`, `H`, `F1`, `Tab`: help on/off
- `Q`, `Esc`, `Ctrl+C`: quit

## Playlist manager

Open with `P`.

- `Tab`, `Left`, `Right`: switch pane
- `Up`, `Down`: move cursor
- `Enter`: apply playlist (left) or toggle preset membership (right)
- `Space`: toggle preset membership
- `N`: create playlist from current active selection
- `A` / `R`: add/remove highlighted preset
- `X` / `D`: delete selected playlist (except `All Presets`)
- `Esc` / `P`: close

Saved file path:
- `$XDG_CONFIG_HOME/tui_visualizer/playlists.txt`, or
- `~/.config/tui_visualizer/playlists.txt`

Stage mode preference path:
- `$XDG_CONFIG_HOME/tui_visualizer/prefs.txt`, or
- `~/.config/tui_visualizer/prefs.txt`

## System audio notes

`--source system` uses ScreenCaptureKit.
Grant Screen Recording permission to your terminal app in:

`System Settings -> Privacy & Security -> Screen Recording`

No BlackHole loopback device is required for the default ScreenCaptureKit path.

## Lyrics + typography sample files

Bundled review files:
- `assets/samples/yankee_doodle_choral.ogg`
- `assets/samples/yankee_doodle_choral.lrc`

Try:

```sh
afplay assets/samples/yankee_doodle_choral.ogg &
cargo run --release --bin tui_visualizer -- \
  --source system --engine metal --renderer kitty \
  --lyrics-file assets/samples/yankee_doodle_choral.lrc \
  --lyrics-loop true \
  --lyrics-offset-ms 0 \
  --system-data subtle
```

System-data feed notes:
- `--system-data subtle` uses a low-rate local token feed (masked user/host + context fields)
- `--system-data creep` increases token variety (includes visible home-directory names)
- data is read locally only and never transmitted by Brotviz

## Troubleshooting

Blank or no updates with Kitty renderer:
- Try `--sync-updates false`
- Confirm Kitty graphics protocol support in your terminal
- Fall back to `half-block` or `braille` to isolate renderer-specific issues
- Or keep `--auto-probe true` for startup fallback

High latency:
- Run `--release`
- Lower `--fps` or `--quality`
- Prefer `half-block` when testing latency-sensitive behavior
- Enable `--latency-calibration` and tune `--latency-offset-ms` (or `-` / `=` hotkeys)

Unexpected CLI errors:
- Use exact renderer value `half-block` (alias `halfblock` is also supported)
- If multiple binaries exist, always include `--bin tui_visualizer`
