mod presets;
#[cfg(target_os = "macos")]
mod metal;

use crate::audio::AudioFeatures;
use crate::config::{Quality, SwitchMode};
use std::time::{Duration, Instant};

pub use presets::{make_presets, Preset, RenderCtx};
#[cfg(target_os = "macos")]
pub use metal::MetalEngine;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum TransitionKind {
    Fade = 0,
    Zoom = 1,
    Datamosh = 2,
    Radial = 3,
    Swirl = 4,
    Dissolve = 5,
    Cut = 6,
    Morph = 7,
    Wipe = 8,
    Luma = 9,
    Flash = 10,
    Prism = 11,
    Remix = 12,
    Echo = 13,
}

impl TransitionKind {
    pub(crate) const fn all() -> [Self; 14] {
        [
            Self::Fade,
            Self::Zoom,
            Self::Datamosh,
            Self::Radial,
            Self::Swirl,
            Self::Dissolve,
            Self::Cut,
            Self::Morph,
            Self::Wipe,
            Self::Luma,
            Self::Flash,
            Self::Prism,
            Self::Remix,
            Self::Echo,
        ]
    }

    pub(crate) fn next(self) -> Self {
        let all = Self::all();
        let mut idx = 0usize;
        while idx < all.len() {
            if all[idx] == self {
                return all[(idx + 1) % all.len()];
            }
            idx += 1;
        }
        Self::Fade
    }

    pub(crate) fn prev(self) -> Self {
        let all = Self::all();
        let mut idx = 0usize;
        while idx < all.len() {
            if all[idx] == self {
                return all[(idx + all.len() - 1) % all.len()];
            }
            idx += 1;
        }
        Self::Fade
    }

    pub(crate) fn operator_label(self) -> &'static str {
        match self {
            Self::Fade => "Fade",
            Self::Zoom => "Zoom",
            Self::Datamosh => "Datamosh",
            Self::Radial => "Radial",
            Self::Swirl => "Swirl",
            Self::Dissolve => "Dissolve",
            Self::Cut => "Cut",
            Self::Morph => "Morph",
            Self::Wipe => "Wipe",
            Self::Luma => "Luma Key",
            Self::Flash => "Flash Cut",
            Self::Prism => "Prism",
            Self::Remix => "Remix",
            Self::Echo => "Echo",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        self.operator_label()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionMode {
    Auto,
    Smooth,
    Punchy,
    Morph,
    Remix,
    Cuts,
}

impl TransitionMode {
    fn next(self) -> Self {
        match self {
            Self::Auto => Self::Smooth,
            Self::Smooth => Self::Punchy,
            Self::Punchy => Self::Morph,
            Self::Morph => Self::Remix,
            Self::Remix => Self::Cuts,
            Self::Cuts => Self::Auto,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Smooth => "Smooth",
            Self::Punchy => "Punchy",
            Self::Morph => "Morph",
            Self::Remix => "Remix",
            Self::Cuts => "Cuts",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneSection {
    Calm,
    Groove,
    Drive,
    Impact,
}

impl SceneSection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Calm => "Calm",
            Self::Groove => "Groove",
            Self::Drive => "Drive",
            Self::Impact => "Impact",
        }
    }

    const fn intensity_rank(self) -> i8 {
        match self {
            Self::Calm => 0,
            Self::Groove => 1,
            Self::Drive => 2,
            Self::Impact => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraPathMode {
    Auto,
    Orbit,
    Dolly,
    Helix,
    Spiral,
    Drift,
}

impl CameraPathMode {
    const fn all() -> [Self; 6] {
        [
            Self::Auto,
            Self::Orbit,
            Self::Dolly,
            Self::Helix,
            Self::Spiral,
            Self::Drift,
        ]
    }

    fn next(self) -> Self {
        let all = Self::all();
        let mut idx = 0usize;
        while idx < all.len() {
            if all[idx] == self {
                return all[(idx + 1) % all.len()];
            }
            idx += 1;
        }
        Self::Auto
    }

    fn prev(self) -> Self {
        let all = Self::all();
        let mut idx = 0usize;
        while idx < all.len() {
            if all[idx] == self {
                return all[(idx + all.len() - 1) % all.len()];
            }
            idx += 1;
        }
        Self::Auto
    }

    fn step(self, forward: bool) -> Self {
        if forward {
            self.next()
        } else {
            self.prev()
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Orbit => "Orbit",
            Self::Dolly => "Dolly",
            Self::Helix => "Helix",
            Self::Spiral => "Spiral",
            Self::Drift => "Drift",
        }
    }
}

fn pick_kind(seed: u32, choices: &[TransitionKind], last: TransitionKind) -> TransitionKind {
    if choices.is_empty() {
        return TransitionKind::Fade;
    }
    fn flavor(kind: TransitionKind) -> u8 {
        match kind {
            TransitionKind::Cut | TransitionKind::Flash => 0, // hard cuts
            TransitionKind::Datamosh | TransitionKind::Echo => 1, // glitch
            TransitionKind::Morph | TransitionKind::Remix => 2, // morph/remix
            TransitionKind::Zoom
            | TransitionKind::Radial
            | TransitionKind::Swirl
            | TransitionKind::Wipe
            | TransitionKind::Prism => 3, // motion geometry
            TransitionKind::Fade | TransitionKind::Dissolve | TransitionKind::Luma => 4, // soft blend
        }
    }
    let len = choices.len();
    let base = (seed as usize) % len;
    let last_flavor = flavor(last);
    // First pass: avoid immediate repeats and same-family repeats.
    for off in 0..len {
        let c = choices[(base + off) % len];
        if c != last && (len <= 2 || flavor(c) != last_flavor) {
            return c;
        }
    }
    // Fallback: at least avoid exact repetition.
    for off in 0..len {
        let c = choices[(base + off) % len];
        if c != last || len == 1 {
            return c;
        }
    }
    choices[base]
}

fn is_hard_cut(kind: TransitionKind) -> bool {
    matches!(kind, TransitionKind::Cut | TransitionKind::Flash)
}

fn step_transition_override(cur: Option<TransitionKind>, forward: bool) -> Option<TransitionKind> {
    match (cur, forward) {
        (None, true) => Some(TransitionKind::all()[0]),
        (None, false) => {
            let all = TransitionKind::all();
            Some(all[all.len() - 1])
        }
        (Some(k), true) => {
            let n = k.next();
            if n == TransitionKind::all()[0] {
                None
            } else {
                Some(n)
            }
        }
        (Some(k), false) => {
            if k == TransitionKind::all()[0] {
                None
            } else {
                Some(k.prev())
            }
        }
    }
}

pub(crate) fn transition_base_duration(kind: TransitionKind) -> Duration {
    match kind {
        TransitionKind::Cut | TransitionKind::Flash => Duration::from_millis(120),
        TransitionKind::Datamosh => Duration::from_millis(240),
        TransitionKind::Wipe => Duration::from_millis(560),
        TransitionKind::Dissolve | TransitionKind::Luma => Duration::from_millis(720),
        TransitionKind::Morph | TransitionKind::Remix | TransitionKind::Echo => {
            Duration::from_millis(1120)
        }
        TransitionKind::Prism => Duration::from_millis(760),
        TransitionKind::Zoom | TransitionKind::Radial | TransitionKind::Swirl => {
            Duration::from_millis(900)
        }
        TransitionKind::Fade => Duration::from_millis(820),
    }
}

pub(crate) fn transition_duration_for_kind(
    kind: TransitionKind,
    audio: &AudioFeatures,
) -> Duration {
    let base = transition_base_duration(kind).as_millis() as f32;
    let drive = (audio.onset * 0.45 + audio.beat_strength * 0.35 + audio.rms * 0.20).clamp(0.0, 1.0);
    let scale = match kind {
        TransitionKind::Cut | TransitionKind::Flash => 1.0 - 0.45 * drive,
        TransitionKind::Datamosh | TransitionKind::Wipe => 1.0 - 0.30 * drive,
        TransitionKind::Morph | TransitionKind::Remix | TransitionKind::Echo => 1.0 - 0.22 * drive,
        _ => 1.0 - 0.28 * drive,
    };
    Duration::from_millis((base * scale.max(0.45)) as u64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FractalZoomMode {
    Hypnotic,
    Balanced,
    Wormhole,
}

impl FractalZoomMode {
    fn next(self) -> Self {
        match self {
            Self::Hypnotic => Self::Balanced,
            Self::Balanced => Self::Wormhole,
            Self::Wormhole => Self::Hypnotic,
        }
    }

    fn multiplier(self) -> f32 {
        match self {
            Self::Hypnotic => 0.58,
            Self::Balanced => 1.0,
            Self::Wormhole => 1.72,
        }
    }
}

#[allow(dead_code)]
pub(crate) fn suggest_transition(
    audio: &AudioFeatures,
    seed: u32,
    mode: TransitionMode,
    last: TransitionKind,
) -> (Duration, TransitionKind) {
    suggest_transition_for_section(audio, seed, mode, last, classify_scene_section(audio))
}

pub(crate) fn suggest_transition_for_section(
    audio: &AudioFeatures,
    seed: u32,
    mode: TransitionMode,
    last: TransitionKind,
    section: SceneSection,
) -> (Duration, TransitionKind) {
    if mode != TransitionMode::Auto {
        let (base, span, pool): (u64, u64, &[TransitionKind]) = match mode {
            TransitionMode::Smooth => (
                1250,
                1100,
                &[
                    TransitionKind::Fade,
                    TransitionKind::Zoom,
                    TransitionKind::Swirl,
                    TransitionKind::Morph,
                    TransitionKind::Prism,
                    TransitionKind::Remix,
                ],
            ),
            TransitionMode::Punchy => (
                110,
                260,
                &[
                    TransitionKind::Cut,
                    TransitionKind::Flash,
                    TransitionKind::Datamosh,
                    TransitionKind::Wipe,
                    TransitionKind::Dissolve,
                    TransitionKind::Radial,
                ],
            ),
            TransitionMode::Morph => (
                760,
                980,
                &[
                    TransitionKind::Morph,
                    TransitionKind::Remix,
                    TransitionKind::Prism,
                    TransitionKind::Swirl,
                    TransitionKind::Luma,
                ],
            ),
            TransitionMode::Remix => (
                980,
                1250,
                &[
                    TransitionKind::Remix,
                    TransitionKind::Morph,
                    TransitionKind::Echo,
                    TransitionKind::Prism,
                    TransitionKind::Luma,
                ],
            ),
            TransitionMode::Cuts => (
                70,
                200,
                &[
                    TransitionKind::Cut,
                    TransitionKind::Flash,
                    TransitionKind::Wipe,
                    TransitionKind::Datamosh,
                ],
            ),
            TransitionMode::Auto => unreachable!(),
        };
        let dur = Duration::from_millis(base + (seed as u64 % span.max(1)));
        return (dur, pick_kind(seed.rotate_left(7), pool, last));
    }

    // Auto policy is section-driven so pacing and transition operators
    // stay aligned with stable scene energy.
    let treb = (audio.bands[5] + audio.bands[6] + audio.bands[7]) * (1.0 / 3.0);
    let hit = audio.onset.max(audio.beat_strength).max(treb);
    let transient = audio.onset.max(audio.beat_strength);
    let strong_transient =
        transient > 0.84 || (audio.beat && audio.beat_strength > 0.78 && audio.onset > 0.42);
    let very_strong_transient =
        transient > 0.92 || (audio.beat && audio.beat_strength > 0.90 && audio.onset > 0.62);
    match section {
        SceneSection::Calm => {
            let dur = Duration::from_millis(1500 + (seed as u64 % 980));
            let kind = pick_kind(
                seed ^ 0x9B54_4E2D,
                &[
                    TransitionKind::Morph,
                    TransitionKind::Remix,
                    TransitionKind::Echo,
                    TransitionKind::Swirl,
                    TransitionKind::Fade,
                    TransitionKind::Luma,
                ],
                last,
            );
            (dur, kind)
        }
        SceneSection::Groove => {
            if strong_transient && (seed & 3) == 0 {
                let dur = Duration::from_millis(260 + (seed as u64 % 220));
                let kind = pick_kind(
                    seed ^ 0x4D81_1FC3,
                    &[
                        TransitionKind::Datamosh,
                        TransitionKind::Wipe,
                        TransitionKind::Radial,
                        TransitionKind::Dissolve,
                        TransitionKind::Prism,
                    ],
                    last,
                );
                return (dur, kind);
            }

            let bass = audio.bands[1];
            let kind = if treb > 0.58 && hit > 0.35 {
                pick_kind(
                    seed ^ 0x73A4_4C9B,
                    &[
                        TransitionKind::Morph,
                        TransitionKind::Prism,
                        TransitionKind::Dissolve,
                        TransitionKind::Datamosh,
                        TransitionKind::Luma,
                    ],
                    last,
                )
            } else if audio.centroid > 0.58 {
                pick_kind(
                    seed ^ 0x1F4C_0AB7,
                    &[TransitionKind::Swirl, TransitionKind::Remix, TransitionKind::Zoom],
                    last,
                )
            } else if bass > 0.55 || (seed % 7) == 0 {
                pick_kind(
                    seed ^ 0x0BD5_EE21,
                    &[TransitionKind::Zoom, TransitionKind::Radial, TransitionKind::Wipe],
                    last,
                )
            } else {
                pick_kind(
                    seed ^ 0xA536_993D,
                    &[
                        TransitionKind::Morph,
                        TransitionKind::Remix,
                        TransitionKind::Fade,
                        TransitionKind::Luma,
                    ],
                    last,
                )
            };
            let base = if hit < 0.26 { 920 } else { 780 };
            (Duration::from_millis(base + (seed as u64 % 520)), kind)
        }
        SceneSection::Drive => {
            let hard_slot = (seed & 15) == 0 || (very_strong_transient && (seed & 7) == 0);
            if hard_slot && strong_transient {
                let mut kind = if very_strong_transient {
                    pick_kind(
                        seed ^ 0xE219_5A0D,
                        &[
                            TransitionKind::Cut,
                            TransitionKind::Flash,
                            TransitionKind::Datamosh,
                            TransitionKind::Wipe,
                        ],
                        last,
                    )
                } else {
                    pick_kind(
                        seed ^ 0x67CF_DA0B,
                        &[
                            TransitionKind::Datamosh,
                            TransitionKind::Wipe,
                            TransitionKind::Dissolve,
                            TransitionKind::Radial,
                        ],
                        last,
                    )
                };
                let mut dur = if very_strong_transient {
                    Duration::from_millis(90 + (seed as u64 % 90))
                } else {
                    Duration::from_millis(140 + (seed as u64 % 140))
                };
                if is_hard_cut(kind) && is_hard_cut(last) && !very_strong_transient {
                    kind = pick_kind(
                        seed ^ 0xAC6A_C329,
                        &[TransitionKind::Datamosh, TransitionKind::Wipe, TransitionKind::Radial],
                        last,
                    );
                    dur = Duration::from_millis(220 + (seed as u64 % 220));
                }
                return (dur, kind);
            }

            let dur = Duration::from_millis(260 + (seed as u64 % 320));
            let kind = pick_kind(
                seed ^ 0x6DA1_47C5,
                &[
                    TransitionKind::Datamosh,
                    TransitionKind::Wipe,
                    TransitionKind::Radial,
                    TransitionKind::Dissolve,
                    TransitionKind::Prism,
                    TransitionKind::Zoom,
                ],
                last,
            );
            (dur, kind)
        }
        SceneSection::Impact => {
            let mut dur = if very_strong_transient {
                Duration::from_millis(70 + (seed as u64 % 90))
            } else {
                Duration::from_millis(120 + (seed as u64 % 170))
            };
            let mut kind = pick_kind(
                seed ^ 0x2E91_31B7,
                &[
                    TransitionKind::Cut,
                    TransitionKind::Flash,
                    TransitionKind::Datamosh,
                    TransitionKind::Wipe,
                    TransitionKind::Radial,
                    TransitionKind::Dissolve,
                    TransitionKind::Prism,
                ],
                last,
            );
            if is_hard_cut(kind) && is_hard_cut(last) && !very_strong_transient {
                kind = pick_kind(
                    seed ^ 0xB41D_71AB,
                    &[TransitionKind::Datamosh, TransitionKind::Wipe, TransitionKind::Radial],
                    last,
                );
                dur = Duration::from_millis(180 + (seed as u64 % 180));
            }
            (dur, kind)
        }
    }
}

pub(crate) fn suggest_manual_transition(
    seed: u32,
    mode: TransitionMode,
    last: TransitionKind,
) -> TransitionKind {
    let pool: &[TransitionKind] = match mode {
        TransitionMode::Auto => &[
            TransitionKind::Fade,
            TransitionKind::Zoom,
            TransitionKind::Radial,
            TransitionKind::Swirl,
            TransitionKind::Dissolve,
            TransitionKind::Cut,
            TransitionKind::Morph,
            TransitionKind::Wipe,
            TransitionKind::Luma,
            TransitionKind::Flash,
            TransitionKind::Prism,
            TransitionKind::Remix,
            TransitionKind::Echo,
        ],
        TransitionMode::Smooth => &[
            TransitionKind::Fade,
            TransitionKind::Zoom,
            TransitionKind::Swirl,
            TransitionKind::Morph,
            TransitionKind::Remix,
            TransitionKind::Prism,
        ],
        TransitionMode::Punchy => &[
            TransitionKind::Cut,
            TransitionKind::Flash,
            TransitionKind::Wipe,
            TransitionKind::Datamosh,
            TransitionKind::Dissolve,
        ],
        TransitionMode::Morph => &[
            TransitionKind::Morph,
            TransitionKind::Remix,
            TransitionKind::Prism,
            TransitionKind::Luma,
            TransitionKind::Swirl,
        ],
        TransitionMode::Remix => &[
            TransitionKind::Remix,
            TransitionKind::Morph,
            TransitionKind::Echo,
            TransitionKind::Prism,
        ],
        TransitionMode::Cuts => &[
            TransitionKind::Cut,
            TransitionKind::Flash,
            TransitionKind::Wipe,
            TransitionKind::Datamosh,
        ],
    };
    pick_kind(seed ^ 0x7CA4_719D, pool, last)
}

pub(crate) fn is_fractal_preset_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("mandelbrot")
        || n.contains("julia")
        || n.contains("burning ship")
        || n.contains("orbit trap")
        || n.contains("fractal")
        || n.contains("flame")
        || n.contains("mandelbulb")
        || n.contains("sphere")
        || n.contains("sdf")
}

#[allow(dead_code)]
pub(crate) fn is_calm_section(audio: &AudioFeatures) -> bool {
    classify_scene_section(audio) == SceneSection::Calm
}

pub(crate) fn classify_scene_section(audio: &AudioFeatures) -> SceneSection {
    let bass = audio.bands[1].clamp(0.0, 1.0);
    let mid = audio.bands[3].clamp(0.0, 1.0);
    let treb = (audio.bands[5] + audio.bands[6] + audio.bands[7]) * (1.0 / 3.0);
    let transient = audio.onset.max(audio.beat_strength).clamp(0.0, 1.0);
    let pulse = (audio.rms * 0.42 + transient * 0.30 + bass * 0.18 + mid * 0.10).clamp(0.0, 1.0);

    if pulse < 0.24 && transient < 0.21 && !audio.beat && treb < 0.34 {
        SceneSection::Calm
    } else if pulse > 0.78
        || transient > 0.90
        || (audio.beat && audio.beat_strength > 0.86 && audio.onset > 0.58)
    {
        SceneSection::Impact
    } else if pulse > 0.54
        || transient > 0.62
        || (audio.beat && audio.beat_strength > 0.56)
        || bass > 0.66
    {
        SceneSection::Drive
    } else {
        SceneSection::Groove
    }
}

fn section_hysteresis_votes(from: SceneSection, to: SceneSection) -> u8 {
    let delta = to.intensity_rank() - from.intensity_rank();
    if delta >= 2 {
        2
    } else if delta > 0 {
        3
    } else if delta < 0 {
        4
    } else {
        1
    }
}

fn section_hysteresis_hold(from: SceneSection, to: SceneSection) -> Duration {
    match (from, to) {
        (SceneSection::Calm, SceneSection::Groove)
        | (SceneSection::Groove, SceneSection::Drive)
        | (SceneSection::Drive, SceneSection::Impact) => Duration::from_millis(650),
        (SceneSection::Impact, SceneSection::Drive)
        | (SceneSection::Drive, SceneSection::Groove)
        | (SceneSection::Groove, SceneSection::Calm) => Duration::from_millis(1650),
        _ => Duration::from_millis(1000),
    }
}

fn section_time_scale(section: SceneSection) -> f32 {
    match section {
        SceneSection::Calm => 1.35,
        SceneSection::Groove => 1.0,
        SceneSection::Drive => 0.78,
        SceneSection::Impact => 0.58,
    }
}

fn section_beats_per_switch(base: u32, section: SceneSection) -> u32 {
    match section {
        SceneSection::Calm => base.saturating_add(2).max(1),
        SceneSection::Groove => base.saturating_add(1).max(1),
        SceneSection::Drive => base.max(1),
        SceneSection::Impact => base.saturating_sub(1).max(1),
    }
}

fn section_energy_gate(section: SceneSection) -> (f32, f32) {
    match section {
        SceneSection::Calm => (0.28, 12.0),
        SceneSection::Groove => (0.34, 8.8),
        SceneSection::Drive => (0.40, 6.2),
        SceneSection::Impact => (0.46, 4.6),
    }
}

pub trait VisualEngine {
    fn resize(&mut self, w: usize, h: usize);
    fn preset_name(&self) -> &'static str;
    fn set_playlist_indices(&mut self, indices: &[usize]);
    fn set_shuffle(&mut self, on: bool);
    fn toggle_shuffle(&mut self);
    fn cycle_transition_mode(&mut self);
    fn transition_mode(&self) -> TransitionMode;
    fn transition_mode_name(&self) -> &'static str {
        self.transition_mode().label()
    }
    fn transition_kind_name(&self) -> &'static str;
    fn transition_operator_name(&self) -> &'static str {
        self.transition_kind_name()
    }
    fn transition_selection_name(&self) -> &'static str;
    fn transition_selection_locked(&self) -> bool;
    fn next_transition_kind(&mut self);
    fn prev_transition_kind(&mut self);
    fn scene_section_name(&self) -> &'static str {
        SceneSection::Groove.label()
    }
    fn cycle_camera_path_mode(&mut self) {}
    fn step_camera_path_mode(&mut self, _forward: bool) {}
    fn camera_path_mode(&self) -> CameraPathMode {
        CameraPathMode::Auto
    }
    fn camera_path_mode_name(&self) -> &'static str {
        self.camera_path_mode().label()
    }
    fn step_camera_path_speed(&mut self, _delta: f32) {}
    fn camera_path_speed(&self) -> f32 {
        1.0
    }
    fn toggle_fractal_bias(&mut self);
    fn fractal_bias(&self) -> bool;
    fn cycle_fractal_zoom_mode(&mut self);
    fn fractal_zoom_mode(&self) -> FractalZoomMode;
    fn set_fractal_zoom_drive(&mut self, v: f32);
    fn fractal_zoom_drive(&self) -> f32;
    fn toggle_fractal_zoom_enabled(&mut self);
    fn fractal_zoom_enabled(&self) -> bool;
    fn toggle_auto_switch(&mut self);
    fn set_switch_mode(&mut self, m: SwitchMode);
    fn switch_mode(&self) -> SwitchMode;
    fn shuffle(&self) -> bool;
    fn auto_switch(&self) -> bool;
    fn prev_preset(&mut self);
    fn next_preset(&mut self);
    fn update_auto_switch(&mut self, now: Instant, audio: &AudioFeatures);
    fn render(&mut self, ctx: RenderCtx, quality: Quality, scale: usize) -> &[u8];
}

pub(crate) struct PlaybackContext {
    pub playlist: Vec<usize>,
    pub active: usize,
    pub next: Option<usize>,
    pub shuffle: bool,
    pub switch_mode: SwitchMode,
    pub last_auto_mode: SwitchMode,
    pub beats_per_switch: u32,
    pub seconds_per_switch: f32,
    pub last_switch: Instant,
    pub beat_counter: u32,
    pub transition_started: Option<Instant>,
    pub transition_dur: Duration,
    pub transition_kind: TransitionKind,
    pub transition_seed: u32,
    pub transition_mode: TransitionMode,
    pub last_transition_kind: TransitionKind,
    pub transition_override: Option<TransitionKind>,
    pub fractal_zoom_mode: FractalZoomMode,
    pub fractal_zoom_drive: f32,
    pub fractal_zoom_enabled: bool,
    pub fractal_bias: bool,
    pub scene_section: SceneSection,
    pub scene_section_pending: SceneSection,
    pub scene_section_votes: u8,
    pub scene_section_changed_at: Instant,
    pub camera_path_mode: CameraPathMode,
    pub camera_path_speed: f32,
    preset_count: usize,
}

impl PlaybackContext {
    pub fn new(
        preset_count: usize,
        active: usize,
        shuffle: bool,
        switch_mode: SwitchMode,
        beats_per_switch: u32,
        seconds_per_switch: f32,
    ) -> Self {
        let now = Instant::now();
        let last_auto_mode = if switch_mode == SwitchMode::Manual {
            SwitchMode::Adaptive
        } else {
            switch_mode
        };
        Self {
            playlist: (0..preset_count).collect(),
            active: active.min(preset_count.saturating_sub(1)),
            next: None,
            shuffle,
            switch_mode,
            last_auto_mode,
            beats_per_switch: beats_per_switch.max(1),
            seconds_per_switch: seconds_per_switch.max(1.0),
            last_switch: now,
            beat_counter: 0,
            transition_started: None,
            transition_dur: Duration::from_millis(900),
            transition_kind: TransitionKind::Fade,
            transition_seed: fastrand::u32(..),
            transition_mode: TransitionMode::Auto,
            last_transition_kind: TransitionKind::Fade,
            transition_override: None,
            fractal_zoom_mode: FractalZoomMode::Balanced,
            fractal_zoom_drive: 1.0,
            fractal_zoom_enabled: true,
            fractal_bias: false,
            scene_section: SceneSection::Groove,
            scene_section_pending: SceneSection::Groove,
            scene_section_votes: 0,
            scene_section_changed_at: now,
            camera_path_mode: CameraPathMode::Auto,
            camera_path_speed: 1.0,
            preset_count,
        }
    }

    pub fn set_playlist_indices(&mut self, indices: &[usize]) {
        if self.preset_count == 0 {
            self.playlist.clear();
            return;
        }
        let mut seen = vec![false; self.preset_count];
        let mut playlist = Vec::with_capacity(indices.len().max(1));
        for &idx in indices {
            if idx < self.preset_count && !seen[idx] {
                seen[idx] = true;
                playlist.push(idx);
            }
        }
        if playlist.is_empty() {
            playlist.extend(0..self.preset_count);
        }
        self.playlist = playlist;
        if !self.playlist.contains(&self.active) {
            self.active = self.playlist[0];
            self.next = None;
            self.transition_started = None;
            self.transition_kind = TransitionKind::Fade;
            self.last_transition_kind = TransitionKind::Fade;
        }
    }

    pub fn set_shuffle(&mut self, on: bool) {
        self.shuffle = on;
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
    }

    pub fn cycle_transition_mode(&mut self) {
        self.transition_mode = self.transition_mode.next();
    }

    pub fn transition_mode(&self) -> TransitionMode {
        self.transition_mode
    }

    pub fn transition_kind_name(&self) -> &'static str {
        if let Some(k) = self.transition_override {
            k.label()
        } else {
            self.transition_kind.label()
        }
    }

    pub fn transition_selection_name(&self) -> &'static str {
        if let Some(k) = self.transition_override {
            k.label()
        } else {
            "Auto"
        }
    }

    pub fn transition_selection_locked(&self) -> bool {
        self.transition_override.is_some()
    }

    pub fn next_transition_kind(&mut self) {
        self.transition_override = step_transition_override(self.transition_override, true);
    }

    pub fn prev_transition_kind(&mut self) {
        self.transition_override = step_transition_override(self.transition_override, false);
    }

    pub fn scene_section_name(&self) -> &'static str {
        self.scene_section.label()
    }

    pub fn cycle_camera_path_mode(&mut self) {
        self.camera_path_mode = self.camera_path_mode.next();
    }

    pub fn step_camera_path_mode(&mut self, forward: bool) {
        self.camera_path_mode = self.camera_path_mode.step(forward);
    }

    pub fn camera_path_mode(&self) -> CameraPathMode {
        self.camera_path_mode
    }

    pub fn step_camera_path_speed(&mut self, delta: f32) {
        self.camera_path_speed = (self.camera_path_speed + delta).clamp(0.15, 4.0);
    }

    pub fn camera_path_speed(&self) -> f32 {
        self.camera_path_speed
    }

    pub fn toggle_fractal_bias(&mut self) {
        self.fractal_bias = !self.fractal_bias;
    }

    pub fn fractal_bias(&self) -> bool {
        self.fractal_bias
    }

    pub fn cycle_fractal_zoom_mode(&mut self) {
        self.fractal_zoom_mode = self.fractal_zoom_mode.next();
    }

    pub fn fractal_zoom_mode(&self) -> FractalZoomMode {
        self.fractal_zoom_mode
    }

    pub fn set_fractal_zoom_drive(&mut self, v: f32) {
        self.fractal_zoom_drive = v.clamp(0.12, 8.0);
    }

    pub fn fractal_zoom_drive(&self) -> f32 {
        self.fractal_zoom_drive
    }

    pub fn toggle_fractal_zoom_enabled(&mut self) {
        self.fractal_zoom_enabled = !self.fractal_zoom_enabled;
    }

    pub fn fractal_zoom_enabled(&self) -> bool {
        self.fractal_zoom_enabled
    }

    pub fn toggle_auto_switch(&mut self) {
        if self.switch_mode == SwitchMode::Manual {
            let m = if self.last_auto_mode == SwitchMode::Manual {
                SwitchMode::Adaptive
            } else {
                self.last_auto_mode
            };
            self.switch_mode = m;
        } else {
            self.last_auto_mode = self.switch_mode;
            self.switch_mode = SwitchMode::Manual;
        }
    }

    pub fn set_switch_mode(&mut self, m: SwitchMode) {
        self.switch_mode = m;
        if m != SwitchMode::Manual {
            self.last_auto_mode = m;
        }
    }

    pub fn switch_mode(&self) -> SwitchMode {
        self.switch_mode
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn auto_switch(&self) -> bool {
        self.switch_mode != SwitchMode::Manual
    }

    fn playlist_pos_for_active(&self) -> usize {
        self.playlist.iter().position(|&i| i == self.active).unwrap_or(0)
    }

    fn pick_shuffle(&mut self) -> usize {
        if self.playlist.is_empty() {
            return self.active;
        }
        if self.playlist.len() == 1 {
            return self.playlist[0];
        }
        let mut idx = self.playlist[fastrand::usize(..self.playlist.len())];
        if idx == self.active {
            let pos = self.playlist_pos_for_active();
            idx = self.playlist[(pos + 1) % self.playlist.len()];
        }
        idx
    }

    pub fn prev_preset(&mut self) {
        if self.playlist.is_empty() {
            return;
        }
        let pos = self.playlist_pos_for_active();
        let next = if pos == 0 {
            self.playlist[self.playlist.len() - 1]
        } else {
            self.playlist[pos - 1]
        };
        self.start_transition(next);
    }

    pub fn next_preset(&mut self) {
        if self.playlist.is_empty() {
            return;
        }
        let next = if self.shuffle {
            self.pick_shuffle()
        } else {
            let pos = self.playlist_pos_for_active();
            self.playlist[(pos + 1) % self.playlist.len()]
        };
        self.start_transition(next);
    }

    pub fn start_transition(&mut self, next: usize) {
        if next == self.active || self.preset_count == 0 {
            return;
        }
        self.transition_seed = fastrand::u32(..);
        self.transition_kind = if let Some(k) = self.transition_override {
            k
        } else {
            suggest_manual_transition(
                self.transition_seed,
                self.transition_mode,
                self.last_transition_kind,
            )
        };
        self.transition_dur = transition_base_duration(self.transition_kind);
        self.last_transition_kind = self.transition_kind;
        self.next = Some(next);
        self.transition_started = Some(Instant::now());
        self.last_switch = Instant::now();
        self.beat_counter = 0;
    }

    pub fn start_transition_with_dur(&mut self, next: usize, dur: Duration, kind: TransitionKind) {
        if next == self.active || self.preset_count == 0 {
            return;
        }
        self.transition_dur = dur.clamp(Duration::from_millis(80), Duration::from_millis(2600));
        self.transition_kind = kind;
        self.last_transition_kind = kind;
        self.transition_seed = fastrand::u32(..);
        self.next = Some(next);
        self.transition_started = Some(Instant::now());
        self.last_switch = Instant::now();
        self.beat_counter = 0;
    }

    pub fn pick_fractal_index(&mut self, name_of: impl Fn(usize) -> &'static str) -> Option<usize> {
        if self.playlist.len() <= 1 {
            return None;
        }
        let fractals = self
            .playlist
            .iter()
            .copied()
            .filter_map(|i| {
                if i != self.active && is_fractal_preset_name(name_of(i)) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if fractals.is_empty() {
            return None;
        }
        if self.shuffle {
            return Some(fractals[fastrand::usize(..fractals.len())]);
        }
        let pos = self.playlist_pos_for_active();
        for d in 1..self.playlist.len() {
            let idx = self.playlist[(pos + d) % self.playlist.len()];
            if idx != self.active && is_fractal_preset_name(name_of(idx)) {
                return Some(idx);
            }
        }
        None
    }

    fn next_preset_auto(&mut self, audio: &AudioFeatures, name_of: impl Fn(usize) -> &'static str) {
        if self.playlist.is_empty() {
            return;
        }
        let mut next = if self.shuffle {
            self.pick_shuffle()
        } else {
            let pos = self.playlist_pos_for_active();
            self.playlist[(pos + 1) % self.playlist.len()]
        };
        if self.fractal_bias
            && self.switch_mode == SwitchMode::Adaptive
            && self.scene_section == SceneSection::Calm
            && fastrand::f32() < 0.78
        {
            if let Some(fr) = self.pick_fractal_index(&name_of) {
                next = fr;
            }
        }
        let (mut dur, mut kind) = suggest_transition_for_section(
            audio,
            fastrand::u32(..),
            self.transition_mode,
            self.last_transition_kind,
            self.scene_section,
        );
        if let Some(k) = self.transition_override {
            kind = k;
            dur = transition_duration_for_kind(kind, audio);
        }
        self.start_transition_with_dur(next, dur, kind);
    }

    fn update_scene_section_state(&mut self, now: Instant, audio: &AudioFeatures) {
        let candidate = classify_scene_section(audio);
        if candidate == self.scene_section {
            self.scene_section_pending = candidate;
            self.scene_section_votes = 0;
            return;
        }
        if candidate != self.scene_section_pending {
            self.scene_section_pending = candidate;
            self.scene_section_votes = 1;
            return;
        }
        self.scene_section_votes = self.scene_section_votes.saturating_add(1);
        let min_votes = section_hysteresis_votes(self.scene_section, candidate);
        let min_hold = section_hysteresis_hold(self.scene_section, candidate);
        if self.scene_section_votes >= min_votes
            && now.duration_since(self.scene_section_changed_at) >= min_hold
        {
            self.scene_section = candidate;
            self.scene_section_pending = candidate;
            self.scene_section_votes = 0;
            self.scene_section_changed_at = now;
        }
    }

    pub fn update_auto_switch(
        &mut self,
        now: Instant,
        audio: &AudioFeatures,
        name_of: impl Fn(usize) -> &'static str,
    ) {
        self.update_scene_section_state(now, audio);

        if self.switch_mode == SwitchMode::Manual {
            return;
        }
        if self.transition_started.is_some() {
            return;
        }

        match self.switch_mode {
            SwitchMode::Manual => {}
            SwitchMode::Beat => {
                if audio.beat {
                    self.beat_counter = self.beat_counter.wrapping_add(1);
                    let beats_per =
                        section_beats_per_switch(self.beats_per_switch, self.scene_section);
                    if self.beat_counter % beats_per == 0 {
                        self.next_preset_auto(audio, &name_of);
                    }
                }
            }
            SwitchMode::Energy => {
                let e = audio.rms;
                let since = now.duration_since(self.last_switch).as_secs_f32();
                let (energy_gate, min_since) = section_energy_gate(self.scene_section);
                if e > energy_gate && since > min_since {
                    self.next_preset_auto(audio, &name_of);
                }
            }
            SwitchMode::Time => {
                let target = (self.seconds_per_switch * section_time_scale(self.scene_section))
                    .clamp(2.0, 60.0);
                if now.duration_since(self.last_switch).as_secs_f32() > target {
                    self.next_preset_auto(audio, &name_of);
                }
            }
            SwitchMode::Adaptive => {
                let since = now.duration_since(self.last_switch).as_secs_f32();
                let treb = (audio.bands[5] + audio.bands[6] + audio.bands[7]) * (1.0 / 3.0);
                let hit = audio.onset.max(audio.beat_strength).max(treb);
                let e = audio.rms;

                let pace_scale = section_time_scale(self.scene_section);
                let mut target =
                    self.seconds_per_switch * pace_scale * (1.25 - 0.7 * e) * (1.10 - 0.55 * hit);
                target = match self.scene_section {
                    SceneSection::Calm => target.clamp(5.5, 32.0),
                    SceneSection::Groove => target.clamp(4.0, 28.0),
                    SceneSection::Drive => target.clamp(3.2, 22.0),
                    SceneSection::Impact => target.clamp(2.2, 15.0),
                };

                let min_since = match self.scene_section {
                    SceneSection::Calm => 3.4,
                    SceneSection::Groove => 2.8,
                    SceneSection::Drive => 2.2,
                    SceneSection::Impact => 1.6,
                };
                let slam_gate = match self.scene_section {
                    SceneSection::Calm => 0.88,
                    SceneSection::Groove => 0.84,
                    SceneSection::Drive => 0.80,
                    SceneSection::Impact => 0.74,
                };
                let slam = (audio.beat && audio.beat_strength > slam_gate)
                    || audio.onset > (slam_gate - 0.04);
                if slam && since > min_since {
                    self.next_preset_auto(audio, &name_of);
                } else if since > target {
                    self.next_preset_auto(audio, &name_of);
                }
            }
        }
    }

    /// Advance transition state; returns current blend alpha (0.0 if no transition active).
    pub fn step_transition(&mut self, now: Instant) -> f32 {
        if let (Some(start), Some(next)) = (self.transition_started, self.next) {
            let t = now.duration_since(start).as_secs_f32() / self.transition_dur.as_secs_f32();
            if t >= 1.0 {
                self.active = next;
                self.next = None;
                self.transition_started = None;
                self.transition_kind = TransitionKind::Fade;
                0.0
            } else {
                t.clamp(0.0, 1.0)
            }
        } else {
            0.0
        }
    }

    pub fn fractal_zoom_mul(&self) -> f32 {
        if self.fractal_zoom_enabled {
            self.fractal_zoom_mode.multiplier() * self.fractal_zoom_drive
        } else {
            -1.0
        }
    }
}

pub struct PresetEngine {
    presets: Vec<Box<dyn Preset>>,
    ctx: PlaybackContext,

    // Buffers
    front: Vec<u8>,
    back: Vec<u8>,
    tmp_a: Vec<u8>,
    tmp_b: Vec<u8>,
    w: usize,
    h: usize,
}

impl PresetEngine {
    pub fn new(
        presets: Vec<Box<dyn Preset>>,
        active: usize,
        shuffle: bool,
        switch_mode: SwitchMode,
        beats_per_switch: u32,
        seconds_per_switch: f32,
    ) -> Self {
        let preset_count = presets.len();
        Self {
            ctx: PlaybackContext::new(
                preset_count,
                active,
                shuffle,
                switch_mode,
                beats_per_switch,
                seconds_per_switch,
            ),
            presets,
            front: Vec::new(),
            back: Vec::new(),
            tmp_a: Vec::new(),
            tmp_b: Vec::new(),
            w: 0,
            h: 0,
        }
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        self.w = w;
        self.h = h;
        let n = w.saturating_mul(h).saturating_mul(4);
        self.front.resize(n, 0);
        self.back.resize(n, 0);
        self.tmp_a.resize(n, 0);
        self.tmp_b.resize(n, 0);
        self.clear();
        for p in &mut self.presets {
            p.on_resize(w, h);
        }
    }

    pub fn clear(&mut self) {
        self.front.fill(0);
        self.back.fill(0);
        self.tmp_a.fill(0);
        self.tmp_b.fill(0);
    }

    pub fn preset_name(&self) -> &'static str {
        self.presets
            .get(self.ctx.active)
            .map(|p| p.name())
            .unwrap_or("<none>")
    }

    pub fn render(
        &mut self,
        mut ctx: RenderCtx,
        quality: Quality,
        scale: usize,
    ) -> &[u8] {
        if self.w == 0 || self.h == 0 || self.presets.is_empty() {
            return &self.front;
        }

        ctx.quality = quality;
        ctx.scale = scale.max(1);
        ctx.fractal_zoom_mul = self.ctx.fractal_zoom_mul();

        let alpha = self.ctx.step_transition(ctx.now);

        if alpha == 0.0 {
            self.presets[self.ctx.active].render(&ctx, &self.front, &mut self.back);
        } else {
            let next = self.ctx.next.unwrap_or(self.ctx.active);
            self.presets[self.ctx.active].render(&ctx, &self.front, &mut self.tmp_a);
            self.presets[next].render(&ctx, &self.front, &mut self.tmp_b);
            blend_transition(
                self.ctx.transition_kind,
                &self.front,
                &self.tmp_a,
                &self.tmp_b,
                alpha,
                ctx.t,
                &ctx.audio,
                self.ctx.transition_seed,
                self.w,
                self.h,
                &mut self.back,
            );
        }

        std::mem::swap(&mut self.front, &mut self.back);
        &self.front
    }
}

impl VisualEngine for PresetEngine {
    fn resize(&mut self, w: usize, h: usize) {
        PresetEngine::resize(self, w, h)
    }

    fn preset_name(&self) -> &'static str {
        PresetEngine::preset_name(self)
    }

    fn set_playlist_indices(&mut self, indices: &[usize]) {
        self.ctx.set_playlist_indices(indices)
    }

    fn set_shuffle(&mut self, on: bool) { self.ctx.set_shuffle(on) }
    fn toggle_shuffle(&mut self) { self.ctx.toggle_shuffle() }
    fn cycle_transition_mode(&mut self) { self.ctx.cycle_transition_mode() }
    fn transition_mode(&self) -> TransitionMode { self.ctx.transition_mode() }
    fn transition_kind_name(&self) -> &'static str { self.ctx.transition_kind_name() }
    fn transition_selection_name(&self) -> &'static str { self.ctx.transition_selection_name() }
    fn transition_selection_locked(&self) -> bool { self.ctx.transition_selection_locked() }
    fn next_transition_kind(&mut self) { self.ctx.next_transition_kind() }
    fn prev_transition_kind(&mut self) { self.ctx.prev_transition_kind() }
    fn scene_section_name(&self) -> &'static str { self.ctx.scene_section_name() }
    fn cycle_camera_path_mode(&mut self) { self.ctx.cycle_camera_path_mode() }
    fn step_camera_path_mode(&mut self, forward: bool) { self.ctx.step_camera_path_mode(forward) }
    fn camera_path_mode(&self) -> CameraPathMode { self.ctx.camera_path_mode() }
    fn step_camera_path_speed(&mut self, delta: f32) { self.ctx.step_camera_path_speed(delta) }
    fn camera_path_speed(&self) -> f32 { self.ctx.camera_path_speed() }
    fn toggle_fractal_bias(&mut self) { self.ctx.toggle_fractal_bias() }
    fn fractal_bias(&self) -> bool { self.ctx.fractal_bias() }
    fn cycle_fractal_zoom_mode(&mut self) { self.ctx.cycle_fractal_zoom_mode() }
    fn fractal_zoom_mode(&self) -> FractalZoomMode { self.ctx.fractal_zoom_mode() }
    fn set_fractal_zoom_drive(&mut self, v: f32) { self.ctx.set_fractal_zoom_drive(v) }
    fn fractal_zoom_drive(&self) -> f32 { self.ctx.fractal_zoom_drive() }
    fn toggle_fractal_zoom_enabled(&mut self) { self.ctx.toggle_fractal_zoom_enabled() }
    fn fractal_zoom_enabled(&self) -> bool { self.ctx.fractal_zoom_enabled() }
    fn toggle_auto_switch(&mut self) { self.ctx.toggle_auto_switch() }
    fn set_switch_mode(&mut self, m: SwitchMode) { self.ctx.set_switch_mode(m) }
    fn switch_mode(&self) -> SwitchMode { self.ctx.switch_mode() }
    fn shuffle(&self) -> bool { self.ctx.shuffle() }
    fn auto_switch(&self) -> bool { self.ctx.auto_switch() }
    fn prev_preset(&mut self) { self.ctx.prev_preset() }
    fn next_preset(&mut self) { self.ctx.next_preset() }

    fn update_auto_switch(&mut self, now: Instant, audio: &AudioFeatures) {
        let presets = &self.presets;
        self.ctx.update_auto_switch(now, audio, |i| presets[i].name())
    }

    fn render(&mut self, ctx: RenderCtx, quality: Quality, scale: usize) -> &[u8] {
        PresetEngine::render(self, ctx, quality, scale)
    }
}

fn blend_rgba(a: &[u8], b: &[u8], t: f32, out: &mut [u8]) {
    let t = t.clamp(0.0, 1.0);
    let it = 1.0 - t;
    for i in (0..out.len()).step_by(4) {
        let ar = a[i] as f32;
        let ag = a[i + 1] as f32;
        let ab = a[i + 2] as f32;
        let br = b[i] as f32;
        let bg = b[i + 1] as f32;
        let bb = b[i + 2] as f32;
        out[i] = (ar * it + br * t) as u8;
        out[i + 1] = (ag * it + bg * t) as u8;
        out[i + 2] = (ab * it + bb * t) as u8;
        out[i + 3] = 255;
    }
}

fn blend_transition(
    kind: TransitionKind,
    prev_frame: &[u8],
    a: &[u8],
    b: &[u8],
    alpha: f32,
    t: f32,
    audio: &AudioFeatures,
    seed: u32,
    w: usize,
    h: usize,
    out: &mut [u8],
) {
    match kind {
        TransitionKind::Fade => blend_rgba(a, b, alpha, out),
        TransitionKind::Zoom => blend_zoom_rgba(a, b, alpha, w, h, out),
        TransitionKind::Radial => blend_radial_rgba(a, b, alpha, w, h, t, audio, out),
        TransitionKind::Swirl => blend_swirl_rgba(a, b, alpha, w, h, t, audio, out),
        TransitionKind::Dissolve => blend_dissolve_rgba(a, b, alpha, w, h, t, audio, seed, out),
        TransitionKind::Cut => blend_cut_rgba(a, b, alpha, w, h, t, audio, out),
        TransitionKind::Morph => blend_morph_rgba(a, b, alpha, w, h, t, audio, out),
        TransitionKind::Wipe => blend_wipe_rgba(a, b, alpha, w, h, t, audio, seed, out),
        TransitionKind::Luma => blend_luma_rgba(a, b, alpha, w, h, t, audio, seed, out),
        TransitionKind::Flash => blend_flash_rgba(a, b, alpha, w, h, t, audio, out),
        TransitionKind::Prism => blend_prism_rgba(a, b, alpha, w, h, t, audio, out),
        TransitionKind::Remix => blend_remix_rgba(prev_frame, a, b, alpha, w, h, t, audio, out),
        TransitionKind::Echo => blend_echo_rgba(prev_frame, a, b, alpha, w, h, t, audio, out),
        TransitionKind::Datamosh => {
            blend_rgba(a, b, alpha, out);
            datamosh_overlay(prev_frame, w, h, out, alpha, t, audio, seed);
        }
    }
}

fn blend_zoom_rgba(a: &[u8], b: &[u8], alpha: f32, w: usize, h: usize, out: &mut [u8]) {
    let alpha = alpha.clamp(0.0, 1.0);
    let k = 0.55;
    let az = 1.0 + alpha * k; // active zooms out as it leaves
    let bz = 1.0 + (1.0 - alpha) * k; // next zooms in as it arrives

    let wf = (w.max(1) as f32).max(1.0);
    let hf = (h.max(1) as f32).max(1.0);

    for y in 0..h {
        let ny = ((y as f32 + 0.5) / hf) * 2.0 - 1.0;
        for x in 0..w {
            let nx = ((x as f32 + 0.5) / wf) * 2.0 - 1.0;
            let ca = sample_rgba(a, w, h, nx * az, ny * az);
            let cb = sample_rgba(b, w, h, nx / bz, ny / bz);
            let i = (y * w + x) * 4;
            out[i] = lerp_u8(ca[0], cb[0], alpha);
            out[i + 1] = lerp_u8(ca[1], cb[1], alpha);
            out[i + 2] = lerp_u8(ca[2], cb[2], alpha);
            out[i + 3] = 255;
        }
    }
}

fn blend_radial_rgba(
    a: &[u8],
    b: &[u8],
    alpha: f32,
    w: usize,
    h: usize,
    t: f32,
    audio: &AudioFeatures,
    out: &mut [u8],
) {
    let alpha = alpha.clamp(0.0, 1.0);
    let bass = audio.bands[1].clamp(0.0, 1.0);
    let treb = ((audio.bands[5] + audio.bands[6] + audio.bands[7]) * (1.0 / 3.0)).clamp(0.0, 1.0);
    let feather = 0.035 + 0.11 * bass;

    let wf = w.max(1) as f32;
    let hf = h.max(1) as f32;
    for y in 0..h {
        let ny = ((y as f32 + 0.5) / hf) * 2.0 - 1.0;
        for x in 0..w {
            let nx = ((x as f32 + 0.5) / wf) * 2.0 - 1.0;
            let r = (nx * nx + ny * ny).sqrt() * 0.70710677;
            let theta = ny.atan2(nx);
            let ripple = (theta * 8.0 + t * (2.0 + 6.0 * treb)).sin() * (0.02 + 0.04 * treb) * (1.0 - alpha);
            let thr = (alpha + ripple).clamp(0.0, 1.0);
            let bmask = 1.0 - smoothstep(thr - feather, thr + feather, r);

            let i = (y * w + x) * 4;
            out[i] = lerp_u8(a[i], b[i], bmask);
            out[i + 1] = lerp_u8(a[i + 1], b[i + 1], bmask);
            out[i + 2] = lerp_u8(a[i + 2], b[i + 2], bmask);
            out[i + 3] = 255;
        }
    }
}

fn blend_swirl_rgba(
    a: &[u8],
    b: &[u8],
    alpha: f32,
    w: usize,
    h: usize,
    t: f32,
    audio: &AudioFeatures,
    out: &mut [u8],
) {
    let alpha = alpha.clamp(0.0, 1.0);
    let bass = audio.bands[1].clamp(0.0, 1.0);
    let mid = audio.bands[3].clamp(0.0, 1.0);
    let spin = (1.2 + 2.6 * bass + 1.4 * mid) * (1.0 + 0.3 * (t * 0.7).sin());

    let wf = w.max(1) as f32;
    let hf = h.max(1) as f32;
    for y in 0..h {
        let ny = ((y as f32 + 0.5) / hf) * 2.0 - 1.0;
        for x in 0..w {
            let nx = ((x as f32 + 0.5) / wf) * 2.0 - 1.0;
            let r = (nx * nx + ny * ny).sqrt().clamp(0.0, 1.4);
            let falloff = (1.0 - (r * 0.85).clamp(0.0, 1.0)).powf(1.1);
            let a_ang = spin * falloff * (1.0 - alpha);
            let b_ang = -spin * falloff * alpha;

            let sa = rotate2(nx, ny, a_ang);
            let sb = rotate2(nx, ny, b_ang);
            let ca = sample_rgba(a, w, h, sa.0, sa.1);
            let cb = sample_rgba(b, w, h, sb.0, sb.1);

            let i = (y * w + x) * 4;
            out[i] = lerp_u8(ca[0], cb[0], alpha);
            out[i + 1] = lerp_u8(ca[1], cb[1], alpha);
            out[i + 2] = lerp_u8(ca[2], cb[2], alpha);
            out[i + 3] = 255;
        }
    }
}

fn blend_dissolve_rgba(
    a: &[u8],
    b: &[u8],
    alpha: f32,
    w: usize,
    h: usize,
    t: f32,
    audio: &AudioFeatures,
    seed: u32,
    out: &mut [u8],
) {
    let alpha = alpha.clamp(0.0, 1.0);
    let treb = ((audio.bands[5] + audio.bands[6] + audio.bands[7]) * (1.0 / 3.0)).clamp(0.0, 1.0);
    let drive = (audio.onset + audio.beat_strength + treb * 0.7).clamp(0.0, 1.0);
    let blocks = (8.0 + 48.0 * drive).clamp(8.0, 64.0) as usize;
    let feather = (0.08 - 0.05 * drive).clamp(0.015, 0.09);

    let hf = h.max(1) as f32;
    for y in 0..h {
        let by = (y.saturating_mul(blocks) / h.max(1)) as u32;
        let scan = ((y as f32 / hf) * 38.0 + t * (9.0 + 28.0 * drive)).sin() * (0.02 + 0.04 * (1.0 - alpha));
        for x in 0..w {
            let bx = (x.saturating_mul(blocks) / w.max(1)) as u32;
            let n_block = (hash_u32(bx, by, seed) as f32) * (1.0 / 4_294_967_295.0);
            let n_px = (hash_u32(x as u32, y as u32, seed ^ 0xA7C1_53E9) as f32) * (1.0 / 4_294_967_295.0);
            let noise = (0.82 * n_block + 0.18 * n_px + scan).clamp(0.0, 1.0);
            let thr = alpha;
            let bmask = 1.0 - smoothstep(thr - feather, thr + feather, noise);

            let i = (y * w + x) * 4;
            out[i] = lerp_u8(a[i], b[i], bmask);
            out[i + 1] = lerp_u8(a[i + 1], b[i + 1], bmask);
            out[i + 2] = lerp_u8(a[i + 2], b[i + 2], bmask);
            out[i + 3] = 255;
        }
    }
}

fn blend_cut_rgba(
    a: &[u8],
    b: &[u8],
    alpha: f32,
    w: usize,
    h: usize,
    t: f32,
    audio: &AudioFeatures,
    out: &mut [u8],
) {
    let alpha = alpha.clamp(0.0, 1.0);
    let drive = (audio.onset + audio.beat_strength + audio.bands[1] * 0.5).clamp(0.0, 1.0);
    let gate = (alpha * (1.35 + 0.65 * drive)).clamp(0.0, 1.0);
    let tear_amp = (1.0 - alpha) * (0.025 + 0.08 * drive);
    let wf = w.max(1) as f32;
    let hf = h.max(1) as f32;

    for y in 0..h {
        let ny = (y as f32 + 0.5) / hf;
        let tear = (ny * 46.0 + t * (14.0 + 52.0 * drive)).sin() * tear_amp;
        for x in 0..w {
            let nx = (x as f32 + 0.5) / wf;
            let idx = (y * w + x) * 4;
            let choose_b = (nx + tear) < gate;
            if choose_b {
                out[idx] = b[idx];
                out[idx + 1] = b[idx + 1];
                out[idx + 2] = b[idx + 2];
            } else {
                out[idx] = a[idx];
                out[idx + 1] = a[idx + 1];
                out[idx + 2] = a[idx + 2];
            }
            out[idx + 3] = 255;
        }
    }
}

fn blend_morph_rgba(
    a: &[u8],
    b: &[u8],
    alpha: f32,
    w: usize,
    h: usize,
    t: f32,
    audio: &AudioFeatures,
    out: &mut [u8],
) {
    let alpha = alpha.clamp(0.0, 1.0);
    let eased = smoothstep(0.0, 1.0, alpha);
    let bass = audio.bands[1].clamp(0.0, 1.0);
    let mid = audio.bands[3].clamp(0.0, 1.0);
    let treb = ((audio.bands[5] + audio.bands[6] + audio.bands[7]) * (1.0 / 3.0)).clamp(0.0, 1.0);
    let amp = (0.06 + 0.22 * (bass * 0.6 + mid * 0.25 + treb * 0.15)) * (1.0 - 0.45 * eased);
    let wf = w.max(1) as f32;
    let hf = h.max(1) as f32;

    for y in 0..h {
        let ny = ((y as f32 + 0.5) / hf) * 2.0 - 1.0;
        for x in 0..w {
            let nx = ((x as f32 + 0.5) / wf) * 2.0 - 1.0;
            let flow_x = ((ny * 5.2 + t * 1.7).sin() + (nx * 3.3 - t * 1.2).cos()) * amp;
            let flow_y = ((nx * 4.7 - t * 1.4).sin() - (ny * 3.8 + t * 1.6).cos()) * amp;
            let wa = 1.0 - eased;
            let wb = eased;
            let ca = sample_rgba(a, w, h, nx + flow_x * wa, ny + flow_y * wa);
            let cb = sample_rgba(b, w, h, nx - flow_x * wb, ny - flow_y * wb);

            let edge = 0.5 + 0.5 * ((nx * 7.0 + ny * 5.0 + t * (2.0 + 5.0 * treb)).sin());
            let mix_t = (eased * 0.78 + edge * 0.22 * (1.0 - alpha)).clamp(0.0, 1.0);
            let i = (y * w + x) * 4;
            out[i] = lerp_u8(ca[0], cb[0], mix_t);
            out[i + 1] = lerp_u8(ca[1], cb[1], mix_t);
            out[i + 2] = lerp_u8(ca[2], cb[2], mix_t);
            out[i + 3] = 255;
        }
    }
}

fn blend_wipe_rgba(
    a: &[u8],
    b: &[u8],
    alpha: f32,
    w: usize,
    h: usize,
    t: f32,
    audio: &AudioFeatures,
    seed: u32,
    out: &mut [u8],
) {
    let alpha = alpha.clamp(0.0, 1.0);
    let drive = (audio.onset + audio.beat_strength + audio.bands[1] * 0.55).clamp(0.0, 1.0);
    let seed_phase = ((seed >> 11) & 1023) as f32 * (1.0 / 1024.0) * std::f32::consts::TAU;
    let ang = t * (0.35 + 0.8 * drive) + seed_phase;
    let dir = (ang.cos(), ang.sin());
    let feather = 0.05 + 0.09 * (1.0 - drive);
    let threshold = alpha * 2.0 - 1.0;
    let wf = w.max(1) as f32;
    let hf = h.max(1) as f32;

    for y in 0..h {
        let ny = ((y as f32 + 0.5) / hf) * 2.0 - 1.0;
        for x in 0..w {
            let nx = ((x as f32 + 0.5) / wf) * 2.0 - 1.0;
            let wave = ((nx * 12.0 + ny * 9.0) + t * (5.0 + 9.0 * drive)).sin() * 0.06 * (1.0 - alpha);
            let d = nx * dir.0 + ny * dir.1 + wave;
            let mask = smoothstep(threshold - feather, threshold + feather, d);
            let i = (y * w + x) * 4;
            out[i] = lerp_u8(a[i], b[i], mask);
            out[i + 1] = lerp_u8(a[i + 1], b[i + 1], mask);
            out[i + 2] = lerp_u8(a[i + 2], b[i + 2], mask);
            out[i + 3] = 255;
        }
    }
}

fn blend_luma_rgba(
    a: &[u8],
    b: &[u8],
    alpha: f32,
    w: usize,
    h: usize,
    t: f32,
    audio: &AudioFeatures,
    seed: u32,
    out: &mut [u8],
) {
    let alpha = alpha.clamp(0.0, 1.0);
    let drive = (audio.onset + audio.beat_strength + audio.bands[5] * 0.45).clamp(0.0, 1.0);
    let feather = (0.09 - 0.05 * drive).clamp(0.02, 0.1);
    let noise_amp = 0.08 + 0.12 * (1.0 - alpha);

    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let lum = (0.2126 * (b[i] as f32) + 0.7152 * (b[i + 1] as f32) + 0.0722 * (b[i + 2] as f32))
                * (1.0 / 255.0);
            let n = (hash_u32(x as u32, y as u32, seed ^ 0xA9A7_5D3C) as f32) * (1.0 / 4_294_967_295.0);
            let scan = (((x as f32) * 0.011 + (y as f32) * 0.014) + t * (0.5 + 1.2 * drive)).sin() * 0.06;
            let lv = (lum + (n - 0.5) * noise_amp + scan).clamp(0.0, 1.0);
            let mask = smoothstep(alpha - feather, alpha + feather, lv);
            out[i] = lerp_u8(a[i], b[i], mask);
            out[i + 1] = lerp_u8(a[i + 1], b[i + 1], mask);
            out[i + 2] = lerp_u8(a[i + 2], b[i + 2], mask);
            out[i + 3] = 255;
        }
    }
}

fn blend_flash_rgba(
    a: &[u8],
    b: &[u8],
    alpha: f32,
    w: usize,
    h: usize,
    _t: f32,
    audio: &AudioFeatures,
    out: &mut [u8],
) {
    let alpha = alpha.clamp(0.0, 1.0);
    let gate = smoothstep(0.20, 0.80, alpha);
    let drive = (audio.onset + audio.beat_strength + audio.bands[1] * 0.45).clamp(0.0, 1.0);
    let flash = (1.0 - (alpha * 2.0 - 1.0).abs()).powf(1.65) * (0.35 + 0.55 * drive);

    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let mut r = lerp_u8(a[i], b[i], gate) as f32;
            let mut g = lerp_u8(a[i + 1], b[i + 1], gate) as f32;
            let mut bb = lerp_u8(a[i + 2], b[i + 2], gate) as f32;
            let boost = 255.0 * flash;
            r = (r + boost).clamp(0.0, 255.0);
            g = (g + boost).clamp(0.0, 255.0);
            bb = (bb + boost).clamp(0.0, 255.0);
            out[i] = r as u8;
            out[i + 1] = g as u8;
            out[i + 2] = bb as u8;
            out[i + 3] = 255;
        }
    }
}

fn blend_prism_rgba(
    a: &[u8],
    b: &[u8],
    alpha: f32,
    w: usize,
    h: usize,
    t: f32,
    audio: &AudioFeatures,
    out: &mut [u8],
) {
    let alpha = alpha.clamp(0.0, 1.0);
    let drive = (audio.bands[5] + audio.bands[6] + audio.onset * 0.6).clamp(0.0, 1.0);
    let eased = smoothstep(0.0, 1.0, alpha);
    let split = (0.02 + 0.11 * drive) * (1.0 - 0.35 * eased);
    let wf = w.max(1) as f32;
    let hf = h.max(1) as f32;

    for y in 0..h {
        let ny = ((y as f32 + 0.5) / hf) * 2.0 - 1.0;
        for x in 0..w {
            let nx = ((x as f32 + 0.5) / wf) * 2.0 - 1.0;
            let tw = ((nx * 8.0 - ny * 6.0) + t * (2.0 + 5.0 * drive)).sin() * 0.06 * (1.0 - alpha);

            let ar = sample_rgba(a, w, h, nx + split + tw, ny);
            let ag = sample_rgba(a, w, h, nx, ny);
            let ab = sample_rgba(a, w, h, nx - split - tw, ny);

            let br = sample_rgba(b, w, h, nx - split * 0.6 - tw * 0.4, ny);
            let bg = sample_rgba(b, w, h, nx, ny);
            let bb = sample_rgba(b, w, h, nx + split * 0.6 + tw * 0.4, ny);

            let i = (y * w + x) * 4;
            out[i] = lerp_u8(ar[0], br[0], eased);
            out[i + 1] = lerp_u8(ag[1], bg[1], eased);
            out[i + 2] = lerp_u8(ab[2], bb[2], eased);
            out[i + 3] = 255;
        }
    }
}

fn blend_remix_rgba(
    prev: &[u8],
    a: &[u8],
    b: &[u8],
    alpha: f32,
    w: usize,
    h: usize,
    t: f32,
    audio: &AudioFeatures,
    out: &mut [u8],
) {
    let alpha = alpha.clamp(0.0, 1.0);
    let eased = smoothstep(0.0, 1.0, alpha);
    let bass = audio.bands[1].clamp(0.0, 1.0);
    let mid = audio.bands[3].clamp(0.0, 1.0);
    let treb = ((audio.bands[5] + audio.bands[6] + audio.bands[7]) * (1.0 / 3.0)).clamp(0.0, 1.0);
    let drive = (bass * 0.45 + mid * 0.35 + treb * 0.2 + audio.onset * 0.35).clamp(0.0, 1.0);
    let amp = (0.05 + 0.22 * drive) * (1.0 - 0.3 * eased);
    let feedback = ((1.0 - alpha) * (0.08 + 0.32 * drive)).clamp(0.0, 0.78);
    let has_prev = prev.len() >= w.saturating_mul(h).saturating_mul(4);
    let wf = w.max(1) as f32;
    let hf = h.max(1) as f32;

    for y in 0..h {
        let ny = ((y as f32 + 0.5) / hf) * 2.0 - 1.0;
        for x in 0..w {
            let nx = ((x as f32 + 0.5) / wf) * 2.0 - 1.0;
            let fx = ((ny * 5.8 + t * 1.8).sin() + (nx * 3.2 - t * 1.1).cos()) * amp;
            let fy = ((nx * 4.4 - t * 1.4).sin() - (ny * 3.9 + t * 1.2).cos()) * amp;

            let ca = sample_rgba(a, w, h, nx + fx * (1.0 - eased), ny + fy * (1.0 - eased));
            let cb = sample_rgba(b, w, h, nx - fy * eased, ny + fx * eased);

            let grid = 0.5 + 0.5 * ((nx * 9.0 - ny * 7.0) + t * (2.0 + 6.0 * drive)).sin();
            let blend = (eased * 0.64 + grid * 0.36 + (fx - fy) * 0.2).clamp(0.0, 1.0);
            let i = (y * w + x) * 4;

            let mut r = lerp_u8(ca[0], cb[0], blend) as f32;
            let mut g = lerp_u8(ca[1], cb[1], blend) as f32;
            let mut bb = lerp_u8(ca[2], cb[2], blend) as f32;

            if has_prev {
                let pr = prev[i + 2] as f32;
                let pg = prev[i + 1] as f32;
                let pb = prev[i] as f32;
                r = r * (1.0 - feedback) + pr * feedback;
                g = g * (1.0 - feedback) + pg * feedback;
                bb = bb * (1.0 - feedback) + pb * feedback;
            }

            out[i] = r.clamp(0.0, 255.0) as u8;
            out[i + 1] = g.clamp(0.0, 255.0) as u8;
            out[i + 2] = bb.clamp(0.0, 255.0) as u8;
            out[i + 3] = 255;
        }
    }
}

fn blend_echo_rgba(
    prev: &[u8],
    a: &[u8],
    b: &[u8],
    alpha: f32,
    w: usize,
    h: usize,
    t: f32,
    audio: &AudioFeatures,
    out: &mut [u8],
) {
    blend_rgba(a, b, alpha, out);
    if prev.len() < w.saturating_mul(h).saturating_mul(4) {
        return;
    }

    let alpha = alpha.clamp(0.0, 1.0);
    let drive = (audio.bands[1] + audio.beat_strength + audio.onset * 0.8).clamp(0.0, 1.0);
    let mix_amt = ((1.0 - alpha) * (0.14 + 0.58 * drive)).clamp(0.0, 0.88);
    let wf = w.max(1) as f32;
    let hf = h.max(1) as f32;

    for y in 0..h {
        let ny = (y as f32 + 0.5) / hf;
        let ox = (((ny * 68.0) + t * (14.0 + 20.0 * drive)).sin() * (0.01 + 0.05 * (1.0 - alpha)))
            * wf;
        for x in 0..w {
            let oy = ((((x as f32 + 0.5) / wf) * 62.0) - t * (13.0 + 18.0 * drive)).cos()
                * (0.01 + 0.04 * (1.0 - alpha))
                * hf;
            let sx = (x as isize + ox as isize).clamp(0, (w as isize) - 1) as usize;
            let sy = (y as isize + oy as isize).clamp(0, (h as isize) - 1) as usize;
            let si = (sy * w + sx) * 4;
            let di = (y * w + x) * 4;
            out[di] = lerp_u8(out[di], prev[si], mix_amt);
            out[di + 1] = lerp_u8(out[di + 1], prev[si + 1], mix_amt);
            out[di + 2] = lerp_u8(out[di + 2], prev[si + 2], mix_amt);
            out[di + 3] = 255;
        }
    }
}

fn datamosh_overlay(
    prev: &[u8],
    w: usize,
    h: usize,
    out: &mut [u8],
    alpha: f32,
    t: f32,
    audio: &AudioFeatures,
    seed: u32,
) {
    if prev.len() < w.saturating_mul(h).saturating_mul(4) {
        return;
    }

    let alpha = alpha.clamp(0.0, 1.0);
    let treb = (audio.bands[5] + audio.bands[6] + audio.bands[7]) * (1.0 / 3.0);
    let drive = (audio.onset + audio.beat_strength + treb * 0.7).clamp(0.0, 1.0);

    // More treble/onset -> smaller blocks and stronger displacement.
    let block = (10.0 - 6.0 * drive).clamp(3.0, 12.0) as usize;
    let amp = ((1.0 - alpha) * (2.0 + 24.0 * drive)).clamp(0.0, 30.0);
    let mix_amt = ((1.0 - alpha) * (0.25 + 0.65 * drive)).clamp(0.0, 0.92);

    for y in 0..h {
        let tear = ((y as f32 / h.max(1) as f32) * 40.0 + t * (10.0 + 70.0 * drive)).sin();
        let tear_x = (tear * (amp * 0.35)) as isize;

        for x in 0..w {
            let bx = x / block;
            let by = y / block;
            let r = hash_u32(bx as u32, by as u32, seed);

            let rx = ((r & 0xFF) as f32 / 255.0) * 2.0 - 1.0;
            let ry = (((r >> 8) & 0xFF) as f32 / 255.0) * 2.0 - 1.0;
            let ox = (rx * amp) as isize + tear_x;
            let oy = (ry * amp) as isize;

            let sx = (x as isize + ox).clamp(0, (w as isize) - 1) as usize;
            let sy = (y as isize + oy).clamp(0, (h as isize) - 1) as usize;

            let si = (sy * w + sx) * 4;
            let di = (y * w + x) * 4;

            // Subtle chromatic split.
            let ca = (1 + (drive * 2.0) as usize).min(4);
            let sxr = (sx + ca).min(w.saturating_sub(1));
            let sxb = sx.saturating_sub(ca);
            let sir = (sy * w + sxr) * 4;
            let sib = (sy * w + sxb) * 4;

            let pr = prev[sir];
            let pg = prev[si + 1];
            let pb = prev[sib + 2];

            out[di] = lerp_u8(out[di], pr, mix_amt);
            out[di + 1] = lerp_u8(out[di + 1], pg, mix_amt);
            out[di + 2] = lerp_u8(out[di + 2], pb, mix_amt);
            out[di + 3] = 255;
        }
    }
}

fn sample_rgba(buf: &[u8], w: usize, h: usize, nx: f32, ny: f32) -> [u8; 4] {
    if buf.len() < w.saturating_mul(h).saturating_mul(4) {
        return [0, 0, 0, 255];
    }
    let x = ((nx * 0.5 + 0.5) * (w as f32 - 1.0)).round() as isize;
    let y = ((ny * 0.5 + 0.5) * (h as f32 - 1.0)).round() as isize;
    let xx = x.clamp(0, (w as isize) - 1) as usize;
    let yy = y.clamp(0, (h as isize) - 1) as usize;
    let i = (yy * w + xx) * 4;
    [buf[i], buf[i + 1], buf[i + 2], 255]
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 * (1.0 - t) + b as f32 * t) as u8
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let d = (edge1 - edge0).abs().max(1e-6);
    let t = ((x - edge0) / d).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn rotate2(x: f32, y: f32, a: f32) -> (f32, f32) {
    let s = a.sin();
    let c = a.cos();
    (c * x - s * y, s * x + c * y)
}

fn hash_u32(x: u32, y: u32, seed: u32) -> u32 {
    // Deterministic 2D hash (not crypto).
    let mut n = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ seed.wrapping_mul(0x9E37_79B9);
    n ^= n >> 13;
    n = n.wrapping_mul(1_274_126_177);
    n ^ (n >> 16)
}
