# Brotviz Master TODO
Date initialized: 2026-02-15
Owner: Master agent

## Master checklist

- [x] Research reports completed for all scoped features.
- [x] Master plan created.
- [x] Master tracker created.
- [x] F00 Infinite zoom continuity fix.
- [x] F01 Beat-synced camera system.
- [x] F02 Transition engine v2.
- [x] F03 Preset graph mode.
- [x] F04 Fractal deep-zoom mode.
- [x] F05 Scene energy auto-DJ.
- [x] F06 Reactive post-processing.
- [x] F07 Multi-band control matrix.
- [x] F10 Capability auto-probing.
- [x] F11 Performance governor.
- [x] F12 Latency-calibrated mode.
- [x] F14 Audio-reactive typography layer.
- [x] F15 Recorder/export mode.
- [x] F17 Theme packs.
- [x] F18 Procedural geometry bank.
- [x] F19 Camera path presets.
- [x] F20 No-HUD performance stage mode.
- [x] Full test pass and benchmark pass with tracker evidence.
- [x] Docs/help/hotkeys fully synced.

## Feature checklists

### F00 Infinite zoom continuity

- [x] F00-T1 audit current zoom reset paths
- [x] F00-T2 monotonic camera state
- [x] F00-T3 continuous integrator (no modulo wrap)
- [x] F00-T4 anti-jump smoothing
- [x] F00-T5 continuity tests + fixtures

### F01 Beat-synced camera

- [x] F01-T1 modulation bus
- [x] F01-T2 beat/onset mapping
- [x] F01-T3 smoothing/hysteresis
- [x] F01-T4 camera mode presets

### F02 Transition engine v2

- [x] F02-T1 transition taxonomy
- [x] F02-T2 operator interface
- [x] F02-T3 beat-aware scheduler
- [x] F02-T4 hard-cut guardrail
- [x] F02-T5 step/lock controls

### F03 Preset graph mode

- [x] F03-T1 graph schema
- [x] F03-T2 compile to runtime IR
- [x] F03-T3 modulation route binding
- [x] F03-T4 runtime safety checks
- [x] F03-T5 playlist integration

### F04 Fractal deep-zoom mode

- [x] F04-T1 high-precision zoom state path
- [x] F04-T2 coordinate rebasing
- [x] F04-T3 deep-zoom quality tiers
- [x] F04-T4 perturbation phase

### F05 Scene energy auto-DJ

- [x] F05-T1 section classifier
- [x] F05-T2 hysteresis state machine
- [x] F05-T3 transition/preset policy binding

### F06 Reactive post-processing

- [x] F06-T1 post-FX graph pipeline
- [x] F06-T2 core FX modules
- [x] F06-T3 audio bindings
- [x] F06-T4 quality + safety clamps

### F07 Multi-band control matrix

- [x] F07-T1 expanded feature vector
- [x] F07-T2 routing table + curves
- [x] F07-T3 smoothing defaults
- [x] F07-T4 edit API + serialization

### F10 Capability auto-probing

- [x] F10-T1 capability probe engine
- [x] F10-T2 fallback ladder integration
- [x] F10-T3 probe status in HUD/help

### F11 Performance governor

- [x] F11-T1 frame budget telemetry
- [x] F11-T2 adaptive quality/resolution/effects policy
- [x] F11-T3 anti-oscillation controls

### F12 Latency-calibrated mode

- [x] F12-T1 latency estimator
- [x] F12-T2 calibration routine
- [x] F12-T3 phase correction

### F14 Typography layer

- [x] F14-T1 overlay engine
- [x] F14-T2 typography modes
- [x] F14-T3 readability guardrails

### F15 Recorder/export

- [x] F15-T1 offscreen deterministic timeline
- [x] F15-T2 ffmpeg frame pipe
- [x] F15-T3 audio mux flow
- [x] F15-T4 export CLI command
- [x] F15-T5 golden export tests

### F17 Theme packs

- [x] F17-T1 pack manifest schema
- [x] F17-T2 grouping + metadata
- [x] F17-T3 pack loader + validation

### F18 Procedural geometry bank

- [x] F18-T1 new algorithm modules
- [x] F18-T2 normalized parameter API
- [x] F18-T3 preset graph integration

### F19 Camera path presets

- [x] F19-T1 path library
- [x] F19-T2 path blending
- [x] F19-T3 beat-aware path switching

### F20 Stage mode

- [x] F20-T1 true HUD-off render path
- [x] F20-T2 stage-mode governor profile
- [x] F20-T3 persistent preference + safe toggles
