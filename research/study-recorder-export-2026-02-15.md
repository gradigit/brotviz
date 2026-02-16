# Study Report: Feature #15 Recorder/Export Mode (Audio Input -> Video Output)
Date: 2026-02-15
Depth: Full
Method: Structured decomposition, quality filtering, cross-verification, hypothesis tracking, self-critique

## Executive summary
Recommended implementation path: build an offline `export` CLI path that reuses the existing `VisualEngine -> RGBA` pipeline, drives it from deterministic audio playback, and streams raw RGBA frames to an `ffmpeg` subprocess for encoding/muxing.

Why this path is best:
1. It matches current architecture boundaries (`app` drives engine, `Frame` already exposes `pixels_rgba`, render loop already tracks timing).
2. It avoids adding libav FFI complexity in the first version.
3. It supports deterministic golden testing with `ffmpeg` frame hashes and fixture audio.

Confidence: High for the base architecture and ffmpeg pipeline details; Medium for exact performance targets on all machines until benchmarks run.

## Scope assumptions
1. “Recorder/export mode” means a non-interactive CLI workflow that takes an audio file input and writes a video file output.
2. V1 target is deterministic offline rendering first; live “record while playing” can be layered later.
3. Existing preset/transition behavior should remain compatible unless a deterministic override is enabled.

## Question decomposition
1. What architecture options exist for offscreen rendering and export?
2. What ffmpeg integration strategy is safest for RGBA frame piping and audio muxing?
3. How should CLI UX and intermediate file formats be designed for reliability and determinism?
4. What dependency-ordered task breakdown minimizes implementation risk?
5. Which acceptance criteria, tests, runbook, and benchmark method make this feature release-ready?

## Source quality table
| ID | Source | Type | Quality | Recency | Used for |
|---|---|---|---|---|---|
| S1 | https://ffmpeg.org/ffmpeg.html | Official docs | Tier 1 | Current | option order, `-map`, `-shortest`, timestamp/timebase, `-fps_mode`, `-bitexact` |
| S2 | https://ffmpeg.org/ffmpeg-formats.html | Official docs | Tier 1 | Current | `rawvideo`, `image2pipe`, `framecrc`, `framehash`, mov/mp4 fragmentation flags |
| S3 | https://ffmpeg.org/ffmpeg-protocols.html | Official docs | Tier 1 | Current | `pipe:` protocol, blocksize behavior, seekability caveat (MOV on pipe) |
| S4 | https://ffmpeg.org/ffprobe.html | Official docs | Tier 1 | Current | machine-readable verification (`-show_format`, `-show_streams`, JSON output) |
| S5 | https://docs.rs/wgpu/latest/wgpu/struct.TexelCopyBufferLayout.html | API docs | Tier 1 | Current | offscreen copy/readback layout constraints (`bytes_per_row`, 256 alignment) |
| S6 | https://jsonlines.org/ | Format spec | Tier 1 | 2025-12 build | newline-delimited metadata/event log format |
| S7 | https://docs.asciinema.org/manual/asciicast/v2/ | Official docs | Tier 1 | Current | header + event-stream file structure for deterministic replay logs |
| S8 | https://docs.rs/clap/latest/clap/trait.Parser.html | API docs | Tier 1 | Current | parser pattern for subcommand UX in existing clap setup |
| S9 | https://docs.rs/clap/latest/clap/trait.Subcommand.html | API docs | Tier 1 | Current | subcommand modeling (`record`, `export`) |
| S10 | https://docs.rs/clap/latest/clap/trait.ValueEnum.html | API docs | Tier 1 | Current | stable enum-driven CLI options |
| S11 | https://bheisler.github.io/criterion.rs/book/analysis.html | Maintainer docs | Tier 2 | Current | benchmark warmup/measurement/outlier methodology |
| S12 | https://docs.rs/symphonia/latest/symphonia/ | API docs | Tier 1 | Current | pure-Rust demux/decode option for multi-format audio |
| S13 | https://docs.rs/hound/latest/hound/ | API docs | Tier 1 | Current | simple WAV encode/decode fallback |
| S14 | /Users/aaaaa/Projects/tui-visualizer/docs/ARCHITECTURE.md | Local primary | Tier 1 | 2026-02-15 | runtime boundaries and module ownership |
| S15 | /Users/aaaaa/Projects/tui-visualizer/src/render/mod.rs | Local primary | Tier 1 | 2026-02-15 | `Frame` + `pixels_rgba` contract |
| S16 | /Users/aaaaa/Projects/tui-visualizer/src/app.rs | Local primary | Tier 1 | 2026-02-15 | current render loop and frame pacing structure |
| S17 | /Users/aaaaa/Projects/tui-visualizer/docs/testing.md | Local primary | Tier 1 | 2026-02-15 | deterministic fixture + benchmark/report conventions |
| S18 | /Users/aaaaa/Projects/tui-visualizer/src/config.rs | Local primary | Tier 1 | 2026-02-15 | clap config/value-enum conventions |

## Verified claims (2+ independent sources)
1. Raw frame ingest requires explicit input parameters (frame rate, size, pixel format) for raw input workflows. Verified by S1 and S2.
2. ffmpeg option ordering is critical, and options apply to the next input/output file. Verified by S1 and command examples in S2/S3.
3. Streaming to `pipe:` is supported, but some formats (especially MOV-like) require seekable outputs, so container choice matters. Verified by S2 and S3.
4. ffmpeg supports packet/frame hash outputs (`framecrc`, `framehash`, `framemd5`) suitable for deterministic golden tests. Verified by S2 and S4 tooling for machine-readable inspection.
5. JSONL + header/event stream format is practical for append-only deterministic logs. Verified by S6 and S7.
6. Existing code already surfaces RGBA frame buffers and a timing/render loop that can be repurposed for export mode. Verified by S14, S15, S16.

## Hypothesis tracking
| Hypothesis | Initial confidence | Final confidence | Evidence |
|---|---|---|---|
| H1: Reuse current RGBA engine output and pipe to ffmpeg is fastest path to production | High | High | S14, S15, S16, S1, S2, S3 |
| H2: Two-pass frame-sequence export is safer than piping for V1 | Medium | Low-Med | S2 (`image2pipe` exists), but storage overhead and extra I/O are high |
| H3: In-process libav bindings should be V1 | Medium | Low | No strong need given S1-S4 CLI coverage; increases complexity |
| H4: Deterministic replay needs an intermediate event/timeline log | Medium | High | S6, S7 + existing deterministic fixture workflow in S17 |

## Architecture options

### Option A (recommended): Offscreen engine + deterministic timeline + ffmpeg stdin pipe
Flow:
1. Decode audio file into sample stream (Symphonia/Hound path).
2. Build deterministic analysis timeline (features per frame or per analysis hop).
3. Drive existing visual engine offscreen with fixed `t = frame_idx / fps` and deterministic seed.
4. Emit RGBA frames to ffmpeg stdin (`-f rawvideo -pix_fmt rgba -video_size WxH -framerate FPS -i pipe:0`).
5. Feed original audio file as second ffmpeg input and mux with explicit `-map` + `-shortest`.

Pros:
1. Minimal architectural drift from current runtime.
2. High throughput, low disk churn.
3. Strongly testable with frame hashes.

Cons:
1. ffmpeg process management and backpressure handling required.
2. Container/codec edge cases must be handled.

### Option B: Frame sequence first (PNG/EXR or raw chunks), then encode
Flow:
1. Render deterministic frames to disk.
2. Run second pass ffmpeg encode + mux.

Pros:
1. Easier debugging (inspect raw frames).
2. Can resume encode step independently.

Cons:
1. Massive temporary I/O and storage.
2. Slower end-to-end for most runs.

### Option C: In-process libav* binding pipeline
Flow:
1. Integrate libavformat/libavcodec directly.
2. Write frames/audio in-process.

Pros:
1. Single process, no subprocess IPC.
2. Potentially finer-grained control.

Cons:
1. Large implementation and maintenance cost.
2. More portability and build complexity risk.

### Option D: Event-log recorder only, separate exporter
Flow:
1. Record deterministic event log (`.jsonl` + metadata).
2. Export in second command.

Pros:
1. Strong reproducibility and debuggability.
2. Enables quick “record once, export many presets/codecs.”

Cons:
1. Adds one more format and conversion step.

### Recommendation
Ship Option A first, with Option D data model support (lightweight export manifest) so replay/export can be separated later without redesign.

## Offscreen renderer and deterministic playback design

### Offscreen rendering contract
Current renderer boundary already includes:
1. `Frame { pixel_width, pixel_height, pixels_rgba, ... }` (S15).
2. Main loop already computes deterministic frame context fields (`t`, `dt`, `w`, `h`) before `engine.render(...)` (S16).

V1 export path should bypass terminal renderer and directly consume RGBA frame output from engine.

### Deterministic timeline mode
Deterministic mode rules:
1. Fixed FPS timeline: `t = frame_idx / fps`.
2. Fixed timestep `dt = 1 / fps`.
3. Fixed seed for all stochastic transitions/effects.
4. Disable adaptive quality in export mode unless explicitly requested.
5. Optional fixed quality profile to avoid quality-driven divergence.

Deterministic artifacts:
1. `export_manifest.json` (parameters, seed, fps, dimensions, audio fingerprint).
2. Optional `timeline.jsonl` (per-frame metadata and key feature values).

### Readback alignment note
If/when a wgpu offscreen path is added, texture copy layouts must honor `bytes_per_row` alignment constraints (multiple of 256 for copy operations), which affects raw frame buffer packing (S5).

## ffmpeg integration blueprint

### Canonical raw RGBA + audio mux command
```bash
ffmpeg \
  -f rawvideo -pix_fmt rgba -video_size ${W}x${H} -framerate ${FPS} -i pipe:0 \
  -i "${AUDIO_IN}" \
  -map 0:v:0 -map 1:a:0 \
  -c:v libx264 -pix_fmt yuv420p \
  -c:a aac \
  -shortest \
  "${OUT}.mp4"
```

Why this shape:
1. Input options are attached to the next input (`pipe:0`), which ffmpeg explicitly requires by option-order semantics (S1).
2. Raw input requires explicit dimensions/pixfmt/framerate (S2).
3. Explicit stream maps avoid auto-selection surprises (S1).
4. `-shortest` ends when shortest stream ends and helps avoid tail mismatches (S1).

### Container/seekability policy
1. File outputs (`.mp4`, `.mkv`) are straightforward.
2. When output target is non-seekable (stdout/pipe), avoid plain MOV/MP4 defaults; ffmpeg notes seekability caveats for some formats (S3).
3. For streaming-ish mp4, fragmented flags are available (`movflags` such as `frag_keyframe`, etc.) (S2).

### Deterministic encode knobs (best effort)
1. Use ffmpeg `-bitexact` where practical for (de)mux/(de/en)coder stability (S1).
2. Keep encoder/thread settings stable within CI to reduce cross-run drift.
3. Use frame hash outputs (`framehash`/`framemd5`/`framecrc`) for golden comparisons (S2).

## CLI UX and file format options

### CLI shape (clap-aligned)
Current code already uses clap `Parser` + `ValueEnum` patterns (S18, S8, S10).

Recommended command model:
```text
tui_visualizer export --audio INPUT --out OUTPUT [options]
tui_visualizer export-plan --audio INPUT --plan OUT.json
```

Recommended options:
1. `--audio <path>` input audio file.
2. `--out <path>` output video path.
3. `--width <u32>` and `--height <u32>` output dimensions.
4. `--fps <u32>` deterministic frame rate.
5. `--engine <cpu|metal>` same semantics as runtime.
6. `--quality <fast|balanced|high|ultra>` fixed quality for deterministic exports.
7. `--preset <name|index>` optional fixed preset.
8. `--switch <manual|beat|energy|time|adaptive>` optional, default deterministic-safe mode.
9. `--seed <u64>` deterministic seed.
10. `--codec <h264|hevc|vp9|av1|prores>` high-level profile selector mapped to ffmpeg args.
11. `--audio-codec <aac|opus|copy>` audio handling.
12. `--overwrite` explicit output clobber.
13. `--dry-run` print generated ffmpeg command and config.
14. `--emit-manifest <path>` export metadata and timing stats.
15. `--emit-timeline <path.jsonl>` optional per-frame timeline.

### File format options
Option 1 (recommended): `manifest + optional JSONL timeline`
1. `manifest.json`: single JSON metadata file (config, versions, seed, audio hash).
2. `timeline.jsonl`: one JSON object per frame (frame idx, t, preset id, beat flags, feature snapshots).
3. Final video file (`.mp4` default, `.mkv` optional).

Option 2: Asciicast-like event stream adaptation
1. Keep first-line header + subsequent event lines approach (S7).
2. Use custom event codes for visual state changes.
3. Useful if future integration with existing cast tooling is desired.

Option 3: Packed binary timeline
1. Smaller and faster.
2. Worse debuggability and tooling ergonomics.

Recommendation: Option 1 for V1.

## Atomic tasks and dependency DAG

### Task list
| ID | Task | Depends on | Output |
|---|---|---|---|
| T1 | Define `ExportConfig` and clap subcommand schema | - | CLI parse model |
| T2 | Implement audio decode abstraction (`AudioInput`) | T1 | PCM sample iterator |
| T3 | Implement deterministic timeline clock (`frame_idx -> t`) | T1 | fixed scheduler |
| T4 | Implement deterministic feature extractor path | T2,T3 | per-frame features |
| T5 | Add `ExportRunner` orchestrator | T1,T3,T4 | frame production loop |
| T6 | Add `FrameProducer` using existing engine interface | T5 | RGBA frame stream |
| T7 | Add ffmpeg command builder module | T1 | validated arg builder |
| T8 | Add ffmpeg subprocess writer (stdin RGBA pipe) | T6,T7 | encoded video output |
| T9 | Add audio mux integration (2nd ffmpeg input + mapping) | T8 | A/V output |
| T10 | Add manifest writer | T5 | `manifest.json` |
| T11 | Add optional timeline JSONL writer | T5 | `timeline.jsonl` |
| T12 | Add structured export metrics/logging | T8,T9 | stage timing + counters |
| T13 | Add deterministic regression test fixtures | T4,T10 | stable fixtures |
| T14 | Add golden frame hash integration test | T8,T13 | hash-verified outputs |
| T15 | Add ffprobe validation test (`streams`, `duration`, `fps`) | T9 | media correctness test |
| T16 | Add error taxonomy + runbook mapping | T8,T9 | diagnosable failures |
| T17 | Add benchmark harness for export throughput | T8,T9,T12 | reproducible perf report |
| T18 | Update docs (`README`, `USAGE`, `testing`) and help text | T1..T17 | user-facing docs |

### DAG edges
```text
T1 -> T2,T3,T7
T2,T3 -> T4
T1,T3,T4 -> T5
T5 -> T6,T10,T11
T1,T7,T6 -> T8
T8 -> T9,T12,T16
T4,T10 -> T13
T8,T13 -> T14
T9 -> T15
T8,T9,T12 -> T17
T1..T17 -> T18
```

## Acceptance criteria
1. `export` command takes an audio file and produces a playable video with audio.
2. Output has exactly one video stream and one audio stream (unless `--audio-codec none`).
3. Export obeys requested width, height, fps, engine, quality, preset/seed settings.
4. Deterministic mode: two runs with identical inputs/settings produce identical frame hashes.
5. Failures return actionable errors (ffmpeg missing, unsupported audio format, broken pipe, etc.).
6. Documentation includes copy-paste examples and troubleshooting table.
7. Performance targets (below) are met on baseline hardware profile.

## Automated tests, golden tests, and failure runbook

### Automated tests
1. Unit tests for ffmpeg argument generation (including edge cases for mappings and container/codec combinations).
2. Unit tests for deterministic scheduler (`frame_idx`, `t`, `dt` correctness).
3. Unit tests for manifest/timeline serialization schemas.
4. Integration test: run short export fixture, assert process success, output file exists, non-zero duration.
5. Integration test: ffprobe JSON parse checks (`-show_format`, `-show_streams`, JSON output) (S4).
6. Integration test: run `framehash`/`framemd5` against output and compare with golden baseline (S2).
7. Regression test: run same export twice with same seed and assert identical hash output.
8. Regression test: different seed produces non-identical hash output (sanity check that seed is active).

### Golden test strategy
1. Keep short deterministic fixture audio (already aligned with current fixture philosophy in S17).
2. Use low-ish resolution test profile for CI speed (e.g., 320x180 at 30 fps for 5-10s).
3. Store expected `framemd5` (or `framehash`) artifacts in test fixtures.
4. Re-generate goldens only via explicit update command.

### Failure runbook
| Symptom | Likely cause | Diagnostic | Remediation |
|---|---|---|---|
| `ffmpeg` spawn fails | binary missing / PATH issue | check process launch error | add preflight check, clear install hint |
| Export hangs mid-run | stdin backpressure / deadlock | monitor child stderr + write loop | use bounded writer loop + stderr reader thread |
| No audio in output | bad stream mapping | inspect ffmpeg args, run ffprobe streams | enforce explicit `-map 0:v:0 -map 1:a:0` |
| Output duration mismatch | timestamp/sync config | ffprobe durations + frame count | adjust `-shortest`, fps mode/timebase |
| Non-deterministic golden hashes | adaptive behavior or seed leak | compare manifests, settings, seed | force deterministic mode defaults |
| Broken pipe during encode | ffmpeg exited early due invalid args/codec | capture child exit + stderr | surface stderr in user-facing error |
| Corrupted colors | pix_fmt mismatch | inspect ffmpeg input args | standardize on `rgba` ingress |
| Offscreen copy panic (future wgpu path) | row alignment issue | validate `bytes_per_row` | enforce 256-aligned row strides (S5) |

## Performance costs and throughput targets

### Raw frame throughput cost
Formula:
```text
bytes_per_second = width * height * 4 * fps
```

Reference workloads:
| Profile | Raw RGBA throughput |
|---|---|
| 1280x720 @ 30 | 110,592,000 B/s (~105.5 MiB/s) |
| 1600x900 @ 60 | 345,600,000 B/s (~329.6 MiB/s) |
| 1920x1080 @ 60 | 497,664,000 B/s (~474.6 MiB/s) |

Implication: raw pipe bandwidth is substantial; avoid unnecessary frame copies and keep frame buffer reuse tight.

### Throughput targets (initial)
1. 1280x720@30 export speed >= 1.5x realtime on baseline dev hardware.
2. 1600x900@60 export speed >= 0.75x realtime.
3. 1920x1080@60 export speed >= 0.5x realtime.
4. Deterministic mode dropped-frame count = 0 (offline export).
5. Per-stage p95 timings tracked for: decode, feature extract, render, pipe write, encode wait.

### Benchmark methodology
1. Use existing benchmark conventions as baseline (S17) and extend with export-specific bench command.
2. Run warmup + measurement phases (Criterion-style structure: warmup, sampled measurement, outlier awareness) (S11).
3. Measure at least 5 runs per profile, report median and p95.
4. Capture ffmpeg internal timing with `-benchmark_all` where practical (S1).
5. Record hardware/OS/build profile in benchmark artifact.
6. Publish benchmark outputs in machine-readable JSON.

## README/help examples

### README command examples
```bash
# deterministic export from audio file to mp4
cargo run --release --bin tui_visualizer -- export \
  --audio assets/test/latency_pulse_120bpm.wav \
  --out out/demo.mp4 \
  --width 1280 --height 720 --fps 30 \
  --engine metal --quality balanced \
  --seed 42 --emit-manifest out/demo.manifest.json

# export with timeline log for replay/debug
cargo run --release --bin tui_visualizer -- export \
  --audio assets/test/latency_pulse_120bpm.wav \
  --out out/demo.mkv \
  --fps 60 --seed 42 \
  --emit-timeline out/demo.timeline.jsonl

# dry-run ffmpeg plan
cargo run --release --bin tui_visualizer -- export \
  --audio assets/test/latency_pulse_120bpm.wav \
  --out out/demo.mp4 \
  --dry-run
```

### Help-text skeleton
```text
tui_visualizer export --audio <PATH> --out <PATH> [OPTIONS]

Options:
  --width <INT>            Output width (default: 1280)
  --height <INT>           Output height (default: 720)
  --fps <INT>              Output frame rate (default: 60)
  --engine <cpu|metal>     Render engine
  --quality <...>          Quality preset
  --seed <INT>             Deterministic seed
  --codec <...>            Video codec profile
  --audio-codec <...>      Audio codec profile
  --emit-manifest <PATH>   Write export metadata JSON
  --emit-timeline <PATH>   Write per-frame timeline JSONL
  --dry-run                Print computed plan and exit
  --overwrite              Overwrite output if exists
```

## User QA checklist
1. Export succeeds with WAV fixture and produces playable file.
2. Export succeeds with at least one non-WAV file through Symphonia path.
3. Output audio/video streams are present and mapped correctly (ffprobe check).
4. Output dimensions/fps match requested values.
5. Running same command twice with same seed yields same frame hash.
6. Running with different seed changes frame hash.
7. Manifest and timeline files are generated and parse as valid JSON/JSONL.
8. Error message is actionable when ffmpeg is missing.
9. Error message is actionable for unsupported audio input.
10. Documentation commands run as written.

## Self-critique
1. I did not include per-codec tuning tables (x264/x265/vp9/av1 knobs) in detail to keep V1 scope focused.
2. Throughput targets are initial estimates; they need calibration on your actual baseline hardware.
3. Symphonia format support is feature-flag dependent; final UX should disclose enabled codec/container matrix at runtime.

## Open risks and mitigations
1. Risk: hidden nondeterminism from floating-point or thread scheduling.
Mitigation: deterministic mode disables adaptive quality, pins seeds, and records manifest hashes/settings.
2. Risk: ffmpeg behavior changes across installed versions.
Mitigation: add ffmpeg version capture in manifest and CI matrix check.
3. Risk: high-resolution exports saturate memory bandwidth.
Mitigation: preallocated frame buffers, avoid extra copies, benchmark-guided defaults.

## Sources
- S1: https://ffmpeg.org/ffmpeg.html
- S2: https://ffmpeg.org/ffmpeg-formats.html
- S3: https://ffmpeg.org/ffmpeg-protocols.html
- S4: https://ffmpeg.org/ffprobe.html
- S5: https://docs.rs/wgpu/latest/wgpu/struct.TexelCopyBufferLayout.html
- S6: https://jsonlines.org/
- S7: https://docs.asciinema.org/manual/asciicast/v2/
- S8: https://docs.rs/clap/latest/clap/trait.Parser.html
- S9: https://docs.rs/clap/latest/clap/trait.Subcommand.html
- S10: https://docs.rs/clap/latest/clap/trait.ValueEnum.html
- S11: https://bheisler.github.io/criterion.rs/book/analysis.html
- S12: https://docs.rs/symphonia/latest/symphonia/
- S13: https://docs.rs/hound/latest/hound/
- S14: /Users/aaaaa/Projects/tui-visualizer/docs/ARCHITECTURE.md
- S15: /Users/aaaaa/Projects/tui-visualizer/src/render/mod.rs
- S16: /Users/aaaaa/Projects/tui-visualizer/src/app.rs
- S17: /Users/aaaaa/Projects/tui-visualizer/docs/testing.md
- S18: /Users/aaaaa/Projects/tui-visualizer/src/config.rs
