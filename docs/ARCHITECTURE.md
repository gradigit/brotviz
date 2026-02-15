# Brotviz Architecture

## Runtime pipeline

1. Capture audio (`mic` or `system`)
2. Extract audio features (RMS, bands, onset, beat, beat strength)
3. Apply intensity scaling
4. Update preset auto-switch and transition state
5. Render pixels with selected visual engine (`cpu` or `metal`)
6. Draw frame with selected terminal renderer (`half-block`, `braille`, `kitty`)
7. Update HUD and latency telemetry

## Major modules

- src/audio.rs
  - CPAL microphone capture
  - ScreenCaptureKit system audio capture
  - ring buffer ingestion and analysis windows
- src/visual/
  - Preset registry and state machine
  - CPU preset engine
  - Metal engine and shader dispatch
  - transition/morph logic
- src/render/
  - terminal renderers
  - HUD and overlay composition
- src/app.rs
  - main loop
  - key handling
  - playlist manager
  - latency/FPS tracking

## Engine layer

Engine decides how RGBA pixels are generated.

- `cpu` engine:
  - preset math runs on CPU
  - portable fallback path

- `metal` engine (macOS):
  - shader-based preset rendering
  - better throughput at higher resolutions
  - intended default for high-fidelity visuals

## Renderer layer

Renderer decides how pixels are emitted to terminal output.

- `half-block`: 1x2 pixel mapping per cell, fastest and safest
- `braille`: 2x4 mapping per cell, denser text-mode resolution
- `kitty`: Kitty graphics protocol image transfer, best color/detail if terminal supports it

## Audio feature usage

Presets and transitions use:
- low bands for bass motion and pulse envelopes
- onset and beat strength for cut/morph timing
- spectral distribution for color shifts and structural deformation
- RMS for global energy scaling

## Transitions

Two decisions happen separately:

1. **When to switch**
   - manual / beat / energy / time / adaptive
2. **How to switch**
   - jump cuts
   - smooth fades
   - morph/remix blends
   - mode-specific transition styles

This split lets auto-mode remain musically reactive while still varying visual style.

## Playlist persistence

Playlists are text-backed and loaded at startup.

Storage path:
- `$XDG_CONFIG_HOME/tui_visualizer/playlists.txt`, or
- `~/.config/tui_visualizer/playlists.txt`

Playlist index `0` (`All Presets`) is immutable.

## Observability

HUD includes:
- current preset and mode
- transition mode/effect
- playlist name and size
- intensity and zoom controls
- FPS
- end-to-end latency (`now/avg/p95`)
- engine/render/total frame ms

These metrics are intended for quick iteration while tuning performance and reactivity.
