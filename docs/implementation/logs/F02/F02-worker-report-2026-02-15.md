# F02 Worker Report (2026-02-15)

## Scope
- Updated scheduling logic only in:
  - /Users/aaaaa/Projects/tui-visualizer/src/visual/mod.rs

## What changed
- Added lightweight anti-repetition guard in transition selection (`pick_kind`):
  - Keeps existing exact-repeat avoidance.
  - Adds family-level avoidance (hard cut, glitch, morph/remix, motion, soft blend) when alternatives exist.
- Added helper `is_hard_cut` to gate back-to-back hard cuts.
- Refined `suggest_transition` auto mode heuristics:
  - Hard cuts are now sparse via deterministic slot gating (`seed` mask) and tied to strong/very-strong transients.
  - Back-to-back hard cuts are blocked unless transient is very strong.
  - Calm-section detection expanded (`beat`, `rms`, `onset`, `beat_strength`, treble), with longer durations and morph/remix-heavy weighted pool.
  - Mid/high-energy non-hard branch now favors punchy-but-smoother kinds (datamosh/wipe/radial/dissolve/prism) instead of frequent hard cuts.
  - Default low-intensity branch now biases morph/remix/fade/luma and slightly lengthens timing when hit is low.
- `suggest_manual_transition` now benefits from the anti-repetition guard through shared `pick_kind` logic.
- Transition enums, hotkeys, and mode structure were kept intact.

## Validation
Command run:
- `cargo build --release`

Result:
- `Compiling tui_visualizer v0.1.0 (/Users/aaaaa/Projects/tui-visualizer)`
- `Finished release profile [optimized] target(s) in 1m 14s`
- Exit code: `0`
