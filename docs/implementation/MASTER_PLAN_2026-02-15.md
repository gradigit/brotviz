# Brotviz Master Implementation Plan (Phase: Major Feature Expansion)
Date: 2026-02-15
Status: Active

## 1) Scope locked for this phase

Must deliver:

- F00: Infinite-zoom continuity fix (no loop-back, no accidental jump cut)
- F01: Beat-synced camera system
- F02: Transition engine v2
- F03: Preset graph mode
- F04: Fractal deep-zoom mode
- F05: Scene energy auto-DJ
- F06: Reactive post-processing
- F07: Multi-band control matrix
- F10: Capability auto-probing
- F11: Performance governor
- F12: Latency-calibrated mode
- F14: Audio-reactive typography layer
- F15: Recorder/export mode (CLI audio->video output)
- F17: Theme packs
- F18: Procedural geometry bank
- F19: Camera path presets
- F20: No-HUD performance stage mode

## 2) Research inputs (completed)

- `research/study-fractal-zoom-2026-02-15.md`
- `research/study-camera-transition-2026-02-15.md`
- `research/study-audio-intelligence-2026-02-15.md`
- `research/study-postfx-geometry-2026-02-15.md`
- `research/study-preset-graph-themepacks-2026-02-15.md`
- `research/study-capability-performance-2026-02-15.md`
- `research/study-typography-layer-2026-02-15.md`
- `research/study-recorder-export-2026-02-15.md`

## 3) Master dependency graph

Top-level dependency edges:

- F00 -> F04
- F01 -> F02
- F19 -> F01
- F05 -> F02
- F07 -> F01
- F07 -> F06
- F10 -> F11
- F11 -> F20
- F12 -> F05
- F03 -> F17
- F03 -> F02
- F18 -> F03
- F14 -> F20
- F15 -> F10
- F15 -> F11

Execution waves:

- Wave A (foundation): F00, F07, F10
- Wave B (core dynamics): F01, F02, F05, F11, F12, F19
- Wave C (visual system expansion): F03, F06, F18
- Wave D (productization): F14, F17, F20
- Wave E (export): F15

## 4) Subagent execution model

Agent roles:

- `R-*` Researcher agents: done for current phase.
- `W-*` Worker implementation agents: own atomic code tasks.
- `T-*` Test agents: own suite execution, failures, and repro notes.
- `P-*` Perf agents: own benchmarks and tuning diffs.
- `D-*` Docs agents: own README/help/hotkey updates.

Required artifact from every implementation/test/perf subagent:

- `docs/implementation/logs/<feature>/<task-id>.md`
- Must include:
  - scope and files touched
  - commands run
  - outcome and blockers
  - next recommended task

## 5) Per-feature subplans

### F00 Infinite zoom continuity fix (hard gate)

Atomic tasks:

- F00-T1: Audit current zoom state variables and reset paths.
- F00-T2: Introduce monotonic fractal camera state (center, scale, velocity) independent of preset cycling.
- F00-T3: Implement continuous zoom integrator with no modulo wrap.
- F00-T4: Add anti-jump blend for parameter changes (except explicit hard-cut transitions).
- F00-T5: Add continuity tests and regression fixtures.

Task dependencies:

- F00-T1 -> F00-T2 -> F00-T3 -> F00-T4 -> F00-T5

Acceptance criteria:

- Zoom path never resets to start unless preset/transition explicitly hard-cuts.
- 20+ minute run has no spontaneous loop-back.

Tests:

- Unit: monotonic scale progression.
- Integration: long-run no-reset test.
- Visual golden: camera continuity snapshots.

Perf targets:

- < 0.5 ms/frame overhead from continuity logic.

Docs/hotkeys:

- Clarify `z`, `x`, `X`, `v` behavior as continuous camera controls.

### F01 Beat-synced camera system

Atomic tasks:

- F01-T1: Add camera modulation bus (`pulse`, `swing`, `drift`, `snap`).
- F01-T2: Map beat/onset envelopes to camera parameters.
- F01-T3: Add smoothing and hysteresis.
- F01-T4: Add camera mode presets.

Dependencies:

- F07-T* -> F01-T2
- F19-T* -> F01-T4

Acceptance criteria:

- Camera reacts to beat strength and tempo without jitter.

Tests/bench:

- Beat-reactivity functional tests.
- Camera jitter metric test.

Docs/hotkeys:

- Add camera mode hotkey and strength controls.

### F02 Transition engine v2

Atomic tasks:

- F02-T1: Transition taxonomy (`hard`, `fade`, `luma`, `morph`, `remix`, `glitch`).
- F02-T2: Transition operator interface (composable blend ops).
- F02-T3: Scheduler with beat-aware timing.
- F02-T4: Hard-cut guardrail (sparse and beat-justified only).
- F02-T5: Per-transition stepping hotkeys and lock mode.

Dependencies:

- F01 -> F02-T3
- F05 -> F02-T3

Acceptance criteria:

- No unintended jump cuts in normal modes.
- Hard cuts only on strong beat thresholds or explicit user step.

Tests:

- Operator correctness tests.
- Transition timing tests.

Perf targets:

- Transition blending overhead < 1.5 ms/frame @ baseline size.

Docs/hotkeys:

- Update transition mode and step keys in help/HUD.

### F03 Preset graph mode

Atomic tasks:

- F03-T1: Graph schema (`base`, `layers`, `post_chain`).
- F03-T2: Compile graph to runtime IR.
- F03-T3: Modulation route binding from audio matrix.
- F03-T4: Runtime safety checks (cycle guards, budget caps).
- F03-T5: Playlist integration for graph presets.

Dependencies:

- F07 -> F03-T3
- F18 -> F03-T2

Acceptance criteria:

- Multi-layer presets render deterministically.
- Graph load failures are safe and recoverable.

Tests/bench:

- Schema validation tests.
- IR compile tests.
- Graph perf budget tests.

Docs/hotkeys:

- Add graph preset authoring section.

### F04 Fractal deep-zoom mode

Atomic tasks:

- F04-T1: f64/double-single zoom state path in CPU+Metal parity.
- F04-T2: Coordinate rebasing strategy.
- F04-T3: Deep-zoom quality tier switching.
- F04-T4: Optional perturbation path (phase 2 of F04).

Dependencies:

- F00 complete required.

Acceptance criteria:

- Extended deep zoom reveals new detail rather than repeating scene.

Tests/bench:

- Deep zoom continuity tests (long horizon).
- Precision drift tests.

Perf targets:

- Maintain target FPS with adaptive quality enabled.

### F05 Scene energy auto-DJ

Atomic tasks:

- F05-T1: Implement section classifier (`calm`, `groove`, `drop`) with confidence.
- F05-T2: Add hysteresis state machine.
- F05-T3: Bind transition profile + preset family policy per section.

Dependencies:

- F07 input matrix.
- F12 latency correction.

Acceptance criteria:

- Auto mode changes style according to section energy, with low chatter.

Tests:

- Fixture-based classification tests.
- Confusion matrix tracking.

### F06 Reactive post-processing

Atomic tasks:

- F06-T1: Post-FX graph pipeline.
- F06-T2: Bloom, trail feedback, chroma shift, scanline modules.
- F06-T3: Audio bindings for FX intensity.
- F06-T4: Quality tier and safety clamping.

Dependencies:

- F07 modulation matrix.
- F11 governor for dynamic scaling.

Acceptance criteria:

- FX are visibly reactive and stable, no overblown artifact accumulation.

Tests/bench:

- Module unit tests.
- FPS regression tests by FX stack.

### F07 Multi-band control matrix

Atomic tasks:

- F07-T1: Expand feature vector (bands, centroid, flux, onset, beat, rms).
- F07-T2: Routing table (`source -> parameter`) with curve types.
- F07-T3: Smoothing filters and anti-jitter defaults.
- F07-T4: Runtime edit API + serialization.

Dependencies:

- none (foundation feature)

Acceptance criteria:

- Mappings are configurable and stable across presets.

### F10 Capability auto-probing

Atomic tasks:

- F10-T1: Probe terminal identity and graphics capability.
- F10-T2: Renderer/transport fallback ladder.
- F10-T3: Expose probe result in HUD/help.

Dependencies:

- none (foundation feature)

Acceptance criteria:

- Startup selects valid renderer path with clear reason.

### F11 Performance governor

Atomic tasks:

- F11-T1: Frame budget telemetry (engine/render/total).
- F11-T2: Adaptive controls (resolution, quality, effect LOD, skip policy).
- F11-T3: Stabilization logic to prevent oscillation.

Dependencies:

- F10 capabilities.

Acceptance criteria:

- Maintains target FPS with graceful quality changes.

### F12 Latency-calibrated mode

Atomic tasks:

- F12-T1: Latency decomposition and estimator.
- F12-T2: Calibration routine using known pulse fixture.
- F12-T3: Phase correction in beat-reactive modules.

Dependencies:

- F07 feature bus.

Acceptance criteria:

- Beat-reactive visuals align closer to audio timeline than baseline.

### F14 Audio-reactive typography layer

Atomic tasks:

- F14-T1: Typography overlay engine with anti-clutter constraints.
- F14-T2: Modes: waveform text, metadata caption, lyric cue.
- F14-T3: Readability-aware reactive mapping.

Dependencies:

- F20 stage mode interactions.

Acceptance criteria:

- Text remains readable during high-energy scenes.

### F15 Recorder/export mode (CLI audio->video)

Atomic tasks:

- F15-T1: Offscreen deterministic renderer timeline.
- F15-T2: Frame pipe to ffmpeg (`raw RGBA`).
- F15-T3: Audio ingest/mux flow.
- F15-T4: `brotviz export` CLI command and presets.
- F15-T5: Golden export tests and deterministic mode.

Dependencies:

- F10 capability abstraction.
- F11 governor settings for offline/export mode.

Acceptance criteria:

- Given audio input, produces shareable video with deterministic option.

### F17 Theme packs

Atomic tasks:

- F17-T1: Pack manifest schema/versioning.
- F17-T2: Theme/preset grouping and metadata.
- F17-T3: Pack loader and validation errors.

Dependencies:

- F03 preset graph mode.

Acceptance criteria:

- User can load/select curated packs with predictable behavior.

### F18 Procedural geometry bank

Atomic tasks:

- F18-T1: Add algorithm modules (attractor, flow field, L-system, SDF motifs).
- F18-T2: Normalize parameter API.
- F18-T3: Integrate into preset graph and transition system.

Dependencies:

- F03 runtime graph path.

Acceptance criteria:

- Geometry modules are reusable across presets and transitions.

### F19 Camera path presets

Atomic tasks:

- F19-T1: Define path library (`tunnel`, `orbit`, `spiral`, `pulse`).
- F19-T2: Path blending and retargeting.
- F19-T3: Beat-aware path switching policy.

Dependencies:

- F01 camera modulation bus.

Acceptance criteria:

- Path changes are smooth unless explicit hard-cut.

### F20 No-HUD performance stage mode

Atomic tasks:

- F20-T1: True HUD-off render path with text-layer suppression.
- F20-T2: Stage-mode defaults for quality/governor.
- F20-T3: Safe toggle and persistent preference.

Dependencies:

- F11 governor.

Acceptance criteria:

- Stage mode yields measurable perf gain vs normal HUD mode.

## 6) Test methodology and suite plan

Global suites:

- `suite_unit`: algorithm and state machine tests.
- `suite_property`: invariants (continuity, boundedness, monotonic camera conditions).
- `suite_integration`: end-to-end runtime with fixture audio.
- `suite_golden_visual`: frame snapshots/hashes for selected seeds and timestamps.
- `suite_perf`: benchmark binary + scenario matrix.
- `suite_export`: deterministic video export checksums/metadata checks.

Failure runbook set:

- FR-1: visual discontinuity / loop-back
- FR-2: transition artifact / unintended jump cut
- FR-3: latency drift
- FR-4: FPS collapse
- FR-5: renderer capability mismatch
- FR-6: export encode pipeline failure

## 7) Benchmark plan

Baseline matrix (minimum):

- Engines: CPU, Metal
- Renderers: half-block, braille, sextant, kitty
- Resolutions: 120x40, 160x54, 220x70
- Audio fixtures: pulse, smooth, transient-dense, sweep
- Modes: manual, adaptive, stage

Key metrics:

- FPS mean/p50/p95
- Frame ms (engine/render/total)
- Latency ms (now/avg/p95)
- Drops/stalls per minute
- Export throughput (frames/s)

## 8) README/help/hotkeys plan

For each feature delivery:

- Update `README.md` feature list and examples.
- Update `docs/USAGE.md` hotkeys and mode docs.
- Update `docs/ARCHITECTURE.md` data flow and module ownership.
- Update `docs/testing.md` with new suites and benchmark commands.
- Update in-app help popup and HUD key legend.

## 9) Human QA scenarios by feature class

- Continuity: long fractal zoom sessions, no loop-back.
- Musicality: calm vs drop tracks in auto-DJ mode.
- Transition quality: morph/fade vs hard-cut behavior.
- Readability: typography in high-saturation scenes.
- Performance: stage mode in large terminal sizes.
- Export quality: output sync and visual determinism.

## 10) Execution protocol

For every completed atomic task:

1. Update `TODO.md` checklist status.
2. Append evidence to `docs/implementation/TRACKER.md`:
   - tests run + result
   - benchmarks
   - bugs found/fixed
3. Add a log note in:
   - `docs/implementation/logs/<feature>/<task-id>.md`

No task is considered done without all three updates.
