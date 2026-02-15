# Research: Terminal Audio-Reactive Visualizer (Rust, Ghostty)
Date: 2026-02-14
Depth: Full

## Executive Summary
You can build a "Milkdrop-inspired" audio-reactive terminal visualizer in full Rust with strong performance by separating:
1) audio capture (mic + system-loopback paths),
2) audio feature extraction (FFT bands + onset/beat),
3) a preset/effects engine (20 built-in presets for v1),
4) a renderer backend (portable ANSI truecolor baseline, with optional Kitty-graphics images for terminals that support it).

The best practical baseline for Ghostty is ANSI truecolor using the Unicode upper-half-block character with per-cell fg/bg colors (effectively 2 vertical pixels per terminal cell). Kitty graphics protocol support appears to exist in Ghostty via third-party compatibility matrices, but continuous video-like streaming performance is terminal-dependent and should be treated as an optional backend rather than the only renderer.

## Sub-Questions Investigated
1. What is the best way to draw high-resolution, colorful visuals in a terminal (Ghostty)?
2. Can we realistically do GPU rendering for a terminal visualizer?
3. How do we capture microphone vs system audio cross-platform in Rust?
4. What audio analysis is needed for bass/beat/hi-hats/vocals reactions?
5. How does Milkdrop/projectM structure presets and audio-reactive variables?

## Detailed Findings

### 1) Terminal Rendering Options (Ghostty)
Option A: ANSI truecolor + Unicode half-blocks (recommended baseline)
- Technique: treat the screen as a pixel grid of (term_width x term_height*2). Each character cell uses the "upper half block" char and encodes two vertically stacked pixels via fg (top) and bg (bottom).
- Pros: widely compatible, very colorful (24-bit), simple, no image protocol negotiation, great for realtime animation.
- Cons: limited to 2 vertical subpixels per cell (unless you switch to braille, sextants, etc., which trades color fidelity).

Option B: Kitty graphics protocol (optional high-fidelity backend)
- Many modern terminals implement Kitty's graphics protocol, which can display images via escape sequences (supports chunking/streaming of base64 data, and other transport options).
- Pros: can represent full-color bitmaps without block-character quantization; can look closer to "real" Milkdrop at high res.
- Cons: not all terminals support it equally; performance for high-FPS updates varies; implementation complexity is higher.
- Ghostty compatibility: third-party Rust terminal-image crates report Kitty protocol working on Ghostty; treat as "supported but performance-sensitive."

Option C: Sixel (not recommended for Ghostty-first)
- Sixel support is inconsistent across terminals; Ghostty support is unclear from first-party docs and should be assumed absent unless proven.

### 2) GPU Rendering Feasibility
"GPU rendered" in a terminal usually means "GPU renders into an offscreen texture, then CPU reads back and encodes it into terminal output (ANSI or an image protocol)."
- This is feasible with Rust + wgpu headless rendering and a readback buffer.
- Practical note: for terminal-sized resolutions, stdout/terminal throughput tends to be the bottleneck more than compute. GPU is most valuable to enable shader-style effects (feedback warps, fractal shading, palette mapping) without burning CPU.

### 3) Audio Capture (Mic + System Output)
Baseline approach (portable): capture from an input device using `cpal`.
- Mic is straightforward: select default input or a named device.
- "System audio" differs by OS:
  - Windows: WASAPI loopback capture can capture the default output device directly (no virtual device).
  - macOS: typically requires a virtual loopback device (BlackHole/Loopback/Soundflower) or routing via an aggregate/multi-output device; your app can still use `cpal` by selecting the loopback as input.
  - Linux: typically capture from a monitor source (PulseAudio/PipeWire); again can be presented as an input device.

For v1, the most robust cross-platform UX is:
- `--source mic` uses the default input device.
- `--source system` selects an OS-appropriate method:
  - Windows: WASAPI loopback.
  - Others: selects a "monitor/loopback" input if present, otherwise errors with actionable setup guidance and `--list-devices`.

### 4) Audio Analysis for "Bass/Beat/Hi-hats/Vocals"
Milkdrop-style control is usually driven by smoothed band energies and beat triggers.
Practical v1 feature set:
- `rms`: overall amplitude (per-frame loudness)
- `spectrum`: FFT magnitude bins (windowed)
- band energies: sub-bass, bass, low-mid, mid, high-mid, treble, air
- onset/beat:
  - compute a spectral-flux onset envelope (optionally per-band),
  - adaptive peak-pick to emit `beat` events and `beat_strength`,
  - optional tempo estimate (autocorrelation over beat envelope) for time-based modulation
- "vocals" proxy (v1): mid-band energy + spectral flatness/centroid heuristics (not true source separation, but good enough to create a "vocal reactive" feel)

### 5) Milkdrop / projectM Takeaways
Milkdrop's "preset feel" comes from:
- a preset scripting model with per-frame equations,
- a set of state variables (including shared `q` variables),
- beat-reactive branching,
- and a feedback/warp-based rendering pipeline (often shader-driven).

projectM is an open-source reimplementation of Milkdrop with a large preset ecosystem; it is an excellent reference for:
- which audio-derived variables are useful (bass/mid/treb, beat flags),
- preset switching behaviors,
- and how to structure a realtime visual pipeline.

## Hypothesis Tracking
| Hypothesis | Confidence | Supporting Evidence | Contradicting Evidence |
|---|---|---|---|
| H1: ANSI truecolor half-block rendering is the best Ghostty-first renderer for realtime visuals | High | Common technique in Rust TUI examples and image renderers; no protocol deps | Lower fidelity than bitmap protocols |
| H2: Kitty graphics protocol is viable as an optional backend for Ghostty | Medium | Third-party Rust crates report Kitty images work in Ghostty | Streaming performance for "video" updates is terminal-dependent |
| H3: GPU rendering materially improves performance | Low-Med | GPU enables shader effects cheaply | Terminal output bandwidth dominates at typical resolutions |

## Verification Status
Verified (2+ sources):
- Half-block truecolor (fg/bg) is a standard way to render images in terminals.
- Ghostty appears to interoperate with Kitty-graphics images per third-party compatibility matrices.

Unverified / risky:
- Ghostty performance for high-FPS Kitty-graphics streaming.
- Ghostty Sixel support.

## Recommendation (For v1)
1. Implement a portable renderer first: ANSI truecolor + half-blocks (and optionally a braille "hi-res monochrome-ish" mode).
2. Keep Kitty-graphics as an optional backend behind a flag, not a requirement.
3. Use `cpal` for mic/input-device capture, and add OS-specific "system audio" support:
   - Windows: WASAPI loopback,
   - macOS/Linux: pick a monitor/loopback input device if available; otherwise provide setup instructions.
4. Use a Milkdrop-inspired preset engine (but not Milkdrop's full DSL in v1):
   - 20 built-in presets defined as effect graphs with audio-modulated parameters,
   - manual switching (left/right), shuffle toggle, and auto-switch modes (time/beat/energy),
   - transitions (crossfade/zoom-warp) to make switching feel intentional.

## Sources (High-Quality Starting Set)
| Source | URL | Quality | Accessed | Notes |
|---|---|---:|---:|---|
| Kitty graphics protocol | https://sw.kovidgoyal.net/kitty/graphics-protocol/ | Official | 2026-02-14 | Terminal bitmap transport |
| ratatui-image | https://docs.rs/ratatui-image/latest/ratatui_image/ | Docs.rs | 2026-02-14 | Kitty protocol compatibility notes (Ghostty listed) |
| serie crate (terminal image protocols) | https://docs.rs/serie/latest/serie/ | Docs.rs | 2026-02-14 | Notes Ghostty + Kitty; warns about Sixel |
| ratatui half-block example | https://github.com/ratatui/ratatui/blob/main/examples/colors_rgb.rs | Upstream example | 2026-02-14 | Demonstrates half-block technique |
| viuer (terminal images) | https://docs.rs/viuer/latest/viuer/ | Docs.rs | 2026-02-14 | Uses half blocks; supports multiple protocols |
| cpal (Rust audio I/O) | https://docs.rs/cpal/latest/cpal/ | Docs.rs | 2026-02-14 | Cross-platform audio input |
| wasapi crate | https://docs.rs/wasapi/latest/wasapi/ | Docs.rs | 2026-02-14 | Loopback capture for Windows |
| projectM | https://github.com/projectM-visualizer/projectm | Official OSS | 2026-02-14 | Milkdrop reimplementation w/ beat detection |
| projectM preset pack | https://github.com/projectM-visualizer/presets-projectm-classic | Official OSS | 2026-02-14 | Preset library reference |
| Milkdrop preset authoring guide | https://www.geisswerks.com/hosted/milkdrop2/milkdrop_preset_authoring.html | Original docs | 2026-02-14 | Preset model and variables |

