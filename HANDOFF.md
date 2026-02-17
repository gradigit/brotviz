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

- Reworked prior WIP commits into scoped commits from `origin/main`.
- Preserved backup pointer before rewrite on:
  - `backup/pre-split-main-20260217`
- Final commit sequence on `main`:
  1. `c2e3a20` — `feat(visual): expand metal transitions and fractal preset dynamics`
  2. `30cacf1` — `fix(input): retry transient terminal event reader failures`
  3. `cc5573c` — `docs: sync usage and hotkey guidance`
  4. `e4b600f` — `chore(git): ignore local artifact outputs`
  5. `120b5fd` — `docs(handoff): finalize wrap context after scoped split`
- Pushed `main` to `origin/main`.
- Wrap flow completed again with updated handoff context.

### Current State

Branch:
- `main` (in sync with `origin/main`)

Tracked changes at handoff time:
- none (clean tracked tree)

Untracked local artifacts:
- `.DS_Store`
- `out/`
- `promo/`

Most recent commit before this handoff update:
- `120b5fd` — `docs(handoff): finalize wrap context after scoped split`

### What's Next

1. Run an interactive Ghostty smoke check focused on hotkeys:
   - global hotkeys
   - stage-mode toggles
   - help/playlist/selector popups
2. If any hotkeys still fail, instrument `Event` logging in `src/app.rs` for problematic keys/modifiers and patch mapping/gating.
3. If stable, proceed with remaining UX/promo polish and release packaging flow.

### Failed Approaches

- Prior behavior (before this session fix): a single terminal input-reader error permanently disabled hotkeys for the session.
- A single parallel git-write batch caused index-lock contention; git write operations should be serialized.

### Open Questions / Blockers

- User still reports “a lot of hotkeys are not working.” Patch is in place, but interactive validation in real Ghostty is still required.
- `claude-md-improver` skill path referenced by wrap is missing locally; manual CLAUDE.md quality pass was used as fallback.

### Key Context

- This repo includes multiple binaries; use `--bin tui_visualizer` when there is ambiguity.
- `--source system` depends on ScreenCaptureKit and terminal Screen Recording permission.
- Stage mode affects overlay visibility and can be mistaken for input or HUD issues during manual testing.
- Local artifact directories (`out/`, `promo/`) remain intentionally untracked here.

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
