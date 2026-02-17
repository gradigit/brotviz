# Context Handoff — 2026-02-17

Session summary for context continuity after clearing.

## First Steps (Read in Order)

1. Read CLAUDE.md — project context, conventions, current phase
2. Read TODO.md — master feature checklist and completion status
3. Read README.md — user-facing commands, hotkeys, runtime options
4. Read docs/USAGE.md — operational details, selectors, troubleshooting
5. Read src/app.rs — input loop, hotkey dispatch, HUD/help/menus
6. Read src/visual/metal.rs and src/visual/presets.rs — renderer/preset behavior and fractal logic

After reading these files, you'll have full context to continue.

## Session Summary

### What Was Done

- Executed wrap workflow intent:
  - synced owned docs/state (`CLAUDE.md` created, `.doc-manifest.yaml` refreshed)
  - generated this complete handoff snapshot for a fresh agent
- Investigated reported hotkey instability and patched `src/app.rs` input handling:
  - transient `crossterm` poll/read errors no longer disable input immediately
  - input disables only after repeated consecutive failures
  - non-TTY stdin now attempts terminal-input fallback instead of immediate disable
- Built successfully after patch (`cargo build --release`) and validated CLI startup (`--help`)

### Current State

Repository state before handoff commit:
- Modified tracked files:
  - .gitignore
  - README.md
  - docs/USAGE.md
  - src/app.rs
  - src/visual/metal.rs
  - src/visual/presets.rs
  - .doc-manifest.yaml
- Created files:
  - CLAUDE.md
  - HANDOFF.md
- Untracked local artifacts:
  - .DS_Store
  - out/
  - promo/

Last existing commit before wrap:
- `787bf21` — `chore: ignore local release artifacts`

### What's Next

1. Commit the current tracked work (exclude `.DS_Store`, `out/`, and generated media unless intentionally shipping promo assets).
2. Run an interactive Ghostty smoke check focused on hotkeys:
   - global hotkeys
   - stage-mode toggles
   - help/playlist/selector popups
3. If any hotkeys still fail, instrument `Event` logging in `src/app.rs` for problematic keys/modifiers and patch mapping/gating.
4. If stable, proceed with remaining UX/promo polish and release packaging flow.

### Failed Approaches

- Prior behavior (before this session fix): a single terminal input-reader error permanently disabled hotkeys for the session.

### Open Questions / Blockers

- User still reports “a lot of hotkeys are not working.” Patch is in place, but interactive validation in real Ghostty is still required.
- `claude-md-improver` skill path referenced by wrap is missing locally; manual CLAUDE.md quality pass was used as fallback.

### Key Context

- This repo includes multiple binaries; use `--bin tui_visualizer` when there is ambiguity.
- `--source system` depends on ScreenCaptureKit and terminal Screen Recording permission.
- Stage mode affects overlay visibility and can be mistaken for input or HUD issues during manual testing.

## Reference Files

| File | Purpose |
|------|---------|
| CLAUDE.md | Session context, commands, gotchas, current phase |
| README.md | Public overview, install/run paths, hotkeys |
| docs/USAGE.md | Detailed runtime flags, selectors, troubleshooting |
| docs/ARCHITECTURE.md | Pipeline and module boundaries |
| TODO.md | Master feature completion tracker |
| .doc-manifest.yaml | Documentation inventory and last sync time |
| src/app.rs | Main loop + input/key handling + overlays |
| src/audio.rs | Audio capture and feature extraction |
| src/visual/metal.rs | GPU rendering path and shader integration |
| src/visual/presets.rs | Preset algorithms/fractal behaviors |
