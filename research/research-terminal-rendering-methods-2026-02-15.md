# Research: Terminal Rendering Methods for Brotviz
Date: 2026-02-15
Depth: Full

## Executive Summary

For Brotviz on macOS + Ghostty, the highest-value rendering path remains Kitty graphics protocol plus text-cell fallbacks. The best near-term improvement is to keep Kitty rendering but switch transmission away from shared memory and toward direct chunked payloads (or file/tempfile medium), then add one higher-density text fallback mode (sextant/quad symbol path).

## Sub-Questions Investigated

1. Which graphics/image protocols are viable for terminal visualizers?
2. What does Ghostty actually support today?
3. Which methods should Brotviz prioritize next?

## Key Findings

### 1) Kitty graphics protocol is the strongest protocol target

- Kitty protocol is explicitly designed for raster graphics with placement, layering, alpha behavior, and multiple transmission media (`d`, `f`, `t`, `s`).
- It supports direct chunked transfer and local-host optimizations.
- Ghostty is listed as an implementation target in Kitty protocol docs.

Implication for Brotviz:
- Keep Kitty as the primary high-fidelity backend.
- Prefer direct transfer (`t=d`) fallback path if shared-memory mode is unstable in practice.

### 2) Ghostty’s own source shows Kitty support, but also important limits

- Ghostty source has a dedicated Kitty graphics implementation:
  - `src/terminal/kitty/graphics.zig`
- That file documents some TODOs including:
  - shared memory transmit
  - virtual placement with unicode
  - animation

Implication for Brotviz:
- Treat shared-memory Kitty transport as optional, not required.
- Implement runtime fallback order for Kitty transport methods.

### 3) Ghostty does not appear to implement Sixel today (inferred)

- Code search in Ghostty repo returned no Sixel implementation hits.
- Ghostty VT external protocols docs list OSC 8 and OSC 21 in the external protocol table.

Inference:
- Sixel is likely not a practical Ghostty backend currently.
- Sixel is still valuable as a cross-terminal backend for non-Ghostty terminals.

### 4) Ghostty parses OSC 1337, but file/image keys are marked unimplemented

- Ghostty parser includes OSC 1337 parser code.
- In that parser, many keys (including `File`, `FilePart`, `MultipartFile`) are marked unimplemented.

Implication for Brotviz:
- iTerm2 inline image protocol is not a Ghostty strategy today.
- It can still be added as a separate backend for iTerm2/WezTerm users.

### 5) Text-cell rendering can still be pushed further

- Notcurses and Chafa document adaptive paths:
  - pixel protocols when available
  - Unicode symbol blitters when not
- Chafa symbol classes include `braille`, `half`, `quad`, `sextant`, etc.

Implication for Brotviz:
- Add a high-density symbol renderer variant (e.g., sextant/quad mode) as a robust fallback for environments where graphics protocol is unavailable or unstable.

## Candidate Rendering Methods (Ranked for Brotviz)

1. Kitty protocol with adaptive transport (`t=d` first, optional `t=f/t=t`, guarded `t=s`)
2. Kitty protocol advanced placement mode (later): relative placement / placeholder-driven compositing
3. High-density Unicode symbol renderer (sextant + quad + braille hybrid strategies)
4. Sixel backend (cross-terminal portability, not Ghostty-first)
5. iTerm2 OSC 1337 backend (for iTerm2/WezTerm compatibility)

## Recommended Roadmap

### Phase A (Ghostty-first, highest ROI)

1. Keep Kitty backend as primary GPU-to-terminal path.
2. Add transport fallback chain:
   - direct chunked payload
   - file/tempfile medium
   - shared memory only if confirmed available
3. Add `sextant` renderer mode for higher text fallback resolution.

### Phase B (Portability)

1. Add iTerm2 image backend for iTerm2/WezTerm.
2. Add Sixel backend for terminals that expose it.
3. Add backend auto-detection + capability probing.

## Verification Notes

Verified with primary sources:
- Ghostty docs and Ghostty source files
- Kitty protocol specification
- iTerm2 image protocol documentation
- xterm control sequence documentation (Sixel references)
- Notcurses and Chafa docs for symbol/pixel rendering strategies

## Sources

- Ghostty About page:
  - http://ghostty.org/docs/about
- Ghostty VT external protocols:
  - http://ghostty.org/docs/vt/external
- Ghostty Kitty graphics implementation source:
  - https://raw.githubusercontent.com/ghostty-org/ghostty/main/src/terminal/kitty/graphics.zig
- Ghostty OSC 1337 parser source:
  - https://raw.githubusercontent.com/ghostty-org/ghostty/main/src/terminal/osc/parsers/iterm2.zig
- Kitty graphics protocol:
  - http://sw.kovidgoyal.net/kitty/graphics-protocol/
- iTerm2 inline images protocol:
  - http://iterm2.com/documentation-images.html
- xterm control sequences (Sixel references):
  - http://invisible-island.net/xterm/ctlseqs/ctlseqs.html
- Notcurses manpage:
  - http://notcurses.com/notcurses.3.html
- Chafa manpage:
  - http://hpjansson.org/chafa/man/
