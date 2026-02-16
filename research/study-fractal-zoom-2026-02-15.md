# Study: Fractal Zoom Continuity Bug + Feature #4 Deep-Zoom Mode (Brotviz)
Date: 2026-02-15
Depth: Full

## Executive summary
- The current loop-back/jump behavior is deterministic and primarily caused by explicit phase wrapping via `.fract()`/`fract()` in both CPU and Metal zoom paths (`src/visual/presets.rs:767`, `src/visual/presets.rs:852`, `src/visual/metal.rs:899`, `src/visual/metal.rs:1779`).
- A continuity-safe fix is to replace wrapped phase zoom with a monotonic `log2_zoom` camera state (no modulo/reset), then derive scale as `exp2(-log2_zoom)`.
- For Feature #4 deep-zoom mode, the best long-term architecture for this codebase is hybrid: high-precision reference orbit on CPU (MPFR via `rug`) + low-precision perturbation on GPU/CPU render paths with glitch detection and rebasing.
- Recommended delivery is phased: ship continuity fix first (low risk), then deep-zoom v1 (double-single + rebasing), then deep-zoom v2 (perturbation + series/BLA acceleration).

## 1) Problem analysis: why loop-back happens now
### Direct code findings
- CPU path uses wrapped phase:
  - `phase = (...) .fract()` in `fractal_zoom_motion` (`src/visual/presets.rs:767`)
  - `phase = (...) .fract()` in deep Mandelbrot/Julia/BurningShip zoom (`src/visual/presets.rs:852`, `src/visual/presets.rs:913`, `src/visual/presets.rs:1009`)
- Metal path mirrors same behavior:
  - `phase = fract(...)` in `deep_zoom_pow` (`src/visual/metal.rs:899`)
  - camera motion phase wrap in transition camera zoom (`src/visual/metal.rs:1779`)
- Wrapped phase then feeds smoothstep and exponential zoom (`src/visual/presets.rs:853-856`, `src/visual/metal.rs:900-901`, `src/visual/metal.rs:1360`), so each cycle returns from max zoom to min zoom.

### Algorithmic cause
- `fract(x)` is defined as `x - floor(x)` in both Rust and GLSL specs, so it is periodic with discontinuities at integer boundaries. This guarantees loop-back when `x` grows monotonically. Sources: [E1], [E2].  
  Inference: if `phase = fract(k*t)`, then `phase` resets 1→0 every period.
- Because zoom is computed from wrapped `phase`, zoom is bounded and cyclic rather than unbounded/deep.
- Audio-coupled drift terms are also phase-coupled, so reset events can look like camera jumps, not only simple zoom breathing.

### Secondary contributors
- `f32` precision limits (`MANTISSA_DIGITS = 24`) make very deep coordinates unstable; cancellation and rounding amplify visible instability when subtracting nearly equal magnitudes. Sources: [E1], [E3].
- This is not the main loop-back cause, but it becomes the next blocker once wrap is removed.

## 2) Research-backed implementation options
## Option A: continuity-only camera fix (fastest path)
- Replace wrapped phase with persistent camera state:
  - `log2_zoom += speed * dt * audio_drive`
  - `scale = exp2(-log2_zoom)`
  - `center += drift_velocity * dt * scale`
- Keep rendering math mostly unchanged.
- Pros:
  - Minimal code churn.
  - Immediately removes deterministic loop-back.
- Cons:
  - Deep zoom depth still limited by floating-point precision.
- Research basis:
  - Wrapped phase behavior from `fract` definitions [E1], [E2].
  - Continuous easing support from `smoothstep` spec for anti-pop envelopes [E2].

## Option B: double-single / f64 camera with rebasing (mid complexity)
- Use `f64` for camera state in Rust and pass hi/lo split floats (double-single) to Metal.
- Rebase center/orbit state periodically to keep local deltas small.
- Pros:
  - Extends usable zoom depth significantly before arbitrary precision is required.
  - Good real-time fit for terminal frame budgets.
- Cons:
  - Still finite depth.
  - More complexity in CPU/GPU parity and serialization of camera uniforms.
- Research basis:
  - Precision limits and cancellation behavior [E1], [E3].
  - Rebasing as practical anti-glitch mechanism [E6], [E7], [E9].

## Option C: perturbation + reference orbit + glitch criterion (deep-zoom canonical)
- Compute one high-precision reference orbit, render neighboring pixels as low-precision deltas.
- Detect perturbation breakdown (“glitches”), then create/rebase to new reference.
- Pros:
  - Standard approach for very deep Mandelbrot/Julia zooms.
  - Matches requirement “no loop-back/jump” while enabling deep exploration.
- Cons:
  - New subsystem (reference generation, cache, glitch rounds).
  - Requires async work scheduling and fallback paths.
- Research basis:
  - Reference orbit + low-precision deltas [E8], [E9].
  - Glitch criterion and rebasing [E6], [E9].

## Option D: series/BLA acceleration for perturbation (for real-time viability)
- Precompute approximation coefficients and skip iteration spans when validity bounds hold.
- Pros:
  - Major speed-up at high iteration counts.
  - Critical for keeping frame-time predictable in deep mode.
- Cons:
  - Additional validity logic and tables.
- Research basis:
  - Series approximation details [E10].
  - Bivariate linear approximation and validity bounds [E7].

## Option E: tile/pyramid fallback under frame pressure
- Keep multi-resolution cache; refine center tiles first; display lower LOD if budget exceeded.
- Pros:
  - Prevents hard stutter under heavy deep-zoom workloads.
  - Improves perceived continuity in terminal rendering.
- Cons:
  - Cache/LOD management complexity.
- Research basis:
  - Image-pyramid layer model and smooth layer transitions [E11].
  - Mipmap/LOD quality-performance trade-off [E12].

## Continuity constraints and anti-jump transitions (applies to all options)
- Enforce camera invariants:
  - `log2_zoom` monotonic in forward mode.
  - screen-center world coordinate continuity across reference switches.
  - bounded per-frame change in zoom derivative.
- Use smoothed parameter updates (e.g., spring/critical damping for user toggles) rather than hard resets.
- Research basis:
  - Exponential spring semantics for zoom animation [E13].
  - Smooth Hermite interpolation behavior via `smoothstep` [E2].

## 3) Recommended implementation for this codebase
## Recommendation: phased hybrid approach
- Phase 1 (ship first): Option A + continuity invariants.
- Phase 2: Option B for practical extra depth and stable continuity in real time.
- Phase 3 (Feature #4 full): Option C + Option D with optional Option E fallback.

## Why this matches Brotviz specifically
- Brotviz has both CPU and Metal engines (`docs/ARCHITECTURE.md`) and active fractal controls (`README.md`, `docs/USAGE.md`), so parity and smooth UX are required.
- Existing deep presets are currently artistic cyclic dives, not true deep zoom (`src/visual/presets.rs:278`, `src/visual/metal.rs:1357`).
- Existing benchmark and test scaffolding can be extended rather than replaced (`src/bin/benchmark.rs`, `tests/presets_suite.rs`).

## Proposed architecture
- New module: `src/visual/deep_zoom.rs`
- Core state:
  - `log2_zoom: f64`
  - `center_re/center_im: f64` (or hi/lo pair for GPU)
  - `reference_epoch: u64`
  - `precision_bits: u32` (deep mode only)
  - `glitch_threshold: f32`
- Render integration:
  - CPU path consumes same camera state.
  - Metal uniforms include camera state and optional perturbation/reference buffers.
- Scheduler:
  - Async reference orbit worker (MPFR via `rug`) at adaptive precision.
  - Main render loop always has a valid previous reference to avoid frame stalls.

## 4) Atomic tasks + dependency DAG
| ID | Task | Depends on |
|---|---|---|
| T1 | Add `deep_zoom` state model and serialization types | - |
| T2 | Replace `.fract()` zoom phase with monotonic `log2_zoom` in CPU path | T1 |
| T3 | Replace `fract()` zoom phase with monotonic `log2_zoom` in Metal shader/uniforms | T1 |
| T4 | Add continuity invariants (no hard reset; center continuity on mode switch) | T2, T3 |
| T5 | Add HUD/debug fields (`log2_zoom`, reference epoch, glitch ratio) | T4 |
| T6 | Add deep-zoom mode enum and input plumbing (hotkeys + CLI flag if desired) | T4 |
| T7 | Implement f64/double-single coordinate mapping helpers | T4 |
| T8 | Introduce asynchronous reference-orbit worker using `rug` | T7 |
| T9 | Implement perturbation delta evaluator (CPU first for correctness oracle) | T8 |
| T10 | Port perturbation evaluator to Metal path | T9 |
| T11 | Implement glitch detection + rebasing/new-reference selection | T9, T10 |
| T12 | Add series/BLA acceleration path and validity checks | T11 |
| T13 | Add LOD/tile fallback under frame budget pressure | T10 |
| T14 | Docs/help/readme/hotkey updates | T6, T12 |

## DAG (text form)
`T1 -> T2 -> T4 -> T6 -> T14`  
`T1 -> T3 -> T4`  
`T4 -> T7 -> T8 -> T9 -> T10 -> T11 -> T12 -> T14`  
`T10 -> T13`

## 5) Acceptance criteria
- No periodic loop-back in deep presets over 10 minutes of continuous run.
- No frame-to-frame jump at former phase boundaries.
- In forward deep-zoom mode, `log2_zoom` never decreases unless user explicitly reverses.
- Switching references/rebasing does not move screen-center world coordinate beyond tolerance.
- Metal and CPU engines produce visually consistent camera motion for same seed/audio stream.
- Deep mode remains interactive at target terminal size in `--release`.

## 6) Testing methodology
## Test suite structure
- `tests/deep_zoom_continuity.rs`
- `tests/deep_zoom_rebase.rs`
- `tests/deep_zoom_perturbation.rs`
- Extend `tests/presets_suite.rs` for deep-mode coverage.
- Extend `src/bin/benchmark.rs` scenarios for deep zoom.

## Concrete test cases
1. Deterministic no-loop test:
   - fixed audio input, 60k frames
   - assert monotonic `log2_zoom`
   - assert no sawtooth reset pattern
2. Continuity under mode toggles:
   - toggle deep mode on/off and zoom mode cycling
   - assert center continuity and bounded derivative spikes
3. Reference switch continuity:
   - force frequent rebasing
   - assert center pixel complex coordinate error below threshold
4. Glitch detector sanity:
   - synthetic cases near perturbation validity boundary
   - verify detector fires and recompute path resolves artifacts
5. CPU/Metal parity:
   - same camera/reference state; compare scalar summary metrics and sampled pixels
6. Long-run stability:
   - 30-minute run, no NaN/Inf propagation, no panic, bounded memory growth

## Failure runbook
| Symptom | Likely cause | Immediate action | Long-term fix |
|---|---|---|---|
| Zoom still loops | hidden `.fract()`/mod in camera path | log camera variables each frame | remove all wrapped phase from camera math |
| Hard jump on rebase | center remap not preserving world center | freeze frame and dump pre/post camera state | enforce world-center invariance transform |
| Blob/noise patches | perturbation glitch undetected | lower glitch threshold, force recompute round | calibrate criterion and multi-reference scheduling |
| FPS collapse | precision/ref generation on render thread | move work to async worker and cap per-frame uploads | add BLA/series and LOD fallback |
| CPU/Metal divergence | mismatch in formulas or precision split | compare per-step intermediate states | unify shared math constants and update order |

## 7) Performance costs, optimization paths, and benchmarks
## Expected cost by phase
- Phase 1 (continuity-only): near-zero runtime cost.
- Phase 2 (double-single/f64 camera): moderate ALU increase in fractal coordinate setup.
- Phase 3 (perturbation): highest complexity; dominated by reference generation, buffer updates, and high-iteration kernels.

## Optimization paths
- Asynchronous reference computation and double-buffered orbit uploads.
- Adaptive precision bits from zoom depth (minimum precision that preserves continuity target).
- Series/BLA skip tables for long iteration spans.
- LOD/tile refinement when frame budget exceeded.
- Metal tuning:
  - function specialization and reduced branch complexity [E14], [E16]
  - occupancy-aware kernel tuning and counter-driven profiling [E15], [E16]

## Benchmark plan
- Add benchmark modes:
  - `continuity_only`
  - `deep_zoom_ds` (double-single)
  - `deep_zoom_perturbation`
- Measure:
  - frame time (p50/p95/p99)
  - FPS stability
  - reference generation latency
  - glitch recompute rate
  - memory footprint over long runs
- Environments:
  - `--engine cpu` and `--engine metal`
  - representative terminal resolutions and quality tiers

## 8) README/help/hotkey update suggestions
- README and `docs/USAGE.md`:
  - clarify distinction:
    - legacy cyclic zoom effects
    - new true deep-zoom mode (continuous, no loop-back)
  - add performance notes for deep mode and quality trade-offs.
- Help overlay/HUD:
  - show deep-zoom status, precision bits, reference epoch, glitch ratio.
- Hotkey suggestions:
  - keep existing `Z` cycle but include deep mode profile.
  - add dedicated deep toggle (example: `G`).
  - add reference refresh/recenter key (example: `R`).
  - add precision/budget adjust keys (example: `,`/`.`) if exposed.

## 9) Human visual QA checklist (“looks good” + “continuous zoom”)
- No visible snapback to wider-scale structures during sustained forward zoom.
- Motion feels directionally consistent during high-energy audio spikes.
- No single-frame positional pop when deep mode toggles or reference updates.
- Filament/detail flow remains coherent near center over long runs.
- Transition effects do not mask continuity regressions in deep mode.
- CPU and Metal look qualitatively similar for same camera seed/audio.
- HUD values (`log2_zoom`, ref epoch) correlate with what the eye sees.

## Hypothesis tracking
| Hypothesis | Confidence | Supporting evidence | Contradicting evidence |
|---|---|---|---|
| H1: Loop-back is primarily from phase wrap (`fract`) | High | Code paths + fract semantics [E1], [E2] | None found |
| H2: Precision limits become next blocker once wrap is removed | High | `f32` precision + cancellation behavior [E1], [E3] | None found |
| H3: Perturbation/rebasing is best path for true deep zoom | High | Independent implementations/docs [E6], [E8], [E9] | Higher implementation cost |
| H4: Series/BLA needed for real-time deep mode | Medium-High | Theory + production practice docs [E7], [E10] | Can defer in early milestones |

## Verification matrix (2+ independent sources per concrete claim)
| Claim | Source A | Source B |
|---|---|---|
| `fract` is `x-floor(x)` (periodic with reset points) | [E1] | [E2] |
| Current Brotviz zoom path is phase-wrapped in CPU and Metal | `src/visual/presets.rs` | `src/visual/metal.rs` |
| Single-precision limits deep coordinate fidelity | [E1] | [E3] |
| Arbitrary precision + correct rounding is available via MPFR/rug | [E4] | [E5] |
| Deep zoom via high-precision reference + low-precision deltas | [E8] | [E9] |
| Glitch detection and rebasing/new references are required in perturbation workflows | [E6] | [E9] |
| Series/BLA can skip iterations with validity constraints | [E7] | [E10] |
| Pyramid/LOD tiling supports smooth multi-scale rendering behavior | [E11] | [E12] |
| Spring/exponential zoom interpolation is established anti-jump pattern | [E13] | [E2] |
| Metal optimization should be occupancy/counter guided | [E15] | [E16] |

## Limitations and gaps
- No public, formal proof source was found for all practical glitch criteria thresholds; current practice is empirical ([E6], [E9]).
- Exact Brotviz deep-zoom performance numbers are not measured in this report; benchmark plan is defined but not executed here.

## Sources
### Internal codebase
- `src/visual/presets.rs`
- `src/visual/metal.rs`
- `src/visual/mod.rs`
- `README.md`
- `docs/USAGE.md`
- `docs/ARCHITECTURE.md`
- `tests/presets_suite.rs`

### External primary/official references
- [E1] Rust `f32` docs (fract, mantissa digits): https://doc.rust-lang.org/std/primitive.f32.html
- [E2] OpenGL GLSL 4.60 spec (`fract`, `smoothstep`): https://registry.khronos.org/OpenGL/specs/gl/GLSLangSpec.4.60.html
- [E3] Goldberg, *What Every Computer Scientist Should Know About Floating-Point Arithmetic*: https://docs.oracle.com/cd/E19059-01/stud.10/819-0499/ncg_goldberg.html
- [E4] GNU MPFR manual: https://www.mpfr.org/mpfr-current/mpfr.html
- [E5] Rust `rug` docs: https://docs.rs/rug/latest/rug/
- [E6] mathr, *Deep zoom theory and practice*: https://mathr.co.uk/blog/2021-05-14_deep_zoom_theory_and_practice.html
- [E7] mathr, *Deep Zoom* (perturbation, rebasing, BLA): https://mathr.co.uk/web/deep-zoom.html
- [E8] Ultra Fractal perturbation calculations: https://www.ultrafractal.com/help/formulas/perturbationcalculations.html
- [E9] DeepDrill perturbation theory: https://dirkwhoffmann.github.io/DeepDrill/docs/Theory/Perturbation.html
- [E10] DeepDrill series approximation: https://dirkwhoffmann.github.io/DeepDrill/docs/Theory/SeriesApproximation.html
- [E11] OpenSeadragon TileSource (image pyramid): https://openseadragon.github.io/docs/OpenSeadragon.TileSource.html
- [E12] Microsoft Learn, mipmaps overview: https://learn.microsoft.com/en-us/windows/uwp/graphics-concepts/texture-filtering-with-mipmaps
- [E13] OpenSeadragon Spring (exponential zoom animation): https://openseadragon.github.io/docs/OpenSeadragon.Spring.html
- [E14] Apple Tech Talk 111373 (Metal shader perf best practices): https://developer.apple.com/videos/play/tech-talks/111373/
- [E15] Apple WWDC20 10603 (GPU counters, occupancy): https://developer.apple.com/videos/play/wwdc2020/10603/
- [E16] Apple WWDC23 10127 (function specialization, compute tuning): https://developer.apple.com/videos/play/wwdc2023/10127/
