# F04/F18 Worker C (Metal) — 2026-02-15

## Scope
- Implemented Metal-side camera path expansion and reactive post-FX updates in:
  - /Users/aaaaa/Projects/tui-visualizer/src/visual/metal.rs
- Kept runtime behavior allocation-free in render/shader hot paths.

## What changed

### 1) Metal shader camera-path counterparts (orbit/dolly/helix/spiral/drift/auto)
- Expanded travel-preset detection to include deeper fractal/travel slots used in the current 56-slot map.
- Added preset-to-camera-path mapping helper with explicit modes:
  - `0=auto`, `1=orbit`, `2=dolly`, `3=helix`, `4=spiral`, `5=drift`
- Added camera path state helpers and smooth application path:
  - `camera_path_mode_for_preset(...)`
  - `camera_auto_state(...)`
  - `camera_state_for_mode(...)`
  - `apply_camera_path(...)`
- Replaced single drift/zoom block with per-stream (`q0`/`q1`) path application using transition-weighted blending (`1-alpha` / `alpha`) to prevent reset jumps across transition boundaries.

### 2) Added >=6 shader branches aligned with F18-style expansion
- Added/used branch families for camera path logic:
  - orbit, dolly, helix, spiral, drift, auto
- Preset mapping now steers known travel-heavy slots into deterministic path styles while defaulting non-explicit slots to `auto`.

### 3) Reactive post-FX improvements (terminal-safe)
- Added lightweight, clamp-safe post-FX stage:
  - adaptive saturation from energy/transient/treble
  - soft-knee highlight compression
  - subtle micro-grain
- Preserves terminal safety with existing final clamp and `u.safe`-gated intensity caps.

## Performance/allocations
- No new Rust heap allocations were introduced in the render loop.
- Shader additions are arithmetic-only and avoid extra texture fetch passes.

## Validation

### Build
- Command:
  - `cargo build --release`
- Result:
  - success (`Finished release profile`)

### Smoke run (PTY)
- Command requested:
  - `target/release/tui_visualizer --source system --engine metal --renderer kitty`
- Ran in PTY, observed active rendering + HUD with:
  - `Source: System | Engine: Metal | Renderer: kitty`
- Stopped after short smoke interval via Ctrl-C.
- Result:
  - success (clean exit)

## app.rs / config.rs integration needs
- No compile-blocking API gaps were found in current tree.
- No immediate `app.rs` or `config.rs` edits were required for this Metal-side implementation.
- If UI/config exposure of explicit camera-path selection is desired later, app/config wiring can be added as a follow-up (current implementation uses preset-mapped and auto-resolved shader-side pathing).
