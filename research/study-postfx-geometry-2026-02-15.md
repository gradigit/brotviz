# Research: Feature #6 Reactive Post-Processing + Feature #18 Procedural Geometry Bank

**Date:** 2026-02-15
**Depth:** Full
**Query:** Practical and implementation-ready research for reactive post-FX and procedural geometry, including DAG/tasks, acceptance criteria, testing/benchmarking/visual-regression, performance tiers, docs/hotkeys updates, and human QA scenarios.
**Time spent:** 30+ searches, 25+ sources reviewed, 26 sources retained.

## Executive Summary

Feature #6 and Feature #18 are a strong fit for Brotviz’s architecture if implemented as a deterministic, instrumented multi-pass graph with quality tiers and strict frame-time budgeting. The most practical high-impact post-FX stack is: `Bloom -> Trails/Feedback -> Chromatic Aberration -> Scanline/CRT modulation`, each driven by a normalized audio modulation bus. For procedural geometry, add five algorithm families behind one trait-based bank: `L-systems`, `attractors`, `Gray-Scott reaction-diffusion approximation`, `SDF motifs`, and `flow fields (curl-noise + lightweight fluid advection options)`. Confidence is **high** for feasibility and performance on the existing Metal/wgpu path, with the primary risk being uncontrolled pass cost from temporal feedback and reaction-diffusion iterations.

## Question Decomposition

1. Which post-FX techniques are practical in terminal/GPU pipelines and how should they be audio-mapped?
- Use low-resolution bright-pass bloom + separable blur, ping-pong history textures for trails, lightweight per-channel UV offsets for chromatic aberration, and scanline/mask modulation as the final display-space pass.

2. Which procedural algorithms should be added, and what are their complexity/perf characteristics?
- Add the requested five families with strict per-frame work caps and deterministic seeds.

3. What implementation plan, acceptance bar, and validation strategy de-risk delivery?
- Use atomic tasks with clear dependencies, quality tiers, deterministic offline renders, GPU timestamp benchmarks, SSIM/ΔE visual regression, and a failure runbook.

## Source Quality Filter (applied)

- Tier 1 accepted: standards/specs, official docs, peer-reviewed papers, major engine docs, canonical references.
- Tier 2 accepted with caution: expert tutorials and practical implementation notes where official references are sparse.
- Rejected: SEO listicles, marketplace pages, single-author unsourced posts.

## Detailed Findings

## A) Practical post-FX for terminal/GPU pipelines

### 1) Bloom (practical path)

Recommended path:
- Render scene to HDR-ish intermediate texture.
- Bright-pass extract via threshold/knee.
- Downsample to 1/2 or 1/4 resolution.
- Separable Gaussian blur (horizontal + vertical ping-pong).
- Additively composite into scene.

Why this path:
- Separable blur reduces sample cost from O(k^2) to O(2k) for kernel width `k`, with equivalent result for Gaussian kernels (verified by both GPU Gems and LearnOpenGL) [S1][S2].
- Bloom is perceptually valuable for bright contrast cues and realism at moderate cost [S1][S26].

Practical notes:
- Keep bloom buffer at 1/2 or 1/4 resolution to cut bandwidth [S1].
- Clamp bloom iterations by quality tier.
- Use audio to modulate threshold and intensity, not blur radius every frame (avoids shimmer).

### 2) Trails / temporal feedback

Recommended path:
- Maintain a history texture (`prev_fx`) and current frame texture (`curr_fx`).
- Composite: `out = curr + prev * decay`, optionally with slight UV drift/warp.
- Swap ping-pong textures each frame.

Correctness constraints:
- Avoid read/write hazards on same texture in the same pass. OpenGL explicitly requires barriers for certain read/modify/write paths (`glTextureBarrier`) [S6].
- In wgpu/WebGPU terms, use separate attachments/bindings and legal texture usages (`TEXTURE_BINDING`, `RENDER_ATTACHMENT`) [S3][S4].

Practical audio mapping:
- `decay` inverse-mapped to onset strength (strong beat => shorter trail to avoid smear).
- Warp amplitude mapped to spectral flux / high-band energy.

### 3) Chromatic aberration

Recommended path:
- Radial offset from screen center for per-channel UV samples.
- Start with 3 samples (R/G/B offsets) and optional spectral LUT mode.

Performance note:
- Intensity increases sample footprint and can increase cost (documented in Unity post-processing notes) [S8].

Audio mapping:
- Tie aberration intensity to transient events (onset peaks) with short attack and medium release.

### 4) Scanlines / CRT-style modulation

Recommended path:
- Final-pass scanline modulation in screen space (sinusoidal row weighting + optional mask texture).
- Precompute expensive mask textures where possible; avoid per-frame heavy recomputation.

Evidence:
- Multi-pass CRT pipelines commonly separate generator/decoder/CRT phases and avoid rerunning expensive mask steps every frame [S7][S14].

Audio mapping:
- Subtle modulation only: scanline contrast ±10-20% from baseline to avoid readability collapse.

### 5) Post-FX graph design for Brotviz

Recommended pass graph:
1. `ScenePass` (existing visual output to offscreen texture)
2. `BloomExtractPass`
3. `BloomBlurPassH/V` (ping-pong)
4. `CompositePass` (scene + bloom)
5. `TrailFeedbackPass` (composite + history)
6. `ChromaticPass`
7. `ScanlineCRTPass`
8. `TerminalQuantizePass` (renderer-specific output)

Key engineering rule:
- Keep all post-FX before terminal quantization; quantize once at the end for stable text-cell output.

## B) Audio feature bus and mapping strategy

Base audio features:
- RMS / loudness proxy
- Low-band, mid-band, high-band energies (log-scaled)
- Onset envelope (spectral flux)
- Tempo/beat events

Evidence:
- `AnalyserNode` FFT size and smoothing behavior define stable frequency/time features [S9][S10].
- Onset and beat extraction methods are established in librosa docs (spectral flux onset, dynamic-programming beat tracking) [S11][S12].

Recommended normalized feature bus (`0..1`):
- `energy`, `bass`, `mid`, `treble`, `onset`, `beat_phase`, `tempo_confidence`

Mapping matrix (default):
- Bloom intensity <- `energy`
- Bloom threshold <- inverse(`bass`)
- Trail decay <- inverse(`onset`)
- Trail warp <- `treble`
- Chromatic amount <- `onset`
- Scanline contrast <- `beat_phase` (low amplitude)
- Feedback hue shift <- `mid`

Stability controls:
- Exponential smoothing with per-parameter attack/release
- Hard clamps and slew-rate limits per frame
- Beat-gated triggers for discrete toggles

## C) Procedural geometry bank (Feature #18)

Implement one common interface:

```rust
trait GeometryGenerator {
    fn name(&self) -> &'static str;
    fn seed(&mut self, seed: u64);
    fn update(&mut self, dt: f32, audio: &AudioFeatures);
    fn emit(&self, out: &mut GeometryBuffer);
}
```

### 1) L-systems

Core idea:
- Parallel rewriting grammar + turtle interpretation to produce branching structures [S13][S14].

Complexity:
- Grammar expansion can grow exponentially with iterations if uncapped.
- Runtime practical complexity per frame: O(symbol_count + segments).

Perf controls:
- Cap max symbols / max depth.
- Expand incrementally across frames.
- Cache derivation by `(axiom, ruleset, iteration, seed)`.

### 2) Attractors (Lorenz / Rössler families)

Core idea:
- Integrate ODE trajectories and render points/ribbons.

Evidence:
- Lorenz model foundational metadata and formulation context [S15].
- Rössler continuous chaos reference [S16].

Complexity:
- O(particles * integration_steps).

Perf controls:
- Fixed-step RK2/RK4 with capped steps per frame.
- Reservoir sampling / ring buffers for tail rendering.

### 3) Reaction-diffusion approximation (Gray-Scott)

Core idea:
- 2-field grid update (A/B concentrations) with reaction + diffusion, visualized as iso-bands.

Evidence:
- Complex spatiotemporal patterns from simple reaction-diffusion models [S17].
- Practical Gray-Scott implementation details and parameter maps [S18].

Complexity:
- O(W * H * iterations_per_frame).

Perf controls:
- Run at lower resolution (e.g., 128x72 to 320x180), upscale for display.
- Use 3x3 stencil, fixed iteration budget per tier.
- Optional temporal subsampling (update every 2nd frame on low tier).

### 4) SDF motifs

Core idea:
- Compose primitive distance functions with smooth boolean/warp ops; render via sphere tracing [S19][S20].

Complexity:
- O(pixels * march_steps * sdf_eval_cost).

Perf controls:
- Max march steps and early-outs.
- Reduced render resolution + upsample.
- Restrict operator depth on low tiers.

### 5) Flow fields

Core idea:
- Particle advection through incompressible-style procedural velocity fields.
- Two practical options:
1. Curl-noise-driven flow fields [S22][S23]
2. Lightweight stable-fluid-inspired velocity update when heavier dynamics are needed [S21]

Complexity:
- Curl-noise advection: O(particles * octaves)
- Grid fluid update: O(W * H * solver_iters)

Perf controls:
- Prefer curl-noise for interactive tier.
- Cap particles by renderer mode.
- Optional obstacle-aware modulation only on higher tiers.

## D) Complexity and performance summary

| Algorithm | Typical state | Per-frame complexity | Primary bottleneck | Practical cap |
|---|---|---:|---|---|
| Bloom | 1-3 textures | O(px * taps * passes) | bandwidth + sampling | 1/2-1/4 res, 2-6 blur passes |
| Trails/feedback | 2 history textures | O(px) | bandwidth | 8-bit/16-bit history + decay clamp |
| Chromatic | 1 pass | O(px * channel_samples) | sampling | 3-5 taps |
| Scanline/CRT | 1 final pass (+optional precomputed mask) | O(px) | ALU on full-screen | precompute masks |
| L-system | symbol graph + segments | O(symbols + segments) | expansion growth | max symbols/depth |
| Attractors | particle buffers | O(particles * steps) | integration | step budget |
| Reaction-diffusion | 2 grids | O(W*H*iters) | stencil bandwidth | low-res grid |
| SDF motifs | implicit scene + ray marcher | O(px*steps) | march count | max steps + LOD |
| Flow field | particles (+optional grid) | O(particles*octaves) or O(W*H*iters) | advection | particle cap/tier |

## Atomic Tasks + Dependency DAG

Task IDs:
- T01: Add GPU/CPU post-FX instrumentation hooks (frame + pass timing).
- T02: Implement post-FX graph abstraction and pass scheduler.
- T03: Add reusable ping-pong texture/history allocator.
- T04: Implement bloom extract + separable blur + composite.
- T05: Implement trails/feedback pass with decay/warp.
- T06: Implement chromatic aberration pass.
- T07: Implement scanline/CRT modulation pass.
- T08: Implement normalized audio feature bus (`energy/bands/onset/beat`).
- T09: Bind audio->postfx mapping with smoothing/clamps.
- T10: Add `GeometryGenerator` trait + registry.
- T11: Implement L-system generator.
- T12: Implement attractor generator bank.
- T13: Implement reaction-diffusion generator.
- T14: Implement SDF motif generator.
- T15: Implement flow-field generator.
- T16: Add deterministic seed plumbing + preset serialization.
- T17: Add QA/debug HUD controls for post-FX/geometry params.
- T18: Add tests + fixtures + visual regression harness.
- T19: Add benchmarks (GPU timestamps + end-to-end frame metrics).
- T20: Update docs/help/hotkeys + failure runbook.

Dependency edges:
- T01 -> T02
- T02 -> T03
- T03 -> T04
- T03 -> T05
- T02 -> T06
- T02 -> T07
- T08 -> T09
- T04 -> T09
- T05 -> T09
- T06 -> T09
- T07 -> T09
- T10 -> T11
- T10 -> T12
- T10 -> T13
- T10 -> T14
- T10 -> T15
- T11 -> T16
- T12 -> T16
- T13 -> T16
- T14 -> T16
- T15 -> T16
- T09 -> T17
- T16 -> T17
- T17 -> T18
- T17 -> T19
- T18 -> T20
- T19 -> T20

Suggested execution waves:
- Wave A: T01-T03, T08, T10
- Wave B: T04-T07, T11-T15
- Wave C: T09, T16, T17
- Wave D: T18-T20

## Acceptance Criteria

### Feature #6 Reactive Post-FX

1. Effects implemented and togglable: bloom, trails/feedback, chromatic, scanlines.
2. All effects driven by audio feature bus with bounded ranges and smoothing.
3. No read/write resource hazards in post-FX pipeline.
4. Stable output at target FPS tiers with no persistent flicker from temporal artifacts.
5. Full hotkey + help overlay coverage for post-FX controls.

### Feature #18 Procedural Geometry Bank

1. Five algorithm families available in runtime registry.
2. Deterministic output with fixed seed and identical input audio.
3. Per-algorithm guardrails prevent runaway complexity.
4. Preset system can select/switch algorithms without crashes or leaks.
5. Geometry bank integrated with existing transition/preset workflow.

## Tests, Benchmark Plan, Visual Regression, Failure Runbook

### Test Plan

Unit tests:
- Audio feature normalization and smoothing invariants.
- Mapping clamps/slew limits.
- Deterministic seed reproducibility for all generators.
- Pass graph validation (no illegal usage combinations).

Property tests:
- L-system bounded growth under caps.
- Attractor integrator finite outputs (no NaN/Inf) under parameter ranges.
- Reaction-diffusion concentrations remain bounded.
- Flow-field divergence check (for incompressible path approximations).

Integration tests:
- End-to-end scene with all post-FX enabled.
- Preset switching stress test (rapid cycling + transitions).

### Benchmark Plan

Metrics:
- Total frame time p50/p95/p99.
- Per-pass GPU time via timestamp queries [S5][S27].
- CPU prep time and upload bandwidth.

Scenarios:
- Idle low-energy audio
- Beat-heavy 120-140 BPM
- High-transient material
- Dense geometry mode + max post-FX

Budgets (initial targets):
- Low tier: <= 16.7 ms at 60 FPS equivalent
- Balanced tier: <= 13.0 ms
- High tier: <= 11.0 ms

### Visual Regression Strategy

Deterministic capture:
- Fixed seed, fixed audio fixtures, fixed dt replay.
- Capture N canonical frames per scenario.

Metrics:
- SSIM threshold for structural drift [S24].
- ΔE2000 mean/max for color drift [S25].

Gates (starter thresholds):
- SSIM >= 0.97 on balanced/high tiers.
- ΔE2000 mean <= 2.0, max <= 6.0.

Human review still required for intentional aesthetic changes.

### Failure Runbook

Symptom: frame-time spikes
- Check per-pass timestamps.
- Disable passes in order: SDF -> RD -> bloom blur count -> trails.
- Drop quality tier one level automatically if sustained >2s.

Symptom: smearing/ghost lock
- Reduce trail decay, reset history buffer, confirm ping-pong swap.

Symptom: flicker/shimmer
- Increase smoothing on mapped params.
- Reduce high-frequency modulation on threshold/UV warp.

Symptom: NaN explosions in generators
- Clamp parameters, reset state, fallback to safe preset.

Symptom: visual regressions after shader changes
- Re-run deterministic capture and compare SSIM/ΔE report.

## Performance Optimization + Quality Tiers

Tier design:

| Tier | Bloom | Trails | Chromatic | Scanline | Geometry budget |
|---|---|---|---|---|---|
| `low` | 1/4 res, 2 passes | decay only, no warp | off or very low | low strength | minimal counts, no SDF marching |
| `balanced` | 1/2 res, 4 passes | decay + light warp | 3 taps | medium | moderate counts, low-res RD |
| `high` | 1/2 res, 6 passes | full feedback | 5 taps/spectral LUT | medium-high | higher counts, capped SDF |
| `ultra` | full/1/2 hybrid | full + advanced warp | full | high | max counts + optional fluid grid |

Adaptive policy:
- Monitor p95 frame time.
- If over budget for N consecutive windows, step down one tier.
- If under budget with headroom for M windows, step up one tier.

## README / Help / Hotkeys Updates

README updates:
- New section: `Reactive Post-FX` with tier/perf notes.
- New section: `Procedural Geometry Bank` with algorithm descriptions.
- New section: `Deterministic replay + visual regression`.

Help overlay updates:
- Expose current post-FX toggles, geometry algorithm, seed, and tier.

Suggested new hotkeys (non-conflicting with current map):
- `O`: post-FX master toggle
- `B`: cycle bloom preset
- `R` / `Shift+R`: trail decay up/down
- `C` / `Shift+C`: chromatic amount up/down
- `G` / `Shift+G`: scanline strength up/down
- `Y`: feedback warp toggle
- `M`: cycle geometry family
- `N` / `Shift+N`: next/previous geometry variant
- `K`: reseed current geometry preset
- `J`: cycle quality tier

## Human QA Playlist Scenarios

1. **Kick-heavy EDM (120-130 BPM)**
- Expect punchy bloom pulses and controlled trail resets on kick.

2. **Fast DnB / breakbeat (165+ BPM)**
- Ensure onset-driven effects remain readable (no seizure-like strobing).

3. **Ambient drone / low transient**
- Expect slow drift, stable feedback, no pumping artifacts.

4. **Solo piano / wide dynamics**
- Confirm smooth transitions between quiet and loud passages.

5. **Speech/podcast**
- Ensure visuals don’t become erratic from consonant transients.

6. **Silence + noise floor**
- Confirm graceful idle mode; no random flicker.

7. **Bass sweep test file**
- Verify low-band mapping monotonicity and no clipping jumps.

8. **Synthetic click track + chirps**
- Validate beat lock and high-band mapping deterministically.

## Hypothesis Tracking

| Hypothesis | Initial Confidence | Final Confidence | Key Evidence |
|---|---|---|---|
| H1: A multi-pass post-FX chain is feasible in current GPU path at interactive frame rates | Medium | High | [S1][S3][S4][S5][S6] |
| H2: Requested geometry bank can run together with post-FX under tiered budgets | Medium | High | [S13][S17][S19][S21][S22] |
| H3: Audio-reactive mapping can be stable without overfitting to a genre | Medium | Medium-High | [S9][S10][S11][S12] |

**Resolution:** H1 and H2 are strongly supported with strict caps/tiering; H3 is supported but requires broader QA playlists and tuning.

## Verification Status

### Verified Claims (2+ independent sources)

- Separable blur drastically reduces bloom cost vs naïve 2D kernel sampling [S1][S2].
- Post-FX needs explicit offscreen render targets and pass chaining [S3][S7].
- Temporal read/modify/write paths must respect resource visibility/hazard rules [S3][S6].
- Audio FFT smoothing and analysis parameters materially affect stability [S9][S10].
- Onset/beat extraction pipeline (spectral flux + tempo/DP tracking) is established [S11][S12].
- Reaction-diffusion can produce rich patterns from simple local update rules [S17][S18].
- SDF + sphere tracing is a robust implicit geometry route [S19][S20].
- Flow fields via stable-fluid lineage and curl-noise are practical procedural options [S21][S22].

### Unverified / lower-confidence claims

- Exact best chromatic sampling pattern for this specific renderer is not externally benchmarked for Brotviz hardware.
- Optimal RD grid sizes per terminal backend need in-repo benchmarking.

### Conflicts Identified

- Curl-noise DOI appears with two variants in indexers (`1276377.1276435` vs `1275808.1276435`).
- Resolution: keep canonical bibliographic source plus DOI link and verify in code comments during implementation.

## Self-Critique

| Issue | Severity | Resolution |
|---|---|---|
| Some foundational papers are indexed but not fully open-access | Medium | Used multiple independent metadata/index references + practical open tutorials |
| Practical scanline/chromatic guidance relies partly on engine docs/tutorials | Low-Med | Kept algorithm recommendations conservative and implementation-focused |
| Hardware-specific perf numbers are absent | Medium | Added explicit benchmark plan + tiering/autoscale policy |

## Limitations & Future Research

- Needs in-repo measurement on target Macs (M1/M2/M3) before locking default tiers.
- Needs empirical validation for text-cell readability under aggressive scanline/chromatic settings.
- Future work: user-adaptive mapping profiles by genre and renderer backend.

## Source Evaluation

| ID | Source | URL | Tier | Recency | Used for |
|---|---|---|---|---|---|
| S1 | GPU Gems Ch.21 Real-Time Glow | https://developer.nvidia.com/gpugems/gpugems/part-iv-image-processing/chapter-21-real-time-glow | 1 | Foundational | Bloom architecture + separable blur cost |
| S2 | LearnOpenGL Bloom | https://learnopengl.com/Advanced-Lighting/Bloom | 2 | Active | Practical Gaussian/separable implementation notes |
| S3 | W3C WebGPU spec | https://www.w3.org/TR/webgpu/ | 1 | 2026 | Texture usage and pass constraints |
| S4 | wgpu TextureUsages docs | https://docs.rs/wgpu/latest/wgpu/struct.TextureUsages.html | 1 | 2026 | Legal texture usage flags |
| S5 | wgpu timestamp query example | https://wgpu.rs/doc/wgpu_examples/timestamp_queries/index.html | 1 | 2026 | GPU benchmarking instrumentation |
| S6 | OpenGL memory model/glTextureBarrier | https://wikis.khronos.org/opengl/Memory_Model | 1 | Maintained | Feedback/read-modify-write visibility |
| S7 | WebGPU Fundamentals post-processing | https://webgpufundamentals.org/webgpu/lessons/webgpu-post-processing.html | 2 | Active | Practical offscreen post-FX pass flow |
| S8 | Unity chromatic aberration docs | https://docs.unity.cn/560/Documentation/Manual/PostProcessing-ChromaticAberration.html | 1 | Older but stable | Chromatic parameters/perf tradeoff |
| S9 | W3C Web Audio spec (AnalyserNode) | https://www.w3.org/TR/webaudio/ | 1 | 2024/2025+ | FFT, smoothing, analyser defaults |
| S10 | MDN smoothingTimeConstant | https://developer.mozilla.org/en-US/docs/Web/API/AnalyserNode/smoothingTimeConstant | 1 | 2025 | Practical analyzer smoothing semantics |
| S11 | librosa onset_strength | https://librosa.org/doc/0.11.0/generated/librosa.onset.onset_strength.html | 1 | 2025 | Spectral flux onset details |
| S12 | librosa beat_track | https://librosa.org/doc/0.11.0/generated/librosa.beat.beat_track.html | 1 | 2025 | Dynamic-programming beat tracking |
| S13 | Algorithmic Botany publications / ABOP entry | https://algorithmicbotany.org/papers/ | 1 | Active | L-system canonical references |
| S14 | Graphical Applications of L-Systems | https://algorithmicbotany.org/papers/graphical-applications-of-l-systems.html | 1 | Foundational | L-system interpretation/generation framing |
| S15 | Lorenz metadata record | https://cds.cern.ch/record/460665 | 1 | Metadata | Lorenz 1963 citation context |
| S16 | Rössler continuous chaos paper metadata | https://www.researchgate.net/publication/223112399_An_Equation_for_Continuous_Chaos | 2 | Foundational | Attractor equation family provenance |
| S17 | Pearson 1993 reaction-diffusion paper | https://pubmed.ncbi.nlm.nih.gov/17829274/ | 1 | Foundational | RD complexity/behavior evidence |
| S18 | Karl Sims Gray-Scott tutorial | https://www.karlsims.com/rd.html | 2 | Stable | Practical Gray-Scott approximations |
| S19 | Hart sphere tracing metadata | https://dblp.org/rec/journals/vc/Hart96 | 1 | Foundational | Sphere tracing/SDF rendering basis |
| S20 | hg_sdf library notes | https://mercury.sexy/hg_sdf/ | 2 | 2021 updates | SDF composition/operator practice |
| S21 | Stable Fluids metadata | https://dblp.org/rec/conf/siggraph/Stam99a | 1 | Foundational | Fluid-inspired flow updates |
| S22 | Curl-noise metadata | https://dblp.org/rec/journals/tog/BridsonHN07 | 1 | Foundational | Procedural incompressible flow fields |
| S23 | Differentiable Curl-Noise | https://uwspace.uwaterloo.ca/items/e2d52165-cb1f-4876-b5a8-f7356d4a8209 | 1 | 2023 | Modern curl-noise improvements |
| S24 | SSIM paper metadata | https://pubmed.ncbi.nlm.nih.gov/15376593/ | 1 | Foundational | Structural visual regression metric |
| S25 | CIEDE2000 implementation notes | https://hajim.rochester.edu/ece/sites/gsharma/ciede2000/ | 1 | Foundational | Color-difference regression metric |
| S26 | Unreal bloom docs | https://dev.epicgames.com/documentation/en-us/unreal-engine/bloom-in-unreal-engine | 1 | Current | Bloom realism/perf framing |
| S27 | wgpu CommandEncoder docs | https://docs.rs/wgpu/latest/wgpu/struct.CommandEncoder.html | 1 | 2026 | Query resolve/timestamp API surface |

## Rejected / Not used

- Marketplace/tutorial sales pages and low-detail blogs (insufficient technical depth).
- Wikipedia entries used only for discovery, not for implementation claims.
- Unverifiable duplicated DOI mirror pages.

