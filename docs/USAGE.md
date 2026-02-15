# Brotviz Usage Guide

## Basic commands

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

CPU engine fallback:

```sh
cargo run --release --bin tui_visualizer -- --source mic --engine cpu --renderer braille
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
- `--sync-updates true|false`
- `--safe true|false`

## Hotkeys

- `Left` / `Right`: preset step
- `Space`: toggle auto
- `1..5`: switch mode
- `S`: shuffle
- `T`: transition mode
- `[` / `]`: transition effect step
- `Up` / `Down`: intensity
- `Z`: fractal zoom mode
- `V`: zoom motion on/off
- `X` / `Shift+X`: zoom speed
- `F`: fractal bias
- `P`: playlist manager
- `I`: HUD on/off
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

## System audio notes

`--source system` uses ScreenCaptureKit.
Grant Screen Recording permission to your terminal app in:

`System Settings -> Privacy & Security -> Screen Recording`

No BlackHole loopback device is required for the default ScreenCaptureKit path.

## Troubleshooting

Blank or no updates with Kitty renderer:
- Try `--sync-updates false`
- Confirm Kitty graphics protocol support in your terminal
- Fall back to `half-block` or `braille` to isolate renderer-specific issues

High latency:
- Run `--release`
- Lower `--fps` or `--quality`
- Prefer `half-block` when testing latency-sensitive behavior

Unexpected CLI errors:
- Use exact renderer value `half-block` (alias `halfblock` is also supported)
- If multiple binaries exist, always include `--bin tui_visualizer`
