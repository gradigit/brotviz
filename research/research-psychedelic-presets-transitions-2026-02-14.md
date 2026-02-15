# Research: Psychedelic Presets + Transitions (Milkdrop-ish) for a Rust TUI Visualizer
Date: 2026-02-14
Depth: Full

## Executive Summary
Milkdrop-style “hypnotic” visuals come from a small set of repeatable ingredients: (1) a preset model that exposes many audio-reactive scalars and shared scratch variables, (2) strong temporal feedback (previous frame used as an input), (3) continuous geometric warps (zoom/rot/warp) modulated by smoothed band energies, and (4) deliberate preset switching with *transitions* that feel like effects (not just crossfades). A TUI visualizer can approximate this well by using shader-friendly procedural fields (fractals, domain warps, attractors, kaleidoscopes, reaction-diffusion-like feedback) and by treating transitions as first-class “mini-presets” (zoom-through, feedback melt, datamosh/glitch smear).

## Sub-Questions Investigated
1. What specific mechanisms make Milkdrop/projectM presets feel “alive” and audio-reactive?
2. What GPU-friendly algorithm families produce psychedelic / fractal / geometric looks at terminal resolutions?
3. How do modern Milkdrop implementations handle preset switching and transitions?
4. How can we approximate “datamosh”/glitch transitions in a realtime shader/feedback pipeline?

## Key Findings (Actionable)

### 1) Milkdrop / projectM preset structure: why it works
Milkdrop’s preset authoring model is equation-driven: presets define parameters and *per-frame* logic, and can also define *per-vertex* and *per-pixel* code paths (depending on version/implementation). This is why presets can have their own motion, color modulation, beat branching, and emergent behavior rather than just “one shader.” Source:
- Milkdrop preset authoring guide: https://www.geisswerks.com/hosted/milkdrop2/milkdrop_preset_authoring.html

projectM (Milkdrop reimplementation) exposes canonical “feel” parameters like `zoom`, `rot`, and `warp`, and audio-derived variables like bass/mid/treble (and attenuated variants), plus `q` scratch variables that can carry state across equations. Source:
- projectM preset authoring wiki: https://github-wiki-see.page/m/projectM-visualizer/projectm/wiki/Preset-Authoring-Guide

Practical takeaway for our engine:
- Every preset should have: `zoom`, `rot`, `warp_amp`, `warp_freq`, `feedback`, `hue_shift`, `sparkle`, and a small set of scratch state variables (even if we don’t implement a full DSL yet).

### 2) Butterchurn (Webamp) confirms transition philosophy
Butterchurn, a popular WebGL Milkdrop implementation, explicitly treats preset switching as an effect: during transitions, “the image from the previous preset is used as input into the next preset,” which is a huge part of the “continuous trip” feel. Source:
- Butterchurn docs: https://webamp.org/docs/butterchurn/

This maps directly to our pipeline:
- Maintain a feedback texture/framebuffer.
- During transition, bias feedback sampling and/or apply macroblock displacement and chromatic splits.

### 3) Psychedelic algorithm families that work well on GPU (and at terminal-ish resolutions)
These are high “wow-per-flop” families, proven across shader communities and fractal art tooling:

Color palette design (fast, vivid, controllable)
- Cosine palette parameterization is a common, cheap way to get vibrant cycling palettes:
  - https://iquilezles.org/articles/palettes/

Domain warping (organic psychedelia)
- Domain warping takes a simple noise/field and warps its input coordinates recursively, producing complex “liquid” motion cheaply:
  - https://iquilezles.org/articles/warp/

Fractals (classic Milkdrop-adjacent)
- Orbit trap coloring is a lightweight method to get rich fractal detail:
  - https://en.wikipedia.org/wiki/Orbit_trap
- Mandelbrot/burning-ship style tutorials remain useful for iteration + coloring structure:
  - https://www.mandelbrowser.com/tutorial/

Reaction-diffusion (brain/coral textures)
- Gray–Scott reaction-diffusion is a standard model for organic patterns:
  - https://en.wikipedia.org/wiki/Gray%E2%80%93Scott_model
- Practical shader-oriented implementations exist (WebGL/compute style):
  - https://github.com/guillaume-haerinck/reaction-diffusion-shader
  - https://piellardj.github.io/reaction-diffusion/

Attractors (hypnotic geometry)
- Strange attractor equations produce intense, “forever novel” geometry with tiny compute:
  - Clifford attractor: https://en.wikipedia.org/wiki/Clifford_attractor

Kaleidoscopic transforms
- Kaleidoscope effects are simple coordinate folding/mirroring + a base field:
  - https://www.geeks3d.com/20140211/kaleidoscope-effect-in-glsl/

### 4) “Datamosh” as a realtime transition effect (what we can approximate)
True datamoshing is tied to video compression (I/P frames, motion vectors). We won’t reproduce codec internals in realtime, but we *can* approximate the aesthetic:
- Use temporal feedback (previous frame) as the primary image source during the transition.
- Apply macroblock (“codec block”) quantization in screen space.
- Displace whole blocks by a pseudo motion vector field driven by onsets/treble and time.
- Add chromatic offsets and luma-threshold tearing.

References for conceptual grounding:
- Compression artifacts overview: https://en.wikipedia.org/wiki/Compression_artifact
- Video compression picture types (I/P/B): https://en.wikipedia.org/wiki/Video_compression_picture_types

Shader-ish / practical datamosh approximations:
- Datamosh-like shader notes: https://thenumb.at/cpp-course/shaders/datamosh.html
- One practical datamoshing tooling approach (codec-focused): https://github.com/Akascape/datamoshing

## Verified Claims (2+ independent sources)
- Milkdrop presets are equation-driven and expose a structured set of audio variables and motion parameters. Sources:
  - https://www.geisswerks.com/hosted/milkdrop2/milkdrop_preset_authoring.html
  - https://github-wiki-see.page/m/projectM-visualizer/projectm/wiki/Preset-Authoring-Guide
- “Previous frame as input during transitions” is a core part of modern Milkdrop implementations (Butterchurn). Sources:
  - https://webamp.org/docs/butterchurn/
  - https://github.com/jberg/butterchurn

## Implementation Recommendations (for this repo)
1. Add more presets by expanding the core algorithm set:
   - Burning Ship fractal, orbit-trap variants, domain warps, Clifford/de Jong attractor fields, polar/kaleidoscope moire, truchet tiles.
2. Make auto-mode “feel smart” by picking:
   - Transition duration (already present): short on hard transients, long on smooth.
   - Transition kind: datamosh/glitch on strong onsets/high-treble, zoom-melt on smooth, standard fade otherwise.
3. Treat transitions as first-class:
   - Store `transition_kind` per switch.
   - Implement at least: Fade, Zoom-through, Datamosh smear.
4. Map audio to motion like Milkdrop:
   - bass -> zoom/push/pulse
   - beat -> strobe/cut triggers
   - treble/onset -> sparkle/glitch displacement
   - centroid -> hue rotation / palette bias
   - flatness -> noise vs geometry bias (more noisy textures when flatness high)

