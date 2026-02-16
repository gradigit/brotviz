# Research: Terminal Typography + Rendering Paths for Brotviz
Date: 2026-02-16
Depth: Full

## Executive summary
High-impact typography in a TUI visualizer is feasible, but there are hard protocol boundaries. The most portable path is still symbol/raster rendering in-cell (ASCII, half-block, braille, sextant). For premium visuals on Ghostty/Kitty-class terminals, Kitty graphics protocol plus GPU-produced RGBA frames is the strongest path today. Advanced typography should be done in-app via shaping/rasterization (Cosmic Text + Rustybuzz + Swash or Fontdue/MSDF), then composited into the RGBA frame.

## What is possible now (verified)
1. Ghostty supports Kitty image protocol controls and has image storage limits/config for Kitty images.
2. Kitty graphics protocol supports direct, file, and shared-memory transfers and placement controls suitable for high-FPS frame updates.
3. Kitty text sizing protocol exists for large/inline terminal text effects, but this is protocol-specific and requires capability checks.
4. Synchronized output mode (`DECSET 2026`) exists for reduced tearing during frame updates.
5. Alternative rendering stacks exist:
   - Symbol blitters (ASCII/Braille/Sextant/Block) via Unicode
   - Pixel protocols (Kitty, iTerm2 inline images, sixel where available)
6. Rust typography stacks are production-ready:
   - `cosmic-text` for shaping/layout
   - `rustybuzz` for shaping
   - `swash` and/or `fontdue` for rasterization
   - `msdfgen` for scalable distance-field text

## Recommended typography architecture for Brotviz
1. Keep current fast path (audio-driven mode transforms + HUD text).
2. Add optional "Typo Layer" in engine output (RGBA compositing stage before renderer):
   - Render shaped text runs to alpha masks (or MSDF atlas)
   - GPU composite in Metal compute pass (or CPU fallback path)
   - Drive typography transform from audio features (onset, centroid, beat, bands)
3. Add capability gates:
   - `--renderer kitty` + protocol available -> full raster typography effects
   - symbol renderers -> fallback to coarse glyph motifs/ASCII typography
4. Add feature switches for cost control:
   - `--typography-level off|lite|full`
   - `--typography-quality low|med|high`
   - runtime toggles for blur/glow/chromatic split

## Concrete typography effects worth implementing next
1. Kinetic lyric band (single-line, beat-snapped baseline drift)
2. Audio-reactive kerning/weight modulation (variable-font axis where available)
3. Glyph echo trails (decay buffers tied to high-mid/treble)
4. MSDF neon outline mode (psychedelic edge glow)
5. Fractal text warp mode (map glyph UVs through fractal field)
6. Type datamosh transition (interpolate between preset labels/phrases in transition windows)

## Performance notes
1. Prefer prebuilt glyph atlases and reuse across frames.
2. Keep text shaping off hot path (shape only on string/style change).
3. Composite typography in Metal with a single extra pass; avoid CPU readback until final renderer stage.
4. Keep synchronized updates configurable because terminal implementations vary.

## Sources
- Ghostty config reference (image protocol + shader/font settings):
  https://ghostty.org/docs/config/reference
- Kitty graphics protocol:
  https://sw.kovidgoyal.net/kitty/graphics-protocol/
- Kitty text sizing protocol:
  https://sw.kovidgoyal.net/kitty/text-sizing-protocol/
- Kitty sixel notes:
  https://sw.kovidgoyal.net/kitty/sixel/
- WezTerm escape sequences (mode 2026 synchronized output):
  https://wezterm.org/escape-sequences.html#mode-2026-synchronized-output
- iTerm2 inline images protocol:
  https://iterm2.com/documentation-images.html
- Notcurses visual API/blitters:
  https://notcurses.com/notcurses_visual.3.html
- Chafa symbol/pixel conversion docs:
  https://hpjansson.org/chafa/man/
- Cosmic Text crate docs:
  https://docs.rs/cosmic-text/latest/cosmic_text/
- Rustybuzz crate docs:
  https://docs.rs/rustybuzz/latest/rustybuzz/
- Swash crate docs:
  https://docs.rs/swash/latest/swash/
- Fontdue crate docs:
  https://docs.rs/fontdue/latest/fontdue/
- MSDF generation crate docs:
  https://docs.rs/msdfgen/latest/msdfgen/
- Apple Metal best practices guide:
  https://developer.apple.com/library/archive/documentation/3DDrawing/Conceptual/MTLBestPracticesGuide/index.html
- Unicode Symbols for Legacy Computing (block/sextant-related glyph set):
  https://www.unicode.org/charts/nameslist/n_1FB00.html
