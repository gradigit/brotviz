# Brotviz Master Tracker
Date initialized: 2026-02-15

## A) Test results tracker

| Date | Feature/Task | Suite | Command | Result | Notes |
|---|---|---|---|---|---|
| 2026-02-15 | bootstrap | smoke | `cargo build --release` | PASS | baseline compile after planning |
| 2026-02-15 | F00-T2/T3/T4 | build | `cargo build --release` | PASS | zoom helpers and deep zoom path switched to monotonic non-reset progression |
| 2026-02-15 | F00-T2/T3/T4 | runtime smoke | `target/release/tui_visualizer --source system --engine metal --renderer kitty` | PASS | app entered render loop, HUD active, no shader/runtime errors on startup |
| 2026-02-15 | F01-T1/T2/T3 | build | `cargo build --release` | PASS | beat/onset-smoothed camera modulation integrated in CPU + Metal paths |
| 2026-02-15 | F01-T1/T2/T3 | runtime smoke | `target/release/tui_visualizer --source system --engine metal --renderer kitty` | PASS | PTY run renders frames with HUD and live FPS/latency; clean interrupt |
| 2026-02-15 | F02-T3/T4 | build | `cargo build --release` | PASS | transition auto-scheduler refined; anti-repetition + hard-cut gating active |
| 2026-02-15 | F20-T1/T2 | build | `cargo build --release` | PASS | stage mode flag/hotkey + HUD/overlay-off path + governor profile compiled cleanly |
| 2026-02-15 | F20-T1/T2 | runtime smoke | `target/release/tui_visualizer --source system --engine metal --renderer kitty --stage-mode` | PASS | stage-mode startup renders visual-only frame path with no HUD text |
| 2026-02-15 | full integration | regression suites | `cargo test` | PASS | unit + integration suites all green (`config_modules_suite` 16, `export_suite` 6, `presets_suite` 6 incl. deep-zoom continuity) |
| 2026-02-15 | full integration | benchmark | `cargo run --release --bin benchmark -- --mode cpu --frames 120 --w 160 --h 96 --quality fast` | PASS | 62 presets rendered; avg 1.580 ms/frame (632.97 FPS) |
| 2026-02-15 | full integration | latency | `cargo run --release --bin latency_report -- --wav assets/test/latency_pulse_120bpm.wav --fail-over-ms 140` | PASS | p95=17.3ms, 0 misses, 0 false positives |
| 2026-02-15 | full integration | runtime smoke (TTY) | `target/release/tui_visualizer --source mic --engine metal --renderer kitty --stage-mode` | PASS | entered render loop in true TTY; clean interrupt |
| 2026-02-15 | full integration | runtime smoke (headless) | `target/release/tui_visualizer --source system --engine metal --renderer kitty --stage-mode` | FAIL (env) | expected in headless/no-display environment: `no displays found (ScreenCaptureKit)` |

## B) Benchmark results tracker

| Date | Feature/Task | Scenario | Metric | Baseline | Current | Delta | Status |
|---|---|---|---|---|---|---|---|
| 2026-02-15 | baseline | CPU benchmark fast quality (160x96) | avg FPS | TBD | 632.97 | n/a | complete |
| 2026-02-15 | F00 zoom continuity patch | system+metal+kitty startup sample | fps (HUD sample) | TBD | 53.8 | TBD | sample-only |
| 2026-02-15 | F01 camera modulation patch | system+metal+kitty startup sample | fps (HUD sample) | TBD | 52.1 | TBD | sample-only |
| 2026-02-15 | F01/F02 integrated pass | system+metal+kitty startup sample | fps (HUD sample) | TBD | 52.3 | TBD | sample-only |
| 2026-02-15 | F20 stage mode | system+metal+kitty startup sample | fps (HUD sample) | TBD | n/a (HUD hidden by design) | TBD | sample-only |
| 2026-02-15 | full integration | CPU benchmark fast quality (160x96) | avg ms/frame | TBD | 1.580 | n/a | complete |
| 2026-02-15 | full integration | latency pulse fixture | p95 latency (ms) | <= 140 target | 17.3 | +122.7 margin | complete |

## C) Bugs and fixes tracker

| Date | ID | Feature | Symptom | Root cause | Fix | Validation | Status |
|---|---|---|---|---|---|---|---|
| 2026-02-15 | B-000 | F00 | Fractal zoom loops back / jumps | `.fract()` phase reset in CPU+Metal zoom/camera math | replaced with monotonic log-progress zoom plus continuous drift; deep zoom power now non-reset | build PASS + runtime smoke PASS + `deep_zoom_fractal_motion_has_no_large_reset_spikes` PASS | closed |
| 2026-02-15 | B-001 | F02 | Excessive hard-cut feel in auto | hard-transition selection too permissive in auto heuristic | added hard-cut gating + calm-section smoothing + anti-repetition family guard | build PASS; human listening/visual QA pending | mitigated |
| 2026-02-15 | B-002 | Runtime env | system capture unavailable in headless runs | ScreenCaptureKit requires active display target | documented fallback to `--source mic` for headless validation | reproduced and documented | closed |

## D) Failure runbook index

| Runbook ID | Trigger | Immediate action | Deep fix owner |
|---|---|---|---|
| FR-1 | visual discontinuity / zoom reset | switch to safe transition mode + disable hard cuts | F00/F04 owner |
| FR-2 | unintended jump cuts | set transition mode `smooth`, lock transition selection | F02 owner |
| FR-3 | latency drift | run calibration routine and inspect audio buffer mode | F12 owner |
| FR-4 | FPS collapse | enable governor safe profile / stage mode | F11/F20 owner |
| FR-5 | renderer mismatch | force renderer fallback and log probe output | F10 owner |
| FR-6 | export pipeline failure | run export dry-run and ffmpeg diagnostics | F15 owner |

## E) Required log artifacts per completed task

Each completed task must include a task log:

- `docs/implementation/logs/<feature>/<task-id>.md`

Template:

```md
# <task-id>
Date:
Owner:

## Scope
- files touched:

## Commands run
- ...

## Result
- ...

## Tests/bench evidence
- ...

## Follow-up
- ...
```
