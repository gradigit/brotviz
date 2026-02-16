use crate::audio::AudioFeatures;
use crate::config::{Quality, SwitchMode};
use crate::visual::{
    classify_scene_section, is_fractal_preset_name, suggest_manual_transition,
    suggest_transition_for_section, transition_base_duration, transition_duration_for_kind,
    CameraPathMode, FractalZoomMode, RenderCtx, SceneSection, TransitionKind, TransitionMode,
    VisualEngine,
};
use anyhow::{anyhow, Context};
use metal::*;
use objc::rc::autoreleasepool;
use std::time::{Duration, Instant};

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    w: u32,
    h: u32,
    active_preset: u32,
    next_preset: u32,
    transition_kind: u32,

    time: f32,
    dt: f32,
    transition_alpha: f32,
    beat_pulse: f32,
    fractal_zoom_mul: f32,

    rms: f32,
    onset: f32,
    centroid: f32,
    flatness: f32,

    bands: [f32; 8],

    seed: u32,
    safe: u32,
    quality: u32,
    has_prev: u32,
    camera_path_mode: u32,
    camera_path_speed: f32,
}

pub struct MetalEngine {
    preset_names: Vec<&'static str>,
    playlist: Vec<usize>,
    active: usize,
    next: Option<usize>,
    shuffle: bool,
    switch_mode: SwitchMode,
    last_auto_mode: SwitchMode,
    beats_per_switch: u32,
    seconds_per_switch: f32,
    last_switch: Instant,
    beat_counter: u32,
    transition_started: Option<Instant>,
    transition_dur: Duration,
    transition_kind: TransitionKind,
    transition_seed: u32,
    transition_mode: TransitionMode,
    last_transition_kind: TransitionKind,
    transition_override: Option<TransitionKind>,
    fractal_zoom_mode: FractalZoomMode,
    fractal_zoom_drive: f32,
    fractal_zoom_enabled: bool,
    fractal_bias: bool,
    scene_section: SceneSection,
    scene_section_pending: SceneSection,
    scene_section_votes: u8,
    scene_section_changed_at: Instant,
    camera_path_mode: CameraPathMode,
    camera_path_speed: f32,

    device: Device,
    queue: CommandQueue,
    pipeline: ComputePipelineState,
    sampler: SamplerState,

    w: usize,
    h: usize,
    out_w: usize,
    out_h: usize,
    ping: bool,
    has_prev: bool,

    tex_a: Texture,
    tex_b: Texture,
    uniforms: Buffer,

    readback: Buffer,
    readback_bpr: usize,
    cpu_pixels: Vec<u8>,
    out_pixels: Vec<u8>,
}

impl MetalEngine {
    pub fn new(
        preset_names: Vec<&'static str>,
        active: usize,
        shuffle: bool,
        switch_mode: SwitchMode,
        beats_per_switch: u32,
        seconds_per_switch: f32,
    ) -> anyhow::Result<Self> {
        let device = Device::system_default().ok_or_else(|| anyhow!("no Metal device found"))?;
        let queue = device.new_command_queue();

        let options = CompileOptions::new();
        options.set_fast_math_enabled(true);
        let library = device
            .new_library_with_source(METAL_SRC, &options)
            .map_err(|e| anyhow!("Metal shader compile failed: {e}"))?;

        let func = library
            .get_function("visualize", None)
            .map_err(|e| anyhow!("Metal get_function(visualize) failed: {e}"))?;
        let pipeline = device
            .new_compute_pipeline_state_with_function(&func)
            .map_err(|e| anyhow!("Metal compute pipeline creation failed: {e}"))?;

        let sampler = {
            let desc = SamplerDescriptor::new();
            desc.set_min_filter(MTLSamplerMinMagFilter::Linear);
            desc.set_mag_filter(MTLSamplerMinMagFilter::Linear);
            desc.set_address_mode_s(MTLSamplerAddressMode::ClampToEdge);
            desc.set_address_mode_t(MTLSamplerAddressMode::ClampToEdge);
            device.new_sampler(&desc)
        };

        let uniforms = device.new_buffer(
            std::mem::size_of::<Uniforms>() as u64,
            MTLResourceOptions::StorageModeShared,
        );

        let (tex_a, tex_b, readback, readback_bpr, cpu_pixels) = make_resources(&device, 1, 1)?;

        let now = Instant::now();
        let active = active.min(preset_names.len().saturating_sub(1));
        let last_auto_mode = if switch_mode == SwitchMode::Manual {
            SwitchMode::Adaptive
        } else {
            switch_mode
        };

        let preset_count = preset_names.len();
        Ok(Self {
            preset_names,
            playlist: (0..preset_count).collect(),
            active,
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
            device,
            queue,
            pipeline,
            sampler,
            w: 1,
            h: 1,
            out_w: 0,
            out_h: 0,
            ping: false,
            has_prev: false,
            tex_a,
            tex_b,
            uniforms,
            readback,
            readback_bpr,
            cpu_pixels,
            out_pixels: Vec::new(),
        })
    }

    fn set_playlist_indices_internal(&mut self, indices: &[usize]) {
        if self.preset_names.is_empty() {
            self.playlist.clear();
            return;
        }

        let mut seen = vec![false; self.preset_names.len()];
        let mut playlist = Vec::with_capacity(indices.len().max(1));
        for &idx in indices {
            if idx < self.preset_names.len() && !seen[idx] {
                seen[idx] = true;
                playlist.push(idx);
            }
        }
        if playlist.is_empty() {
            playlist.extend(0..self.preset_names.len());
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

    fn playlist_pos_for_active(&self) -> usize {
        self.playlist
            .iter()
            .position(|&i| i == self.active)
            .unwrap_or(0)
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

    fn start_transition(&mut self, next: usize) {
        if next == self.active || self.preset_names.is_empty() {
            return;
        }
        // Manual transitions keep a consistent feel.
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

    fn start_transition_with_dur(&mut self, next: usize, dur: Duration, kind: TransitionKind) {
        if next == self.active || self.preset_names.is_empty() {
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

    fn next_preset_auto(&mut self, audio: &AudioFeatures) {
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
            if let Some(fr) = self.pick_fractal_index() {
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
        let min_votes = metal_section_hysteresis_votes(self.scene_section, candidate);
        let min_hold = metal_section_hysteresis_hold(self.scene_section, candidate);
        if self.scene_section_votes >= min_votes
            && now.duration_since(self.scene_section_changed_at) >= min_hold
        {
            self.scene_section = candidate;
            self.scene_section_pending = candidate;
            self.scene_section_votes = 0;
            self.scene_section_changed_at = now;
        }
    }

    fn pick_fractal_index(&mut self) -> Option<usize> {
        if self.playlist.len() <= 1 {
            return None;
        }
        let fractals = self
            .playlist
            .iter()
            .copied()
            .filter_map(|i| {
                if i != self.active && is_fractal_preset_name(self.preset_names[i]) {
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
            if idx != self.active && is_fractal_preset_name(self.preset_names[idx]) {
                return Some(idx);
            }
        }
        None
    }

    fn ensure_size(&mut self, w: usize, h: usize) -> anyhow::Result<()> {
        let w = w.max(1);
        let h = h.max(1);
        if w == self.w && h == self.h {
            return Ok(());
        }
        let (tex_a, tex_b, readback, readback_bpr, cpu_pixels) = make_resources(&self.device, w, h)
            .with_context(|| format!("allocate Metal render targets ({w}x{h})"))?;
        self.tex_a = tex_a;
        self.tex_b = tex_b;
        self.readback = readback;
        self.readback_bpr = readback_bpr;
        self.cpu_pixels = cpu_pixels;
        self.w = w;
        self.h = h;
        self.ping = false;
        self.has_prev = false;
        Ok(())
    }

    fn quality_u32(q: Quality) -> u32 {
        match q {
            Quality::Fast => 0,
            Quality::Balanced => 1,
            Quality::High => 2,
            Quality::Ultra => 3,
        }
    }
}

impl VisualEngine for MetalEngine {
    fn resize(&mut self, w: usize, h: usize) {
        // Resize is fallible for Metal (resource allocation); keep trait simple and make
        // failures surface during render.
        let _ = self.ensure_size(w, h);
    }

    fn preset_name(&self) -> &'static str {
        self.preset_names
            .get(self.active)
            .copied()
            .unwrap_or("<none>")
    }

    fn set_playlist_indices(&mut self, indices: &[usize]) {
        self.set_playlist_indices_internal(indices);
    }

    fn set_shuffle(&mut self, on: bool) {
        self.shuffle = on;
    }

    fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
    }

    fn cycle_transition_mode(&mut self) {
        self.transition_mode = self.transition_mode.next();
    }

    fn transition_mode(&self) -> TransitionMode {
        self.transition_mode
    }

    fn transition_kind_name(&self) -> &'static str {
        self.transition_kind.label()
    }

    fn transition_selection_name(&self) -> &'static str {
        if let Some(k) = self.transition_override {
            k.label()
        } else {
            "Auto"
        }
    }

    fn transition_selection_locked(&self) -> bool {
        self.transition_override.is_some()
    }

    fn scene_section_name(&self) -> &'static str {
        self.scene_section.label()
    }

    fn next_transition_kind(&mut self) {
        self.transition_override = match self.transition_override {
            None => Some(TransitionKind::all()[0]),
            Some(k) => {
                let n = k.next();
                if n == TransitionKind::all()[0] {
                    None
                } else {
                    Some(n)
                }
            }
        };
    }

    fn prev_transition_kind(&mut self) {
        self.transition_override = match self.transition_override {
            None => {
                let all = TransitionKind::all();
                Some(all[all.len() - 1])
            }
            Some(k) => {
                if k == TransitionKind::all()[0] {
                    None
                } else {
                    Some(k.prev())
                }
            }
        };
    }

    fn toggle_fractal_bias(&mut self) {
        self.fractal_bias = !self.fractal_bias;
    }

    fn fractal_bias(&self) -> bool {
        self.fractal_bias
    }

    fn cycle_camera_path_mode(&mut self) {
        self.camera_path_mode = self.camera_path_mode.next();
    }

    fn step_camera_path_mode(&mut self, forward: bool) {
        self.camera_path_mode = self.camera_path_mode.step(forward);
    }

    fn camera_path_mode(&self) -> CameraPathMode {
        self.camera_path_mode
    }

    fn camera_path_mode_name(&self) -> &'static str {
        self.camera_path_mode.label()
    }

    fn step_camera_path_speed(&mut self, delta: f32) {
        self.camera_path_speed = (self.camera_path_speed + delta).clamp(0.15, 4.0);
    }

    fn camera_path_speed(&self) -> f32 {
        self.camera_path_speed
    }

    fn cycle_fractal_zoom_mode(&mut self) {
        self.fractal_zoom_mode = self.fractal_zoom_mode.next();
    }

    fn fractal_zoom_mode(&self) -> FractalZoomMode {
        self.fractal_zoom_mode
    }

    fn set_fractal_zoom_drive(&mut self, v: f32) {
        self.fractal_zoom_drive = v.clamp(0.12, 8.0);
    }

    fn fractal_zoom_drive(&self) -> f32 {
        self.fractal_zoom_drive
    }

    fn toggle_fractal_zoom_enabled(&mut self) {
        self.fractal_zoom_enabled = !self.fractal_zoom_enabled;
    }

    fn fractal_zoom_enabled(&self) -> bool {
        self.fractal_zoom_enabled
    }

    fn toggle_auto_switch(&mut self) {
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

    fn set_switch_mode(&mut self, m: SwitchMode) {
        self.switch_mode = m;
        if m != SwitchMode::Manual {
            self.last_auto_mode = m;
        }
    }

    fn switch_mode(&self) -> SwitchMode {
        self.switch_mode
    }

    fn shuffle(&self) -> bool {
        self.shuffle
    }

    fn auto_switch(&self) -> bool {
        self.switch_mode != SwitchMode::Manual
    }

    fn prev_preset(&mut self) {
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

    fn next_preset(&mut self) {
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

    fn update_auto_switch(&mut self, now: Instant, audio: &AudioFeatures) {
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
                        metal_section_beats_per_switch(self.beats_per_switch, self.scene_section);
                    if self.beat_counter % beats_per == 0 {
                        self.next_preset_auto(audio);
                    }
                }
            }
            SwitchMode::Energy => {
                let e = audio.rms;
                let since = now.duration_since(self.last_switch).as_secs_f32();
                let (energy_gate, min_since) = metal_section_energy_gate(self.scene_section);
                if e > energy_gate && since > min_since {
                    self.next_preset_auto(audio);
                }
            }
            SwitchMode::Time => {
                let target =
                    (self.seconds_per_switch * metal_section_time_scale(self.scene_section))
                        .clamp(2.0, 60.0);
                if now.duration_since(self.last_switch).as_secs_f32() > target {
                    self.next_preset_auto(audio);
                }
            }
            SwitchMode::Adaptive => {
                let since = now.duration_since(self.last_switch).as_secs_f32();
                let treb = (audio.bands[5] + audio.bands[6] + audio.bands[7]) * (1.0 / 3.0);
                let hit = audio.onset.max(audio.beat_strength).max(treb);
                let e = audio.rms;

                let pace_scale = metal_section_time_scale(self.scene_section);
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
                let slam =
                    (audio.beat && audio.beat_strength > slam_gate) || audio.onset > (slam_gate - 0.04);
                if slam && since > min_since {
                    self.next_preset_auto(audio);
                } else if since > target {
                    self.next_preset_auto(audio);
                }
            }
        }
    }

    fn render(&mut self, ctx: RenderCtx, quality: Quality, scale: usize) -> &[u8] {
        let scale = scale.max(1);
        let out_w = ctx.w;
        let out_h = ctx.h;
        let iw = (out_w + scale - 1) / scale;
        let ih = (out_h + scale - 1) / scale;

        let _ = self.ensure_size(iw, ih);
        if self.w == 0 || self.h == 0 {
            return &self.cpu_pixels;
        }

        let alpha = if let (Some(start), Some(next)) = (self.transition_started, self.next) {
            let t = ctx.now.duration_since(start).as_secs_f32() / self.transition_dur.as_secs_f32();
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
        };

        let active = self.active as u32;
        let next = self.next.unwrap_or(self.active) as u32;

        let seed = if alpha == 0.0 {
            fastrand::u32(..)
        } else {
            self.transition_seed
        };
        let u = Uniforms {
            w: self.w as u32,
            h: self.h as u32,
            active_preset: active,
            next_preset: next,
            transition_kind: self.transition_kind as u32,
            time: ctx.t,
            dt: ctx.dt,
            transition_alpha: alpha,
            beat_pulse: ctx.beat_pulse.clamp(0.0, 1.0),
            fractal_zoom_mul: if self.fractal_zoom_enabled {
                self.fractal_zoom_mode.multiplier() * self.fractal_zoom_drive
            } else {
                -1.0
            },
            rms: ctx.audio.rms,
            onset: ctx.audio.onset,
            centroid: ctx.audio.centroid,
            flatness: ctx.audio.flatness,
            bands: ctx.audio.bands,
            seed,
            safe: if ctx.safe { 1 } else { 0 },
            quality: Self::quality_u32(quality),
            has_prev: if self.has_prev { 1 } else { 0 },
            camera_path_mode: self.camera_path_mode as u32,
            camera_path_speed: self.camera_path_speed,
        };

        unsafe {
            std::ptr::copy_nonoverlapping(
                (&u as *const Uniforms).cast::<u8>(),
                self.uniforms.contents().cast::<u8>(),
                std::mem::size_of::<Uniforms>(),
            );
        }

        let (prev, out) = if self.ping {
            (&self.tex_a, &self.tex_b)
        } else {
            (&self.tex_b, &self.tex_a)
        };

        autoreleasepool(|| {
            let cmd = self.queue.new_command_buffer();

            let encoder = cmd.new_compute_command_encoder();
            encoder.set_compute_pipeline_state(&self.pipeline);
            encoder.set_texture(0, Some(prev));
            encoder.set_texture(1, Some(out));
            encoder.set_sampler_state(0, Some(&self.sampler));
            encoder.set_buffer(0, Some(&self.uniforms), 0);

            let tpg = MTLSize::new(self.w as u64, self.h as u64, 1);
            let tptg = MTLSize::new(16, 16, 1);
            encoder.dispatch_threads(tpg, tptg);
            encoder.end_encoding();

            let blit = cmd.new_blit_command_encoder();
            blit.copy_from_texture_to_buffer(
                out,
                0,
                0,
                MTLOrigin { x: 0, y: 0, z: 0 },
                MTLSize::new(self.w as u64, self.h as u64, 1),
                &self.readback,
                0,
                self.readback_bpr as u64,
                (self.readback_bpr.saturating_mul(self.h)) as u64,
                MTLBlitOption::None,
            );
            blit.end_encoding();

            cmd.commit();
            cmd.wait_until_completed();
        });

        let row_bytes = self.w.saturating_mul(4);
        if row_bytes == 0 {
            return &self.cpu_pixels;
        }

        unsafe {
            let src = std::slice::from_raw_parts(
                self.readback.contents().cast::<u8>(),
                self.readback_bpr.saturating_mul(self.h),
            );
            for y in 0..self.h {
                let src_off = y * self.readback_bpr;
                let dst_off = y * row_bytes;
                let src_row = &src[src_off..src_off + row_bytes];
                let dst_row = &mut self.cpu_pixels[dst_off..dst_off + row_bytes];
                dst_row.copy_from_slice(src_row);
            }
        }

        self.has_prev = true;
        self.ping = !self.ping;

        if scale == 1 && self.w == out_w && self.h == out_h {
            return &self.cpu_pixels;
        }

        let out_row_bytes = out_w.saturating_mul(4);
        let out_len = out_row_bytes.saturating_mul(out_h);
        if out_len == 0 {
            return &self.out_pixels;
        }

        if self.out_w != out_w || self.out_h != out_h || self.out_pixels.len() != out_len {
            self.out_pixels.resize(out_len, 0);
            self.out_w = out_w;
            self.out_h = out_h;
        }

        let src_row_bytes = self.w.saturating_mul(4);
        if src_row_bytes == 0 {
            return &self.out_pixels;
        }

        for y in 0..out_h {
            let sy = (y / scale).min(self.h.saturating_sub(1));
            let src_row = &self.cpu_pixels[sy * src_row_bytes..sy * src_row_bytes + src_row_bytes];
            let dst_row = &mut self.out_pixels[y * out_row_bytes..y * out_row_bytes + out_row_bytes];
            for x in 0..out_w {
                let sx = (x / scale).min(self.w.saturating_sub(1));
                let si = sx * 4;
                let di = x * 4;
                dst_row[di..di + 4].copy_from_slice(&src_row[si..si + 4]);
            }
        }

        &self.out_pixels
    }
}

fn metal_section_hysteresis_votes(from: SceneSection, to: SceneSection) -> u8 {
    let from_rank = metal_section_intensity_rank(from);
    let to_rank = metal_section_intensity_rank(to);
    let delta = to_rank - from_rank;
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

fn metal_section_hysteresis_hold(from: SceneSection, to: SceneSection) -> Duration {
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

fn metal_section_time_scale(section: SceneSection) -> f32 {
    match section {
        SceneSection::Calm => 1.35,
        SceneSection::Groove => 1.0,
        SceneSection::Drive => 0.78,
        SceneSection::Impact => 0.58,
    }
}

fn metal_section_beats_per_switch(base: u32, section: SceneSection) -> u32 {
    match section {
        SceneSection::Calm => base.saturating_add(2).max(1),
        SceneSection::Groove => base.saturating_add(1).max(1),
        SceneSection::Drive => base.max(1),
        SceneSection::Impact => base.saturating_sub(1).max(1),
    }
}

fn metal_section_energy_gate(section: SceneSection) -> (f32, f32) {
    match section {
        SceneSection::Calm => (0.28, 12.0),
        SceneSection::Groove => (0.34, 8.8),
        SceneSection::Drive => (0.40, 6.2),
        SceneSection::Impact => (0.46, 4.6),
    }
}

const fn metal_section_intensity_rank(section: SceneSection) -> i8 {
    match section {
        SceneSection::Calm => 0,
        SceneSection::Groove => 1,
        SceneSection::Drive => 2,
        SceneSection::Impact => 3,
    }
}

fn make_resources(
    device: &Device,
    w: usize,
    h: usize,
) -> anyhow::Result<(Texture, Texture, Buffer, usize, Vec<u8>)> {
    let w = w.max(1);
    let h = h.max(1);

    let desc = TextureDescriptor::new();
    desc.set_texture_type(MTLTextureType::D2);
    desc.set_pixel_format(MTLPixelFormat::RGBA8Unorm);
    desc.set_width(w as u64);
    desc.set_height(h as u64);
    desc.set_storage_mode(MTLStorageMode::Private);
    desc.set_usage(MTLTextureUsage::ShaderRead | MTLTextureUsage::ShaderWrite);

    let tex_a = device.new_texture(&desc);
    let tex_b = device.new_texture(&desc);

    let align = (device.minimum_linear_texture_alignment_for_pixel_format(MTLPixelFormat::RGBA8Unorm) as usize).max(16);
    let row_bytes = w.saturating_mul(4);
    let readback_bpr = ((row_bytes + align - 1) / align) * align;
    let readback_len = readback_bpr.saturating_mul(h);
    let readback = device.new_buffer(readback_len as u64, MTLResourceOptions::StorageModeShared);

    let cpu_pixels = vec![0u8; row_bytes.saturating_mul(h)];
    Ok((tex_a, tex_b, readback, readback_bpr, cpu_pixels))
}

const METAL_SRC: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct Uniforms {
    uint w;
    uint h;
    uint active_preset;
    uint next_preset;
    uint transition_kind;

    float time;
    float dt;
    float transition_alpha;
    float beat_pulse;
    float fractal_zoom_mul;

    float rms;
    float onset;
    float centroid;
    float flatness;

    float bands[8];

    uint seed;
    uint safe;
    uint quality;
    uint has_prev;
    uint camera_path_mode;
    float camera_path_speed;
};

static inline float3 pal(float t, float3 a, float3 b, float3 c, float3 d) {
    return a + b * cos(6.2831853 * (c * t + d));
}

static inline float hash21(float2 p) {
    // Cheap hash -> 0..1
    float3 p3 = fract(float3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

static inline float2 rot(float2 p, float a) {
    float s = sin(a);
    float c = cos(a);
    return float2(c*p.x - s*p.y, s*p.x + c*p.y);
}

static inline float fbm(float2 p) {
    float f = 0.0;
    float a = 0.5;
    for (int i = 0; i < 5; i++) {
        f += a * (sin(p.x) * cos(p.y));
        p = rot(p * 1.7, 1.2);
        a *= 0.55;
    }
    return f;
}

static inline float noise2(float2 p) {
    float2 i = floor(p);
    float2 f = fract(p);
    float a = hash21(i + float2(0.0, 0.0));
    float b = hash21(i + float2(1.0, 0.0));
    float c = hash21(i + float2(0.0, 1.0));
    float d = hash21(i + float2(1.0, 1.0));
    float2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

static inline float fbm_noise(float2 p) {
    float f = 0.0;
    float a = 0.55;
    float2 q = p;
    for (int i = 0; i < 5; i++) {
        f += a * noise2(q);
        q = rot(q * 1.91 + float2(0.13, -0.21), 0.57);
        a *= 0.53;
    }
    return f;
}

static inline float2 curl_noise(float2 p) {
    const float e = 0.0016;
    float n1 = fbm_noise(p + float2(e, 0.0));
    float n2 = fbm_noise(p - float2(e, 0.0));
    float n3 = fbm_noise(p + float2(0.0, e));
    float n4 = fbm_noise(p - float2(0.0, e));
    float dx = (n1 - n2) / (2.0 * e);
    float dy = (n3 - n4) / (2.0 * e);
    return float2(dy, -dx);
}

static inline float sdGyroid(float3 p, float k) {
    return abs(dot(sin(p), cos(p.zxy)) / k) - 0.2;
}

static inline float map_mandelbulb(float3 p) {
    float3 z = p;
    float dr = 1.0;
    float r = 0.0;
    const int ITER = 8;
    const float POWER = 6.0;
    for (int i = 0; i < ITER; i++) {
        r = length(z);
        if (r > 2.0) break;
        float theta = acos(clamp(z.z / max(r, 1e-6), -1.0, 1.0));
        float phi = atan2(z.y, z.x);
        dr = pow(r, POWER - 1.0) * POWER * dr + 1.0;
        float zr = pow(r, POWER);
        theta *= POWER;
        phi *= POWER;
        z = zr * float3(sin(theta) * cos(phi), sin(phi) * sin(theta), cos(theta)) + p;
    }
    return 0.5 * log(max(r, 1e-6)) * r / max(dr, 1e-6);
}

static inline float deep_zoom_pow(float t, float speed, float zm, float beat, float base, float span) {
    if (zm <= 0.0) {
        return base;
    }
    float z = clamp(zm, 0.35, 2.5);
    float sweep = max(t, 0.0) * (0.12 + 0.22*max(speed, 0.0)) * z;
    sweep *= 1.0 + 0.55 * clamp(beat, 0.0, 1.0);
    float lz = log2(1.0 + sweep);
    return base + span * 0.12 * lz;
}

static inline float fractal_motion_zoom(float t, float zoom_mul, float bass, float beat) {
    if (zoom_mul <= 0.0) {
        return 1.0;
    }
    float zm = clamp(zoom_mul, 0.35, 2.5);
    float rate = max(0.01, 0.09 + 0.07*bass) * zm * (1.0 + 0.45*clamp(beat, 0.0, 1.0));
    float lz = log2(1.0 + max(t, 0.0) * rate);
    return 1.0 + (0.75 + 1.65*zm) * lz;
}

static inline bool is_fractal_preset(uint preset) {
    uint p = preset % 56u;
    return (p == 2u) || (p == 3u) || (p == 11u) || (p == 20u) || (p == 21u) ||
           (p >= 38u && p <= 45u) || (p == 49u) || (p == 50u) || (p == 54u) || (p == 55u);
}

static inline bool is_camera_travel_preset(uint preset) {
    uint p = preset % 56u;
    return (p == 2u) || (p == 3u) || (p == 11u) || (p == 20u) || (p == 21u) ||
           (p >= 38u && p <= 45u) || (p == 49u) || (p == 50u) || (p == 54u) || (p == 55u);
}

// 0=auto, 1=orbit, 2=dolly, 3=helix, 4=spiral, 5=drift
static inline uint camera_path_mode_for_preset(uint preset) {
    switch (preset % 56u) {
        case 2u:
        case 41u:
        case 49u:
            return 2u; // dolly
        case 3u:
        case 42u:
        case 43u:
            return 1u; // orbit
        case 11u:
        case 21u:
        case 45u:
            return 3u; // helix
        case 20u:
        case 44u:
        case 54u:
            return 4u; // spiral
        case 38u:
        case 39u:
        case 40u:
        case 50u:
        case 55u:
            return 5u; // drift
        default:
            return 0u; // auto
    }
}

struct CameraPathState {
    float2 drift;
    float zoom;
    float spin;
};

static inline CameraPathState make_camera_state(float2 drift, float zoom, float spin) {
    CameraPathState s;
    s.drift = drift;
    s.zoom = max(zoom, 1.0);
    s.spin = spin;
    return s;
}

static inline CameraPathState camera_auto_state(
    float t,
    float motion,
    float transient,
    float bass,
    float mid,
    float treb,
    float beat,
    float zoom_mul
) {
    float zm = clamp(zoom_mul, 0.35, 8.0);
    float rate = max(0.01, 0.08 + 0.11*motion) * zm * (1.0 + 0.26*transient);
    float lz = log2(1.0 + max(t, 0.0) * rate);
    float phase = t*(0.26 + 0.22*motion + 0.04*transient) + 1.4*bass + 0.9*mid + 0.7*treb;

    float w_orbit = smoothstep(0.15, 0.95, 0.55*mid + 0.30*treb + 0.20*motion);
    float w_dolly = smoothstep(0.14, 0.92, 0.62*bass + 0.38*transient);
    float w_helix = smoothstep(0.18, 0.96, 0.50*mid + 0.28*transient + 0.22*bass);
    float w_spiral = smoothstep(0.20, 0.96, 0.58*treb + 0.25*motion + 0.17*beat);
    float w_drift = smoothstep(0.10, 0.90, 0.58*(1.0 - transient) + 0.25*(1.0 - beat) + 0.17*motion);
    float w_sum = max(w_orbit + w_dolly + w_helix + w_spiral + w_drift, 1e-4);
    w_orbit /= w_sum;
    w_dolly /= w_sum;
    w_helix /= w_sum;
    w_spiral /= w_sum;
    w_drift /= w_sum;

    float2 orbit = (0.020 + 0.060*(0.35 + 0.65*lz)) * float2(
        sin(phase),
        cos(phase*1.07 + 0.5*mid)
    );
    float2 helix = (0.018 + 0.050*(0.30 + 0.70*lz)) * float2(
        sin(phase*1.23 + 1.3*treb),
        cos(phase*1.11 - 1.0*bass)
    );
    float2 spiral = (0.014 + 0.044*(0.40 + 0.60*lz)) * float2(
        cos(phase*1.37),
        sin(phase*1.37)
    );
    float2 drift = (0.010 + 0.028*(0.45 + 0.55*lz)) * float2(
        sin(t*(0.39 + 0.08*mid) + 2.4*treb + 1.7*transient) + 0.45*sin(t*0.91 + 2.1*bass),
        cos(t*(0.33 + 0.09*bass) + 1.8*mid - 1.9*transient) + 0.40*cos(t*0.79 - 2.3*treb)
    );

    float2 d = w_orbit*orbit + w_helix*helix + w_spiral*(spiral + 0.35*orbit) + w_drift*drift;
    float zoom = 1.0 + (0.30 + 1.10*w_dolly + 0.50*w_helix + 0.28*w_spiral) * (0.65 + 1.05*zm) * lz;
    zoom = clamp(zoom, 1.0, 52.0);
    float spin = 0.08*w_orbit*sin(phase*0.63) +
                 0.12*w_helix*sin(phase*0.74) +
                 0.15*w_spiral*sin(phase*0.58 + 1.2*bass) +
                 0.05*w_drift*sin(t*0.31 + 2.0*treb);
    spin = clamp(spin, -0.55, 0.55);
    return make_camera_state(d, zoom, spin);
}

static inline CameraPathState camera_state_for_mode(
    uint mode,
    float t,
    float motion,
    float transient,
    float bass,
    float mid,
    float treb,
    float beat,
    float zoom_mul
) {
    float zm = clamp(zoom_mul, 0.35, 8.0);
    float rate = max(0.01, 0.08 + 0.11*motion) * zm * (1.0 + 0.26*transient);
    float lz = log2(1.0 + max(t, 0.0) * rate);
    float phase = t*(0.22 + 0.16*motion + 0.05*transient) + 1.6*bass + 0.8*mid + 0.5*treb;

    switch (mode) {
        case 1u: { // orbit
            float2 drift = (0.020 + 0.060*(0.35 + 0.65*lz)) * float2(
                sin(phase),
                cos(phase*1.07 + 0.5*mid)
            );
            float zoom = clamp(1.0 + (0.46 + 0.78*zm) * lz * (1.0 + 0.10*mid), 1.0, 46.0);
            float spin = 0.11 * sin(phase*0.72 + 0.4*treb);
            return make_camera_state(drift, zoom, spin);
        }
        case 2u: { // dolly
            float2 drift = (0.010 + 0.028*(0.4 + 0.6*lz)) * float2(
                sin(t*(0.31 + 0.08*mid) + 0.7*bass),
                cos(t*(0.27 + 0.08*bass) + 0.6*treb)
            );
            float zoom = clamp(1.0 + (1.12 + 1.46*zm) * lz * (1.0 + 0.10*bass), 1.0, 56.0);
            float spin = 0.04 * sin(phase*0.41 + 0.8*mid);
            return make_camera_state(drift, zoom, spin);
        }
        case 3u: { // helix
            float helix_r = (0.016 + 0.055*(0.30 + 0.70*lz)) * (1.0 + 0.40*motion);
            float2 drift = helix_r * float2(
                sin(phase*1.22 + 1.5*treb),
                cos(phase*1.04 - 1.2*bass)
            );
            float zoom = clamp(1.0 + (0.82 + 1.10*zm) * lz * (1.0 + 0.08*mid), 1.0, 50.0);
            float spin = clamp(0.12 * sin(phase*0.76) + 0.08*lz, -0.60, 0.60);
            return make_camera_state(drift, zoom, spin);
        }
        case 4u: { // spiral
            float spiral_r = (0.018 + 0.062*motion) * (0.36 + 0.64*lz);
            float2 drift = spiral_r * float2(
                cos(phase*1.36 + 0.8*bass),
                sin(phase*1.36 + 0.8*bass)
            );
            float zoom = clamp(1.0 + (0.70 + 1.04*zm) * lz * (1.0 + 0.11*treb), 1.0, 50.0);
            float spin = clamp(0.18 * sin(phase*0.58 + 0.8*treb), -0.70, 0.70);
            return make_camera_state(drift, zoom, spin);
        }
        case 5u: { // drift
            float2 drift = (0.014 + 0.040*(0.38 + 0.62*lz)) * float2(
                sin(t*(0.43 + 0.06*motion) + 2.0*treb + 1.3*transient) + 0.45*sin(t*0.93 + 2.4*bass),
                cos(t*(0.37 + 0.08*motion) + 1.6*mid - 1.2*transient) + 0.40*cos(t*0.86 - 2.1*treb)
            );
            float zoom = clamp(1.0 + (0.54 + 0.78*zm) * lz * (1.0 + 0.08*motion), 1.0, 44.0);
            float spin = clamp(0.06*sin(t*(0.28 + 0.16*motion)) + 0.03*cos(t*0.41 + 2.7*treb), -0.50, 0.50);
            return make_camera_state(drift, zoom, spin);
        }
        default:
            return camera_auto_state(t, motion, transient, bass, mid, treb, beat, zoom_mul);
    }
}

static inline float2 apply_camera_path(
    float2 q,
    float t,
    float motion,
    float transient,
    float bass,
    float mid,
    float treb,
    float beat,
    float zoom_mul,
    uint mode,
    float path_mix
) {
    float m = clamp(path_mix, 0.0, 1.0);
    if (m <= 0.0001) {
        return q;
    }
    CameraPathState state = camera_state_for_mode(mode, t, motion, transient, bass, mid, treb, beat, zoom_mul);
    float2 qq = q + state.drift * m;
    qq = rot(qq, state.spin * m);
    float z = mix(1.0, state.zoom, m);
    return qq / max(z, 1e-4);
}

static inline float sphere_trace_scene(float3 ro, float3 rd, float t, float bass, float mid, float treb, uint mode) {
    float dist = 0.0;
    for (int i = 0; i < 72; i++) {
        float3 p = ro + rd * dist;
        float d;
        if (mode == 0u) {
            float3 q = p;
            q.xy = rot(q.xy, 0.13*t);
            d = map_mandelbulb(q * (1.15 + 0.6*bass));
        } else {
            float3 q = p * (2.4 + 2.2*bass);
            q.xy = rot(q.xy, 0.19*t);
            q.yz = rot(q.yz, 0.16*t);
            d = sdGyroid(q, 1.2 + 0.8*treb);
            d += 0.12 * sin(q.x*2.0 + t*(1.0 + 1.8*mid));
        }
        if (d < 0.0015) {
            return exp(-0.09 * dist) * (1.0 + 0.65 * (72.0 - (float)i) / 72.0);
        }
        dist += clamp(d, 0.004, 0.22);
        if (dist > 8.0) break;
    }
    return exp(-0.22 * dist);
}

static inline float3 preset_color(uint preset, float2 p, float t, float bass, float mid, float treb, float beat, uint quality, float aspect, float zoom_mul) {
    // p: aspect-corrected (-aspect..aspect, -1..1)
    float r = length(p);
    float a = atan2(p.y, p.x);
    float q = (float)quality;
    float2 pn = float2(p.x / max(aspect, 1e-5), p.y);
    float2 uv = pn * 0.5 + 0.5;

    switch (preset % 56u) {
        default:
        case 0u: {
            float v = sin(p.x*3.2 + t*1.3) + sin(p.y*4.1 - t*1.1) + sin((p.x+p.y)*2.7 + t*0.8);
            v += 0.65 * sin(r*10.0 - t*2.0 + bass*2.5);
            v = 0.5 + 0.5 * sin(v + beat*2.0);
            return pal(v + 0.2*bass, float3(0.35,0.10,0.15), float3(0.65,0.75,0.85), float3(1.0,1.0,1.0), float3(0.0,0.33,0.67));
        }
        case 1u: {
            // Tunnel
            float u = 1.0 / max(r, 1e-3);
            float v = a / 6.2831853 + 0.5;
            float w = u*0.35 + t*0.25 + beat*0.2;
            float n = fbm(float2(v*8.0 + t, w*8.0 - t));
            float s = fract(w + n*0.12);
            return pal(s + 0.15*treb, float3(0.30,0.30,0.25), float3(0.65,0.55,0.75), float3(1.0,1.0,1.0), float3(0.2,0.3,0.4));
        }
        case 2u: {
            // Mandelbrot-ish
            float zf = fractal_motion_zoom(t, zoom_mul, bass, beat);
            float2 c = float2((p.x*0.9)/zf - 0.25 + 0.08*sin(t*0.3 + bass*2.0), (p.y*0.9)/zf + 0.05*cos(t*0.2 + mid*2.0));
            float2 z = float2(0.0);
            int iters = 28 + int(q)*18;
            int i = 0;
            for (; i < iters; i++) {
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                if (dot(z,z) > 4.0) break;
            }
            float m = (float)i / (float)iters;
            float glow = exp(-3.5 * m);
            float3 col = pal(m + 0.08*t + 0.2*bass, float3(0.15,0.10,0.25), float3(0.75,0.70,0.95), float3(1.0,1.0,1.0), float3(0.0,0.25,0.5));
            return col + glow*0.35;
        }
        case 3u: {
            // Julia
            float zf = fractal_motion_zoom(t, zoom_mul, bass, beat);
            float2 z = float2((p.x*1.15)/zf, (p.y*1.15)/zf);
            float2 c = float2(0.32 + 0.12*sin(t*0.23 + mid*2.0), 0.51 + 0.12*cos(t*0.19 + treb*2.0));
            int iters = 26 + int(q)*16;
            int i = 0;
            for (; i < iters; i++) {
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                if (dot(z,z) > 6.0) break;
            }
            float m = (float)i / (float)iters;
            return pal(m + 0.12*t + beat*0.18, float3(0.10,0.15,0.20), float3(0.85,0.70,0.90), float3(1.0,1.0,1.0), float3(0.1,0.4,0.7));
        }
        case 4u: {
            // Kaleidoscope
            float2 k = p;
            k = abs(k);
            k = rot(k, 0.3 + 0.9*bass + 0.2*sin(t*0.5));
            float v = sin(k.x*10.0 + t*1.2) * cos(k.y*10.0 - t*1.1);
            v += sin((k.x+k.y)*8.0 + t*0.9 + beat*1.5);
            v = 0.5 + 0.5*sin(v*2.0);
            return pal(v + 0.2*mid, float3(0.25,0.10,0.30), float3(0.85,0.75,0.95), float3(1.0,1.0,1.0), float3(0.0,0.2,0.4));
        }
        case 5u: {
            // Neon rings
            float rr = r + 0.05*sin(a*6.0 + t*2.0);
            float v = sin(rr*18.0 - t*4.0 + bass*4.0);
            v = smoothstep(-0.2, 0.8, v);
            float3 col = pal(rr + 0.1*t + 0.15*treb, float3(0.20,0.10,0.10), float3(0.85,0.85,0.95), float3(1.0,1.0,1.0), float3(0.6,0.2,0.0));
            return col * (0.25 + 0.95*v);
        }
        case 6u: {
            // Electric grid
            float2 g = p * (4.0 + 6.0*treb);
            float v = abs(sin(g.x + t)) + abs(sin(g.y - t*1.1));
            float e = exp(-2.0 * abs(v - 1.0));
            float3 col = pal(v + 0.2*bass + 0.12*t, float3(0.15,0.15,0.20), float3(0.75,0.85,0.95), float3(1.0,1.0,1.0), float3(0.0,0.1,0.2));
            return col * (0.25 + 1.2*e);
        }
        case 7u: {
            // Spirals
            float v = sin(8.0*a + 6.0*log(max(r,1e-3)) + t*2.0 + bass*2.5);
            v = 0.5 + 0.5*v;
            return pal(v + 0.25*beat, float3(0.25,0.05,0.15), float3(0.85,0.80,0.95), float3(1.0,1.0,1.0), float3(0.15,0.35,0.55));
        }
        case 8u: {
            // Starburst
            float rays = 12.0 + 18.0*bass;
            float v = cos(a*rays + t*1.5) * (1.0 - smoothstep(0.0, 1.2, r));
            v = 0.5 + 0.5*v;
            float3 col = pal(v + 0.1*t, float3(0.18,0.12,0.08), float3(0.95,0.85,0.70), float3(1.0,1.0,1.0), float3(0.0,0.3,0.7));
            return col * (0.25 + 1.1*v);
        }
        case 9u: {
            // Liquid marble
            float2 m = p + 0.25*float2(sin(t + p.y*3.0), cos(t*1.2 + p.x*3.0));
            float v = fbm(m*3.2 + float2(t*0.3, -t*0.2));
            v = 0.5 + 0.5*sin(v*4.0 + beat*1.2);
            return pal(v + 0.1*bass, float3(0.12,0.10,0.18), float3(0.88,0.80,0.95), float3(1.0,1.0,1.0), float3(0.4,0.25,0.1));
        }
        case 10u: {
            // Chromatic waves
            float v = sin(p.x*6.0 + t*1.8) + sin(p.y*5.0 - t*1.6);
            v += sin((p.x-p.y)*4.0 + t*1.1 + treb*2.0);
            v = 0.5 + 0.5*sin(v + 0.8*bass);
            return pal(v + 0.12*t, float3(0.15,0.10,0.08), float3(0.85,0.95,0.90), float3(1.0,1.0,1.0), float3(0.0,0.33,0.67));
        }
        case 11u: {
            // Orbit trap-ish
            float zf = fractal_motion_zoom(t, zoom_mul, bass, beat);
            float2 z = (p*0.9)/zf;
            float2 c = float2(-0.3 + 0.2*sin(t*0.2), 0.55 + 0.2*cos(t*0.17));
            float d = 10.0;
            int iters = 22 + int(q)*12;
            for (int i=0; i<iters; i++) {
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                d = min(d, length(z - float2(0.2, 0.0)));
            }
            float v = exp(-6.0*d);
            return pal(v + 0.2*mid + 0.05*t, float3(0.10,0.10,0.15), float3(0.80,0.90,0.95), float3(1.0,1.0,1.0), float3(0.2,0.1,0.0)) * (0.2 + 1.4*v);
        }
        case 12u: {
            // Kaleido tunnel
            float2 k = rot(p, 0.4*sin(t*0.4) + bass);
            k = abs(k);
            float u = 1.0 / max(length(k), 1e-3);
            float v = atan2(k.y, k.x) / 6.2831853 + 0.5;
            float s = fract(u*0.4 + v*2.0 + t*0.35 + beat*0.15);
            return pal(s + 0.15*treb, float3(0.20,0.15,0.10), float3(0.85,0.80,0.95), float3(1.0,1.0,1.0), float3(0.1,0.2,0.3));
        }
        case 13u: {
            // Hyper checker
            float2 g = floor((p + 1.5) * (8.0 + 10.0*bass));
            float v = fmod(g.x + g.y, 2.0);
            v = mix(v, 1.0 - v, 0.5 + 0.5*sin(t + beat*1.5));
            return pal(v + 0.25*mid, float3(0.10,0.10,0.10), float3(0.95,0.85,0.75), float3(1.0,1.0,1.0), float3(0.0,0.45,0.9));
        }
        case 14u: {
            // Fireflies
            float v = 0.0;
            for (int i=0; i<7; i++) {
                float2 o = float2(hash21(float2((float)i, 1.0)), hash21(float2((float)i, 2.0)));
                o = (o*2.0 - 1.0) * float2(1.4, 0.9);
                float2 q = p - o - 0.25*float2(sin(t*0.7 + (float)i), cos(t*0.6 - (float)i));
                v += exp(-18.0*dot(q,q)) * (0.5 + 0.7*bass);
            }
            v = clamp(v, 0.0, 1.0);
            return pal(v + 0.12*t, float3(0.05,0.08,0.10), float3(0.95,0.90,0.85), float3(1.0,1.0,1.0), float3(0.2,0.55,0.85)) * (0.15 + 1.5*v);
        }
        case 15u: {
            // Vortex
            float2 qv = rot(p, t*0.35 + 1.2*bass);
            float v = sin(12.0*qv.x + 6.0*qv.y + t*2.2) + cos(10.0*qv.y - t*1.7);
            v = 0.5 + 0.5*sin(v + beat*1.8);
            return pal(v + 0.18*treb, float3(0.18,0.10,0.25), float3(0.82,0.85,0.95), float3(1.0,1.0,1.0), float3(0.05,0.25,0.55));
        }
        case 16u: {
            // Acid noise bands
            float v = fbm(p*6.0 + float2(t*0.9, -t*0.7));
            v = 0.5 + 0.5*sin(v*6.0 + 2.0*bass + beat*1.5);
            float3 col = pal(v + 0.15*mid, float3(0.12,0.10,0.08), float3(0.95,0.90,0.85), float3(1.0,1.0,1.0), float3(0.0,0.33,0.67));
            return col * (0.25 + 1.0*v);
        }
        case 17u: {
            // Rainbow petals
            float k = abs(sin(a*6.0 + t*0.8 + bass*2.0));
            float v = exp(-3.0*abs(r - 0.55 - 0.15*k));
            return pal(k + 0.1*t, float3(0.20,0.10,0.10), float3(0.85,0.95,0.95), float3(1.0,1.0,1.0), float3(0.0,0.2,0.4)) * (0.2 + 1.4*v);
        }
        case 18u: {
            // Moire
            float2 m = rot(p, 0.25*sin(t*0.2) + 0.4*bass);
            float v = sin(m.x*18.0 + t*0.6) + sin(m.y*17.0 - t*0.7);
            v += sin((m.x+m.y)*9.0 + t*0.8 + beat*1.2);
            v = 0.5 + 0.5*sin(v);
            return pal(v + 0.12*treb, float3(0.10,0.10,0.15), float3(0.90,0.80,0.95), float3(1.0,1.0,1.0), float3(0.15,0.35,0.55));
        }
        case 19u: {
            // Minimal fractal sparks
            float2 z = p * (1.2 + 0.8*bass);
            float2 c = float2(-0.7 + 0.15*sin(t*0.21), 0.25 + 0.15*cos(t*0.17));
            int iters = 18 + int(q)*10;
            float glow = 0.0;
            for (int i=0; i<iters; i++) {
                z = float2(abs(z.x), abs(z.y));
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                glow += exp(-4.0*dot(z,z));
            }
            glow = clamp(glow * 0.12, 0.0, 1.0);
            return pal(glow + 0.05*t + 0.2*mid, float3(0.05,0.10,0.12), float3(0.95,0.90,0.85), float3(1.0,1.0,1.0), float3(0.1,0.3,0.6)) * (0.2 + 1.4*glow);
        }

        case 20u: {
            // Burning Ship
            float zf = fractal_motion_zoom(t, zoom_mul, bass, beat);
            float2 c = float2((p.x*0.92)/zf - 0.35 + 0.08*sin(t*0.22 + bass*2.0), (p.y*0.92)/zf + 0.06*cos(t*0.19 + mid*2.0));
            float2 z = float2(0.0);
            int iters = 26 + int(q)*18;
            int i = 0;
            for (; i < iters; i++) {
                z = float2(abs(z.x), abs(z.y));
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                if (dot(z,z) > 8.0) break;
            }
            float m = (float)i / (float)iters;
            float3 col = pal(m + 0.08*t + 0.25*bass, float3(0.15,0.08,0.05), float3(0.95,0.80,0.65), float3(1.0,1.0,1.0), float3(0.0,0.18,0.55));
            return col * (0.2 + 1.3*(1.0-m));
        }
        case 21u: {
            // Orbit trap bloom
            float zf = fractal_motion_zoom(t, zoom_mul, bass, beat);
            float2 z = (p*0.95)/zf;
            float2 c = float2(-0.38 + 0.18*sin(t*0.17), 0.58 + 0.18*cos(t*0.13));
            float d = 9.0;
            int iters = 26 + int(q)*16;
            for (int i=0; i<iters; i++) {
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                d = min(d, abs(length(z - float2(0.15, 0.05)) - (0.22 + 0.12*bass)));
            }
            float v = exp(-10.0*d);
            return pal(v + 0.12*t + 0.15*mid, float3(0.05,0.10,0.18), float3(0.90,0.85,1.0), float3(1.0,1.0,1.0), float3(0.15,0.35,0.65)) * (0.18 + 1.5*v);
        }
        case 22u: {
            // Clifford attractor field (cheap)
            float a0 = 1.6 + 0.8*bass;
            float b0 = 1.7 + 0.9*mid;
            float c0 = 0.6 + 0.5*treb;
            float d0 = 1.2 + 0.4*bass;
            float2 z = p*0.6;
            float acc = 0.0;
            for (int i=0; i<10 + int(q)*3; i++) {
                float x = sin(a0*z.y) + c0*cos(a0*z.x);
                float y = sin(b0*z.x) + d0*cos(b0*z.y);
                z = float2(x, y);
                acc += exp(-2.2*dot(z,z));
            }
            float v = clamp(acc * 0.18, 0.0, 1.0);
            return pal(v + 0.06*t + 0.2*beat, float3(0.10,0.05,0.15), float3(0.95,0.90,0.85), float3(1.0,1.0,1.0), float3(0.0,0.25,0.5)) * (0.2 + 1.3*v);
        }
        case 23u: {
            // de Jong attractor-ish field
            float a0 = 1.4 + 1.0*bass;
            float b0 = 1.8 + 1.0*treb;
            float c0 = 1.6 + 0.7*mid;
            float d0 = 1.9 + 0.6*bass;
            float2 z = p*0.7;
            float acc = 0.0;
            for (int i=0; i<10 + int(q)*3; i++) {
                float x = sin(a0*z.y) - cos(b0*z.x);
                float y = sin(c0*z.x) - cos(d0*z.y);
                z = float2(x, y);
                acc += exp(-2.4*dot(z,z));
            }
            float v = clamp(acc * 0.20, 0.0, 1.0);
            return pal(v + 0.05*t + 0.12*treb, float3(0.08,0.10,0.12), float3(0.95,0.90,0.85), float3(1.0,1.0,1.0), float3(0.2,0.1,0.0)) * (0.2 + 1.4*v);
        }
        case 24u: {
            // Domain-warp candy
            float2 w = p;
            float f = fbm(w*2.5 + float2(t*0.4, -t*0.3));
            w += 0.35*float2(sin(f*6.0 + t*1.3), cos(f*5.0 - t*1.1));
            float v = sin(w.x*(3.5+6.0*bass) + t*1.1) * cos(w.y*(3.0+5.0*treb) - t*1.0);
            v = 0.5 + 0.5*sin(v*2.2 + beat*1.3);
            return pal(v + 0.1*t + 0.2*mid, float3(0.12,0.10,0.08), float3(0.95,0.90,0.85), float3(1.0,1.0,1.0), float3(0.0,0.33,0.67));
        }
        case 25u: {
            // Polar moire
            float rr = r;
            float aa = a;
            float v = sin(rr*(20.0+30.0*bass) + t*(1.0+beat*2.0)) + cos(aa*(10.0+18.0*treb) - t*1.4);
            v += sin((rr+aa)*12.0 + t*0.8);
            v = 0.5 + 0.5*sin(v);
            return pal(v + 0.12*t, float3(0.10,0.10,0.16), float3(0.90,0.85,0.98), float3(1.0,1.0,1.0), float3(0.12,0.35,0.65));
        }
        case 26u: {
            // Truchet-ish tiles
            float tiles = 6.0 + 18.0*bass;
            float2 g = uv * tiles;
            float2 id = floor(g);
            float2 f = fract(g) - 0.5;
            float h = hash21(id + float2((float)(quality+1), (float)(quality+7)));
            bool flip = h > 0.5;
            float2 c1 = flip ? float2(-0.5,-0.5) : float2(-0.5,0.5);
            float2 c2 = flip ? float2(0.5,0.5) : float2(0.5,-0.5);
            float d1 = abs(length(f - c1) - 0.5);
            float d2 = abs(length(f - c2) - 0.5);
            float d = min(d1,d2);
            float v = exp(-20.0*d);
            v = clamp(v*(0.8+0.6*treb), 0.0, 1.0);
            return pal(v + 0.08*t + 0.2*beat, float3(0.05,0.08,0.10), float3(0.95,0.90,0.85), float3(1.0,1.0,1.0), float3(0.0,0.2,0.4)) * (0.2 + 1.4*v);
        }
        case 27u: {
            // SDF-ish orbs
            float2 q2 = p;
            q2 = rot(q2, 0.2*sin(t*0.3) + bass);
            float2 rep = fract(q2*2.4) - 0.5;
            float d = length(rep) - (0.15 + 0.12*bass);
            float v = exp(-14.0*abs(d));
            return pal(v + 0.08*t + 0.1*mid, float3(0.10,0.06,0.12), float3(0.95,0.90,0.85), float3(1.0,1.0,1.0), float3(0.25,0.55,0.85)) * (0.18 + 1.5*v);
        }
        case 28u: {
            // Chladni plates
            float ax = 2.0 + 10.0*bass;
            float ay = 2.0 + 10.0*mid;
            float v = sin(ax*p.x*3.14159) * sin(ay*p.y*3.14159);
            v += 0.35*sin((ax+ay)*0.5*(p.x+p.y)*3.14159 + t);
            v = pow(abs(v), 0.35);
            return pal(v + 0.05*t + 0.15*treb, float3(0.10,0.10,0.14), float3(0.90,0.85,0.98), float3(1.0,1.0,1.0), float3(0.0,0.25,0.5)) * (0.2 + 1.2*v);
        }
        case 29u: {
            // Scanline CRT-ish
            float v = fbm(p*5.5 + float2(t*0.7, -t*0.5));
            float scan = 0.7 + 0.3*sin(uv.y*300.0 + t*25.0 + 6.0*beat);
            v = (0.5 + 0.5*sin(v*7.0 + 2.5*bass + beat*1.3)) * scan;
            return pal(v + 0.12*t, float3(0.12,0.10,0.08), float3(0.95,0.90,0.85), float3(1.0,1.0,1.0), float3(0.0,0.33,0.67));
        }
        case 30u: {
            // Kaleido mandala
            float2 k = abs(rot(p, 0.4*sin(t*0.35) + 0.7*bass));
            float rays = 10.0 + 16.0*bass;
            float v = cos(atan2(k.y,k.x)*rays + t*1.1) * (1.0 - smoothstep(0.0, 1.3, length(k)));
            v = 0.5 + 0.5*v;
            return pal(v + 0.08*t + 0.2*mid, float3(0.15,0.10,0.20), float3(0.85,0.75,0.95), float3(1.0,1.0,1.0), float3(0.1,0.4,0.7)) * (0.2 + 1.3*v);
        }
        case 31u: {
            // Hyper stripes v2
            float2 s2 = rot(p, 0.2*sin(t*0.25) + 0.5*treb);
            float v = sin(s2.x*(18.0+30.0*bass) + t*(2.2+4.0*beat)) + cos(s2.y*(10.0+22.0*treb) - t*1.7);
            v = 0.5 + 0.5*sin(v);
            return pal(v + 0.1*t, float3(0.12,0.10,0.08), float3(0.95,0.90,0.85), float3(1.0,1.0,1.0), float3(0.0,0.33,0.67));
        }
        case 32u: {
            // Fireflies v2 (denser)
            float v = 0.0;
            for (int i=0; i<10; i++) {
                float2 o = float2(hash21(float2((float)i, 9.0)), hash21(float2((float)i, 10.0)));
                o = (o*2.0 - 1.0) * float2(1.5, 1.0);
                float2 qf = p - o - 0.22*float2(sin(t*0.9 + (float)i), cos(t*0.75 - (float)i));
                v += exp(-16.0*dot(qf,qf)) * (0.4 + 0.9*bass);
            }
            v = clamp(v, 0.0, 1.0);
            return pal(v + 0.1*t, float3(0.05,0.08,0.10), float3(0.95,0.90,0.85), float3(1.0,1.0,1.0), float3(0.2,0.55,0.85)) * (0.15 + 1.6*v);
        }
        case 33u: {
            // Starburst v2
            float rays = 18.0 + 24.0*bass + 10.0*treb;
            float v = cos(a*rays + t*(1.8+3.0*beat)) * (1.0 - smoothstep(0.0, 1.25, r));
            v = 0.5 + 0.5*v;
            return pal(v + 0.1*t + 0.15*treb, float3(0.15,0.10,0.08), float3(0.95,0.85,0.70), float3(1.0,1.0,1.0), float3(0.0,0.3,0.7)) * (0.2 + 1.2*v);
        }
        case 34u: {
            // Liquid marble v2
            float2 m = p + 0.35*float2(sin(t*1.1 + p.y*3.0), cos(t*1.3 + p.x*3.0));
            float v = fbm(m*3.8 + float2(t*0.35, -t*0.25));
            v = 0.5 + 0.5*sin(v*5.0 + beat*1.4 + 1.5*bass);
            return pal(v + 0.12*t, float3(0.12,0.10,0.18), float3(0.88,0.80,0.95), float3(1.0,1.0,1.0), float3(0.4,0.25,0.1));
        }
        case 35u: {
            // Vortex v2
            float2 qv = rot(p, t*0.55 + 1.6*bass);
            float v = sin(14.0*qv.x + 7.0*qv.y + t*2.6) + cos(11.0*qv.y - t*2.0);
            v = 0.5 + 0.5*sin(v + beat*2.0);
            return pal(v + 0.15*treb, float3(0.18,0.10,0.25), float3(0.82,0.85,0.95), float3(1.0,1.0,1.0), float3(0.05,0.25,0.55));
        }
        case 36u: {
            // Noise ribbons
            float2 qn = p + 0.25*float2(sin(t*0.8 + p.y*2.5), cos(t*0.7 + p.x*2.5));
            float v = fbm(qn*(6.0 + 6.0*treb) + float2(t*0.9, -t*0.8));
            v = 0.5 + 0.5*sin(v*8.0 + 2.0*bass + beat*1.3);
            return pal(v + 0.05*t + 0.2*mid, float3(0.10,0.10,0.14), float3(0.90,0.85,0.98), float3(1.0,1.0,1.0), float3(0.1,0.4,0.7)) * (0.25 + 1.0*v);
        }
        case 37u: {
            // Glitch blocks
            float blocks = 18.0 + 42.0*treb;
            float2 id = floor(uv*blocks);
            float n = hash21(id + float2((float)(preset+1), (float)(preset+7)));
            float v = n;
            v = mix(v, 1.0 - v, 0.5 + 0.5*sin(t*6.0 + beat*2.0));
            return pal(v + 0.12*t + 0.25*mid, float3(0.08,0.08,0.10), float3(0.95,0.90,0.85), float3(1.0,1.0,1.0), float3(0.0,0.45,0.9));
        }
        case 38u: {
            // Deep mandelbrot zoom-ish
            float zf = fractal_motion_zoom(t, zoom_mul, bass, beat);
            float2 c = float2((p.x*0.75)/zf - 0.5 + 0.05*sin(t*0.2 + bass*2.0), (p.y*0.75)/zf + 0.02*cos(t*0.17 + mid*2.0));
            float2 z = float2(0.0);
            int iters = 34 + int(q)*22;
            int i = 0;
            for (; i < iters; i++) {
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                if (dot(z,z) > 4.0) break;
            }
            float m = (float)i / (float)iters;
            float glow = exp(-4.0*m);
            float3 col = pal(m + 0.07*t + 0.25*bass, float3(0.10,0.08,0.18), float3(0.90,0.85,0.98), float3(1.0,1.0,1.0), float3(0.0,0.25,0.5));
            return col + glow*0.28;
        }
        case 39u: {
            // Neon rings v2
            float rr = r + 0.08*sin(a*(8.0+6.0*bass) + t*2.6);
            float v = sin(rr*(22.0+18.0*bass) - t*(4.5+5.0*beat) + bass*4.0);
            v = smoothstep(-0.2, 0.85, v);
            float3 col = pal(rr + 0.12*t + 0.2*treb, float3(0.20,0.10,0.10), float3(0.85,0.85,0.95), float3(1.0,1.0,1.0), float3(0.6,0.2,0.0));
            return col * (0.25 + 1.0*v);
        }
        case 40u: {
            // Mandelbrot infinite dive
            float zm = clamp(zoom_mul, 0.35, 2.5);
            float zpow = deep_zoom_pow(t, 0.20 + 0.30*bass, zm, beat, 1.6, 16.4);
            float zoom = exp2(zpow);
            float sc = 1.9 / max(zoom, 1.0);
            float2 c0 = float2(-0.743643887, 0.131825904);
            float2 c = c0 + float2(
                sin(t*(0.17 + 0.12*mid)),
                cos(t*(0.13 + 0.10*treb))
            ) * (sc * (40.0 + 40.0*treb));
            c += p * sc;
            float2 z = float2(0.0);
            int iters = 38 + int(q)*52;
            float nu = (float)iters;
            bool esc = false;
            float trap = 1e9;
            for (int i=0; i<iters; i++) {
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                float m2 = dot(z,z);
                trap = min(trap, length(z - float2(0.18, 0.05)));
                if (m2 > 256.0) {
                    nu = (float)i + 1.0 - log2(max(log2(max(m2, 1.0001)), 1e-6));
                    esc = true;
                    break;
                }
            }
            if (!esc) {
                float iv = clamp(exp(-6.0*trap), 0.0, 1.0);
                iv = 0.15 + 0.85*iv;
                return pal(iv + 0.08*t + 0.10*bass, float3(0.08,0.07,0.16), float3(0.80,0.78,0.92), float3(1.0,1.0,1.0), float3(0.0,0.22,0.52)) * iv;
            }
            float n = clamp(nu / (float)iters, 0.0, 1.0);
            float stripe = 0.5 + 0.5*sin(nu*(0.10 + 0.06*treb) + t*(0.8 + 1.4*beat));
            float v = clamp(pow(1.0 - n, 0.33)*0.72 + stripe*0.28, 0.0, 1.0);
            return pal(v + 0.10*t + 0.12*bass, float3(0.12,0.09,0.20), float3(0.90,0.85,0.98), float3(1.0,1.0,1.0), float3(0.0,0.25,0.5));
        }
        case 41u: {
            // Mandelbrot seahorse dive
            float zm = clamp(zoom_mul, 0.35, 2.5);
            float zpow = deep_zoom_pow(t, 0.18 + 0.28*bass, zm, beat, 1.3, 16.3);
            float zoom = exp2(zpow);
            float sc = 1.8 / max(zoom, 1.0);
            float2 c = float2(-0.7453, 0.1127);
            c += float2(
                sin(t*(0.19 + 0.10*mid)),
                cos(t*(0.15 + 0.10*treb))
            ) * (sc * (46.0 + 34.0*treb));
            c += p * sc;
            float2 z = float2(0.0);
            int iters = 36 + int(q)*50;
            float nu = (float)iters;
            bool esc = false;
            float trap = 1e9;
            for (int i=0; i<iters; i++) {
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                float m2 = dot(z,z);
                trap = min(trap, length(z - float2(0.15, 0.02)));
                if (m2 > 256.0) {
                    nu = (float)i + 1.0 - log2(max(log2(max(m2, 1.0001)), 1e-6));
                    esc = true;
                    break;
                }
            }
            if (!esc) {
                float iv = clamp(exp(-6.0*trap), 0.0, 1.0);
                iv = 0.15 + 0.85*iv;
                return pal(iv + 0.07*t + 0.10*mid, float3(0.08,0.10,0.16), float3(0.82,0.82,0.92), float3(1.0,1.0,1.0), float3(0.1,0.30,0.60)) * iv;
            }
            float n = clamp(nu / (float)iters, 0.0, 1.0);
            float stripe = 0.5 + 0.5*sin(nu*(0.14 + 0.04*treb) - t*(0.9 + 1.2*beat));
            float v = clamp(pow(1.0 - n, 0.34)*0.70 + stripe*0.30, 0.0, 1.0);
            return pal(v + 0.08*t + 0.10*mid, float3(0.08,0.12,0.18), float3(0.92,0.88,0.95), float3(1.0,1.0,1.0), float3(0.1,0.35,0.65));
        }
        case 42u: {
            // Julia infinite bloom
            float zm = clamp(zoom_mul, 0.35, 2.5);
            float zpow = deep_zoom_pow(t, 0.19 + 0.28*mid, zm, beat, 1.2, 15.8);
            float zoom = exp2(zpow);
            float sc = 1.8 / max(zoom, 1.0);
            float2 z = p * sc;
            float2 c = float2(
                -0.745 + 0.16*cos(t*(0.21 + treb*0.25)) + bass*0.05,
                 0.186 + 0.15*sin(t*(0.19 + bass*0.22)) - treb*0.04
            );
            int iters = 34 + int(q)*46;
            float nu = (float)iters;
            bool esc = false;
            float trap = 1e9;
            for (int i=0; i<iters; i++) {
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                float m2 = dot(z,z);
                trap = min(trap, length(z - float2(-0.10, 0.22)));
                if (m2 > 256.0) {
                    nu = (float)i + 1.0 - log2(max(log2(max(m2, 1.0001)), 1e-6));
                    esc = true;
                    break;
                }
            }
            if (!esc) {
                float iv = clamp(exp(-6.5*trap), 0.0, 1.0);
                iv = 0.14 + 0.86*iv;
                return pal(iv + 0.09*t + 0.12*treb, float3(0.10,0.08,0.20), float3(0.82,0.78,0.92), float3(1.0,1.0,1.0), float3(0.18,0.05,0.52)) * iv;
            }
            float n = clamp(nu / (float)iters, 0.0, 1.0);
            float stripe = 0.5 + 0.5*sin(nu*(0.12 + 0.05*treb) - t*(0.9 + 1.2*beat));
            float v = clamp(pow(1.0 - n, 0.30)*0.68 + stripe*0.32, 0.0, 1.0);
            return pal(v + 0.11*t + 0.16*treb, float3(0.10,0.10,0.20), float3(0.88,0.82,0.96), float3(1.0,1.0,1.0), float3(0.2,0.05,0.5));
        }
        case 43u: {
            // Julia cathedral zoom
            float zm = clamp(zoom_mul, 0.35, 2.5);
            float zpow = deep_zoom_pow(t, 0.17 + 0.24*mid, zm, beat, 1.1, 15.7);
            float zoom = exp2(zpow);
            float sc = 1.85 / max(zoom, 1.0);
            float2 z = p * sc;
            float2 c = float2(
                -0.391 + 0.12*cos(t*(0.19 + treb*0.19)) + bass*0.04,
                -0.587 + 0.12*sin(t*(0.16 + bass*0.20)) - treb*0.03
            );
            int iters = 34 + int(q)*44;
            float nu = (float)iters;
            bool esc = false;
            float trap = 1e9;
            for (int i=0; i<iters; i++) {
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                float m2 = dot(z,z);
                trap = min(trap, length(z - float2(0.05, -0.20)));
                if (m2 > 256.0) {
                    nu = (float)i + 1.0 - log2(max(log2(max(m2, 1.0001)), 1e-6));
                    esc = true;
                    break;
                }
            }
            if (!esc) {
                float iv = clamp(exp(-6.5*trap), 0.0, 1.0);
                iv = 0.14 + 0.86*iv;
                return pal(iv + 0.08*t + 0.11*mid, float3(0.08,0.10,0.15), float3(0.84,0.82,0.92), float3(1.0,1.0,1.0), float3(0.05,0.28,0.58)) * iv;
            }
            float n = clamp(nu / (float)iters, 0.0, 1.0);
            float stripe = 0.5 + 0.5*sin(nu*(0.11 + 0.04*treb) + t*(0.8 + beat));
            float v = clamp(pow(1.0 - n, 0.32)*0.70 + stripe*0.30, 0.0, 1.0);
            return pal(v + 0.09*t + 0.14*mid, float3(0.08,0.11,0.16), float3(0.90,0.86,0.96), float3(1.0,1.0,1.0), float3(0.05,0.30,0.60));
        }
        case 44u: {
            // Burning ship abyss dive
            float zm = clamp(zoom_mul, 0.35, 2.5);
            float zpow = deep_zoom_pow(t, 0.17 + 0.26*bass, zm, beat, 1.1, 15.4);
            float zoom = exp2(zpow);
            float sc = 2.0 / max(zoom, 1.0);
            float2 c = float2(-1.7443, -0.0173);
            c += float2(
                sin(t*(0.11 + 0.08*mid)),
                cos(t*(0.09 + 0.07*treb))
            ) * float2(sc * 46.0, sc * 36.0);
            c += p * sc;
            float2 z = float2(0.0);
            int iters = 38 + int(q)*52;
            float nu = (float)iters;
            bool esc = false;
            float trap = 1e9;
            for (int i=0; i<iters; i++) {
                z = float2(abs(z.x), abs(z.y));
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                float m2 = dot(z,z);
                trap = min(trap, length(z - float2(0.12, 0.08)));
                if (m2 > 512.0) {
                    nu = (float)i + 1.0 - log2(max(log2(max(m2, 1.0001)), 1e-6));
                    esc = true;
                    break;
                }
            }
            if (!esc) {
                float iv = clamp(exp(-6.0*trap), 0.0, 1.0);
                iv = 0.16 + 0.84*iv;
                return pal(iv + 0.05*t + 0.16*bass, float3(0.14,0.08,0.04), float3(0.84,0.70,0.54), float3(1.0,1.0,1.0), float3(0.02,0.16,0.48)) * iv;
            }
            float n = clamp(nu / (float)iters, 0.0, 1.0);
            float grain = (0.5 + 0.5*sin(p.x*120.0 + p.y*90.0 + t*(1.2 + 1.8*beat))) * 0.18;
            float v = clamp(pow(1.0 - n, 0.36)*0.82 + grain, 0.0, 1.0);
            return pal(v + 0.05*t + 0.20*bass, float3(0.14,0.08,0.04), float3(0.95,0.80,0.60), float3(1.0,1.0,1.0), float3(0.02,0.18,0.55));
        }
        case 45u: {
            // Orbit trap cavern dive
            float zm = clamp(zoom_mul, 0.35, 2.5);
            float zpow = deep_zoom_pow(t, 0.18 + 0.24*bass, zm, beat, 1.3, 15.8);
            float zoom = exp2(zpow);
            float sc = 1.9 / max(zoom, 1.0);
            float2 c = float2(-0.36, 0.57);
            c += float2(
                sin(t*(0.14 + 0.09*mid)),
                cos(t*(0.11 + 0.08*treb))
            ) * (sc * (38.0 + 30.0*treb));
            c += p * sc;
            float2 z = float2(0.0);
            int iters = 36 + int(q)*48;
            float dmin = 1e9;
            for (int i=0; i<iters; i++) {
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                dmin = min(dmin, abs(length(z - float2(0.18, 0.05)) - (0.22 + 0.12*bass)));
                if (dot(z,z) > 64.0) break;
            }
            float v = clamp(exp(-11.0*dmin), 0.0, 1.0);
            return pal(v + 0.10*t + 0.12*mid, float3(0.06,0.10,0.16), float3(0.90,0.88,1.0), float3(1.0,1.0,1.0), float3(0.15,0.35,0.65)) * (0.22 + 1.45*v);
        }
        case 46u: {
            // Gray-Scott inspired reaction-diffusion field.
            float2 x = pn * (2.8 + 2.6*bass);
            float A = 0.82 + 0.18 * noise2(x * 1.7 + float2(0.0, t*0.08));
            float B = 0.16 + 0.20 * noise2(x * 2.4 + float2(t*0.11, 0.0));
            float feed = 0.026 + 0.022*bass + 0.006*beat;
            float kill = 0.048 + 0.020*treb;
            int steps = 7 + int(q)*2;
            for (int i=0; i<steps; i++) {
                float2 o = float2(cos((float)i*1.618), sin((float)i*1.618)) * 0.11;
                float na = noise2(x + o + float2(0.0, t*0.05));
                float nb = noise2(x*1.16 - o + float2(t*0.06, 0.0));
                float lapA = na - A;
                float lapB = nb - B;
                float reaction = A * B * B;
                A += (0.86*lapA - reaction + feed*(1.0 - A)) * 0.52;
                B += (0.44*lapB + reaction - (kill + feed)*B) * 0.52;
                A = clamp(A, 0.0, 1.0);
                B = clamp(B, 0.0, 1.0);
                x = rot(x + 0.02*float2(B - A, A - B), 0.08 + 0.15*mid);
            }
            float v = clamp(1.15*B - 0.25*A + 0.25*beat, 0.0, 1.0);
            return pal(v + 0.08*t + 0.20*mid, float3(0.05,0.08,0.12), float3(0.95,0.90,0.85), float3(1.0,1.0,1.0), float3(0.02,0.35,0.65)) * (0.20 + 1.45*v);
        }
        case 47u: {
            // Fluid-vorticity style advection with curl-noise transport.
            float2 pos = pn * (1.2 + 1.5*bass);
            float dens = 0.0;
            float vort = 0.0;
            int steps = 12 + int(q)*4;
            for (int i=0; i<steps; i++) {
                float fi = (float)i;
                float2 c = curl_noise(pos * (1.8 + 0.7*treb) + float2(t*0.23 + fi*0.07, -t*0.19));
                float2 drift = c * (0.06 + 0.03*bass) + 0.015*float2(sin(fi+t), cos(fi-t));
                pos += drift;
                float d = fbm_noise(pos*2.2 + float2(t*0.11, -t*0.09));
                dens += exp(-1.9*dot(pos, pos)) * (0.12 + 0.4*d);
                vort += length(c) * 0.04;
            }
            float v = clamp(dens*0.72 + vort*0.35 + 0.15*beat, 0.0, 1.0);
            return pal(v + 0.07*t + 0.15*bass, float3(0.06,0.08,0.10), float3(0.90,0.95,1.0), float3(1.0,1.0,1.0), float3(0.12,0.42,0.75)) * (0.18 + 1.40*v);
        }
        case 48u: {
            // Fractal-flame style IFS variation stack.
            float2 z = pn * (1.0 + 1.0*bass);
            float acc = 0.0;
            float hue = 0.0;
            int iters = 22 + int(q)*10;
            for (int i=0; i<iters; i++) {
                float fi = (float)i;
                float h = hash21(z*3.2 + float2(fi*0.13, t*0.17));
                if (h < 0.33) {
                    z = float2(sin(z.x*2.1), sin(z.y*2.1)); // sinusoidal
                } else if (h < 0.66) {
                    float d = dot(z, z) + 0.18;              // spherical
                    z = z / d;
                } else {
                    z = float2(atan2(z.y, z.x) * 0.42, length(z) - (0.55 + 0.15*mid)); // polar
                }
                z = rot(z, 0.35 + 0.95*mid + 0.03*fi) +
                    0.27*float2(sin(t*0.8 + fi*0.37), cos(t*0.7 - fi*0.31));
                float wv = exp(-2.2*dot(z, z));
                acc += wv;
                hue += wv * h;
            }
            float v = clamp(acc * (0.09 + 0.12*treb), 0.0, 1.0);
            float hv = fract(hue * 1.9 + 0.15*t + 0.4*beat);
            return pal(hv + 0.25*v, float3(0.08,0.05,0.10), float3(0.98,0.92,0.86), float3(1.0,1.0,1.0), float3(0.0,0.20,0.65)) * (0.20 + 1.55*v);
        }
        case 49u: {
            // Sphere-traced mandelbulb volume.
            float3 ro = float3(0.0, 0.0, -3.0 + 0.75*bass);
            float3 rd = normalize(float3(p.x, p.y, 1.85));
            float hit = sphere_trace_scene(ro, rd, t, bass, mid, treb, 0u);
            float fog = exp(-2.6 * length(p));
            float v = clamp(hit*0.9 + fog*0.3 + 0.12*beat, 0.0, 1.0);
            return pal(v + 0.06*t + 0.2*bass, float3(0.06,0.08,0.14), float3(0.95,0.90,0.98), float3(1.0,1.0,1.0), float3(0.02,0.28,0.58)) * (0.20 + 1.4*v);
        }
        case 50u: {
            // Sphere-traced gyroid temple.
            float3 ro = float3(0.0, 0.0, -2.7 + 0.65*bass);
            float3 rd = normalize(float3(p.x, p.y, 1.6));
            float hit = sphere_trace_scene(ro, rd, t, bass, mid, treb, 1u);
            float rim = pow(clamp(1.0 - length(p)*0.7, 0.0, 1.0), 1.6);
            float v = clamp(hit*0.92 + rim*0.35 + 0.08*treb, 0.0, 1.0);
            return pal(v + 0.08*t + 0.15*mid, float3(0.07,0.09,0.12), float3(0.90,0.97,0.95), float3(1.0,1.0,1.0), float3(0.10,0.42,0.75)) * (0.20 + 1.35*v);
        }
        case 51u: {
            // Curl-noise advection ink.
            float2 pos = pn * (2.0 + 1.5*bass);
            float d = 0.0;
            for (int i=0; i<9 + int(q)*3; i++) {
                float fi = (float)i;
                float2 c = curl_noise(pos * 2.1 + float2(t*0.24 + fi*0.17, -t*0.18));
                pos += c * (0.09 + 0.05*treb);
                d += fbm_noise(pos*2.3 + float2(fi*0.1, -fi*0.07));
            }
            float v = clamp((d / (9.0 + (float)int(q)*3.0)) * 1.35 + 0.18*beat, 0.0, 1.0);
            return pal(v + 0.10*t + 0.16*mid, float3(0.06,0.06,0.10), float3(0.95,0.90,0.88), float3(1.0,1.0,1.0), float3(0.12,0.33,0.66)) * (0.22 + 1.3*v);
        }
        case 52u: {
            // Perlin-ish domain-warp liquid aurora.
            float2 w0 = pn;
            float2 w1 = float2(
                fbm_noise(w0*3.0 + float2(0.0, t*0.32)),
                fbm_noise(w0*3.0 + float2(4.2, -t*0.28))
            );
            float2 w2 = w0 + (w1*2.0 - 1.0) * (0.38 + 0.22*bass);
            float v = fbm_noise(w2*(4.0 + 3.0*treb) + float2(t*0.8, -t*0.6));
            float rib = 0.5 + 0.5*sin((w2.x + w2.y) * (10.0 + 14.0*mid) + t*(2.0 + 5.0*beat));
            v = clamp(v*0.7 + rib*0.45, 0.0, 1.0);
            return pal(v + 0.10*t + 0.20*treb, float3(0.07,0.10,0.12), float3(0.92,0.96,0.95), float3(1.0,1.0,1.0), float3(0.15,0.45,0.75)) * (0.2 + 1.32*v);
        }
        case 53u: {
            // Strange-attractor ribbon density (de-Jong style accumulation).
            float a0 = 1.40 + 1.1*bass;
            float b0 = 1.86 + 0.9*treb;
            float c0 = 1.52 + 0.8*mid;
            float d0 = 1.94 + 0.6*bass;
            float2 z = float2(0.1*sin(t*0.7), 0.1*cos(t*0.6));
            float acc = 0.0;
            int iters = 42 + int(q)*16;
            for (int i=0; i<iters; i++) {
                float x = sin(a0*z.y + 0.03*(float)i) - cos(b0*z.x - 0.02*t);
                float y = sin(c0*z.x - 0.02*(float)i) - cos(d0*z.y + 0.03*t);
                z = float2(x, y);
                float2 k = z * 0.34;
                float d = length(pn*1.25 - k);
                acc += exp(-18.0*d*d);
            }
            float v = clamp(acc * (0.040 + 0.035*beat), 0.0, 1.0);
            return pal(v + 0.06*t + 0.18*mid, float3(0.06,0.05,0.10), float3(0.96,0.90,0.86), float3(1.0,1.0,1.0), float3(0.22,0.05,0.55)) * (0.2 + 1.45*v);
        }
        case 54u: {
            // Fractal morph: blend deep Mandel-style stripes with flame texture.
            float2 c = float2(p.x*0.82 - 0.58 + 0.06*sin(t*0.18 + bass*2.0), p.y*0.82 + 0.03*cos(t*0.15 + mid*1.8));
            float2 z = float2(0.0);
            int iters = 26 + int(q)*26;
            float nu = (float)iters;
            bool esc = false;
            for (int i=0; i<iters; i++) {
                z = float2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
                float m2 = dot(z,z);
                if (m2 > 128.0) {
                    nu = (float)i + 1.0 - log2(max(log2(max(m2, 1.0001)), 1e-6));
                    esc = true;
                    break;
                }
            }
            float mand = esc ? clamp(1.0 - nu / (float)iters, 0.0, 1.0) : 0.0;
            float2 fz = pn * (1.0 + 0.9*bass);
            float fl = 0.0;
            for (int i=0; i<12; i++) {
                float fi = (float)i;
                fz = rot(fz, 0.38 + 0.9*mid + fi*0.03) + 0.24*float2(sin(fi+t*0.7), cos(fi-t*0.5));
                fl += exp(-2.4*dot(fz,fz));
            }
            fl = clamp(fl * 0.16, 0.0, 1.0);
            float mixf = 0.5 + 0.5*sin(t*0.22 + 4.0*beat + 2.0*treb);
            float v = mix(mand, fl, mixf);
            return pal(v + 0.10*t + 0.14*bass, float3(0.08,0.07,0.12), float3(0.95,0.88,0.92), float3(1.0,1.0,1.0), float3(0.0,0.30,0.68)) * (0.22 + 1.36*v);
        }
        case 55u: {
            // SDF fractal monolith (repeated gyroid + tunnel fog).
            float2 rp = p;
            float2 rr = rot(rp, 0.2*sin(t*0.3) + 0.5*bass);
            float3 ro = float3(0.0, 0.0, -3.1);
            float3 rd = normalize(float3(rr.x, rr.y, 1.75));
            float hit = sphere_trace_scene(ro, rd, t*0.85, bass, mid, treb, 1u);
            float tunnel = exp(-2.0*abs(length(rr) - (0.35 + 0.22*sin(t*0.6))));
            float pulse = 0.5 + 0.5*sin((rr.x*7.0 - rr.y*9.0) + t*(2.0 + 4.0*beat));
            float v = clamp(hit*0.85 + tunnel*0.35 + pulse*0.25*treb, 0.0, 1.0);
            return pal(v + 0.12*t + 0.16*bass, float3(0.06,0.08,0.11), float3(0.92,0.96,0.98), float3(1.0,1.0,1.0), float3(0.12,0.38,0.72)) * (0.22 + 1.34*v);
        }
    }
}

inline float smooth_transient_drive(float beat, float onset) {
    float x = clamp(0.62*clamp(beat, 0.0, 1.0) + 0.38*clamp(onset, 0.0, 1.0), 0.0, 1.0);
    return x * x * (3.0 - 2.0 * x);
}

inline float smooth_motion_drive(float bass, float mid, float treb, float beat, float onset) {
    float groove = clamp(0.58*bass + 0.27*mid + 0.15*treb, 0.0, 1.0);
    float pulse = clamp(beat, 0.0, 1.0);
    float trans = clamp(onset, 0.0, 1.0);
    float accent = max(pulse, trans);
    float x = clamp(0.76*groove + 0.16*accent + 0.08*pulse, 0.0, 1.0);
    float eased = x * x * (3.0 - 2.0 * x);
    return clamp(eased * (0.90 + 0.20*accent), 0.0, 1.0);
}

kernel void visualize(
    texture2d<half, access::sample> prevTex [[texture(0)]],
    texture2d<half, access::write> outTex [[texture(1)]],
    sampler s [[sampler(0)]],
    constant Uniforms& u [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]
) {
    if (gid.x >= u.w || gid.y >= u.h) return;

    float2 uv = (float2(gid) + 0.5) / float2((float)u.w, (float)u.h);
    float aspect = (float)u.w / max((float)u.h, 1.0);
    float2 p = uv * 2.0 - 1.0;
    p.x *= aspect;

    float sub = clamp(u.bands[0], 0.0, 1.0);
    float bass = clamp(u.bands[1], 0.0, 1.0);
    float lowmid = clamp(u.bands[2], 0.0, 1.0);
    float mid = clamp(0.62*u.bands[3] + 0.23*lowmid + 0.15*clamp(u.bands[4], 0.0, 1.0), 0.0, 1.0);
    float highmid = clamp(u.bands[4], 0.0, 1.0);
    float treb = clamp(u.bands[5], 0.0, 1.0);
    float air = clamp(u.bands[6], 0.0, 1.0);
    float pres = clamp(u.bands[7], 0.0, 1.0);

    float beat = clamp(u.beat_pulse, 0.0, 1.0);
    float onset = clamp(u.onset, 0.0, 1.0);
    float energy = clamp(u.rms, 0.0, 1.0);
    float t = u.time;
    float motion = smooth_motion_drive(bass, mid, treb, beat, onset);
    float transient = smooth_transient_drive(beat, onset);

    float wobble = 0.012 + 0.034*motion + 0.015*transient + 0.008*treb + 0.006*highmid;
    float2 q = p;
    float2 pn_base = float2(p.x / max(aspect, 1e-5), p.y);
    q += wobble * float2(
        sin(p.y*(3.0 + 0.8*motion) + t*(1.3 + 0.8*motion) + 7.0*treb + 2.0*transient),
        cos(p.x*(2.8 + 0.6*motion) - t*(1.1 + 0.7*motion) + 5.0*mid - 1.8*transient)
    );
    q = rot(q, 0.04*sin(t*(0.18 + 0.10*motion)) + 0.12*motion + 0.05*transient);

    float alpha = clamp(u.transition_alpha, 0.0, 1.0);

    float2 q0 = q;
    float2 q1 = q;
    bool travel0 = (u.fractal_zoom_mul > 0.0) && is_camera_travel_preset(u.active_preset);
    bool travel1 = (u.fractal_zoom_mul > 0.0) && is_camera_travel_preset(u.next_preset);
    if (travel0 || (alpha > 0.001 && travel1)) {
        uint mode_override = min(u.camera_path_mode, 5u);
        uint mode0 = (mode_override == 0u) ? camera_path_mode_for_preset(u.active_preset) : mode_override;
        uint mode1 = (mode_override == 0u) ? camera_path_mode_for_preset(u.next_preset) : mode_override;
        float path_speed = clamp(u.camera_path_speed, 0.15, 4.0);
        float mix0 = travel0 ? (1.0 - alpha) : 0.0;
        float mix1 = travel1 ? alpha : 0.0;
        q0 = apply_camera_path(
            q0,
            t * path_speed,
            motion,
            transient,
            bass,
            mid,
            treb,
            beat,
            u.fractal_zoom_mul,
            mode0,
            mix0
        );
        q1 = apply_camera_path(
            q1,
            t * path_speed,
            motion,
            transient,
            bass,
            mid,
            treb,
            beat,
            u.fractal_zoom_mul,
            mode1,
            mix1
        );
    }
    if (u.transition_kind == 1u) {
        // Zoom-through: old zooms out, new zooms in.
        float k = 0.55 + 0.55*bass;
        float z0 = 1.0 + alpha * k;
        float z1 = 1.0 + (1.0 - alpha) * k;
        q0 = rot(q * z0, -0.25*alpha + 0.12*bass);
        q1 = rot(q / z1, 0.25*(1.0-alpha) + 0.10*treb);
    } else if (u.transition_kind == 2u) {
        // Add mild coordinate jitter on entry for a glitchier feel.
        float j = (1.0 - alpha) * (0.02 + 0.10*clamp(u.onset + beat, 0.0, 1.0));
        q0 += j * float2(sin(p.y*11.0 + t*18.0), cos(p.x*9.0 - t*16.0));
        q1 = q0;
    } else if (u.transition_kind == 4u) {
        // Swirl morph: opposite spin fields for outgoing/incoming presets.
        float spin = (1.2 + 2.6*bass + 1.4*mid) * (1.0 + 0.3*sin(t*0.7));
        float r = clamp(length(p), 0.0, 1.4);
        float falloff = pow(clamp(1.0 - r*0.85, 0.0, 1.0), 1.1);
        float a0 = spin * falloff * (1.0 - alpha);
        float a1 = -spin * falloff * alpha;
        q0 = rot(q, a0);
        q1 = rot(q, a1);
    } else if (u.transition_kind == 5u) {
        // Dissolve with slight unstable entry wobble.
        float j = (1.0 - alpha) * (0.008 + 0.030*clamp(u.onset + beat + treb*0.5, 0.0, 1.0));
        q1 += j * float2(
            sin((p.y + p.x*0.6)*13.0 + t*22.0),
            cos((p.x - p.y*0.4)*11.0 - t*19.0)
        );
    } else if (u.transition_kind == 7u) {
        // Flow morph: opposing curl-noise advection fields.
        float2 n0 = curl_noise(pn_base * (2.3 + 1.4*bass) + float2(t*0.40, -t*0.36));
        float2 n1 = curl_noise(pn_base * (2.9 + 1.1*treb) + float2(-t*0.33, t*0.29));
        float amp = (0.09 + 0.20*clamp(mid + treb*0.4, 0.0, 1.0)) * (1.0 - 0.35*alpha);
        q0 += n0 * amp * (1.0 - alpha);
        q1 -= n1 * amp * alpha;
    } else if (u.transition_kind == 11u) {
        // Prism split transition with subtle opposing drifts.
        float split = (0.012 + 0.060*clamp(treb + 0.6*air, 0.0, 1.0)) * (1.0 - 0.35*alpha);
        float tw = sin((p.x*8.0 - p.y*6.0) + t*(2.0 + 5.0*treb)) * 0.06 * (1.0 - alpha);
        q0 += float2(split + tw, -0.7*tw) * (1.0 - alpha);
        q1 -= float2(split - 0.4*tw, 0.8*tw) * alpha;
    } else if (u.transition_kind == 12u) {
        // Remix: dual flow fields crossing into each other.
        float2 f0 = curl_noise(pn_base * (3.0 + 2.0*bass) + float2(t*0.27, -t*0.22));
        float2 f1 = curl_noise(pn_base * (3.4 + 1.6*treb) + float2(-t*0.25, t*0.31));
        float amp = (0.06 + 0.24*clamp(bass + mid + 0.4*treb, 0.0, 1.0)) * (1.0 - 0.25*alpha);
        q0 += f0 * amp * (1.0 - alpha);
        q1 += f1 * amp * alpha;
    } else if (u.transition_kind == 13u) {
        // Echo smear.
        float smear = (0.018 + 0.085*(1.0 - alpha)*clamp(beat + bass, 0.0, 1.0));
        q0 += float2(sin(p.y*22.0 + t*12.0), 0.0) * smear;
        q1 += float2(0.0, cos(p.x*20.0 - t*11.0)) * smear;
    }

    float3 c0 = preset_color(u.active_preset, q0, t, bass, mid, treb, beat, u.quality, aspect, u.fractal_zoom_mul);
    float3 c1 = preset_color(u.next_preset, q1, t, bass, mid, treb, beat, u.quality, aspect, u.fractal_zoom_mul);
    float mix_alpha = alpha;
    if (u.transition_kind == 3u) {
        // Radial/ripple wipe from center.
        float rr = clamp(length(p) * 0.70710677, 0.0, 1.0);
        float feather = 0.035 + 0.11*bass;
        float theta = atan2(p.y, p.x);
        float ripple = sin(theta*8.0 + t*(2.0 + 6.0*treb)) * (0.02 + 0.04*treb) * (1.0 - alpha);
        float thr = clamp(alpha + ripple, 0.0, 1.0);
        mix_alpha = 1.0 - smoothstep(thr - feather, thr + feather, rr);
    } else if (u.transition_kind == 5u) {
        // Noise dissolve / block reveal.
        float drive = clamp(u.onset + u.beat_pulse + treb*0.7, 0.0, 1.0);
        float blocks = mix(8.0, 56.0, drive);
        float2 id = floor(uv * blocks);
        float n0 = hash21(id + float2((float)(u.seed & 65535u), (float)(u.seed >> 16)));
        float n1 = hash21(float2((float)gid.x, (float)gid.y) + float2((float)(u.seed ^ 0xA7C153E9u), 19.0));
        float scan = sin(uv.y*38.0 + t*(9.0 + 28.0*drive)) * (0.02 + 0.04*(1.0 - alpha));
        float n = clamp(0.82*n0 + 0.18*n1 + scan, 0.0, 1.0);
        float feather = clamp(0.08 - 0.05*drive, 0.015, 0.09);
        mix_alpha = 1.0 - smoothstep(alpha - feather, alpha + feather, n);
    } else if (u.transition_kind == 6u) {
        // Jump cut with tearing scan.
        float drive = clamp(u.onset + beat + bass*0.6, 0.0, 1.0);
        float gate = clamp(alpha * (1.35 + 0.65*drive), 0.0, 1.0);
        float tear = sin(uv.y*(30.0 + 40.0*drive) + t*(18.0 + 70.0*drive)) * (0.012 + 0.040*(1.0-alpha));
        mix_alpha = step(uv.x + tear, gate);
    } else if (u.transition_kind == 7u) {
        // Morph mask bends with structured noise.
        float n = fbm_noise(pn_base * (4.0 + 3.0*treb) + float2(t*0.8, -t*0.7));
        float m = clamp(0.35 + 0.9*alpha + 0.28*(n - 0.5), 0.0, 1.0);
        mix_alpha = smoothstep(0.0, 1.0, m);
    } else if (u.transition_kind == 8u) {
        // Directional wipe with wave edge.
        float drive = clamp(u.onset + beat + bass*0.5, 0.0, 1.0);
        float seed_phase = 6.2831853 * hash21(float2((float)(u.seed & 1023u), (float)(u.seed >> 22)));
        float ang = t*(0.35 + 0.8*drive) + seed_phase;
        float2 dir = float2(cos(ang), sin(ang));
        float d = dot(p, dir) + sin((p.x*12.0 + p.y*9.0) + t*(5.0 + 9.0*drive)) * 0.06 * (1.0 - alpha);
        float thr = alpha * 2.0 - 1.0;
        float feather = 0.05 + 0.09*(1.0 - drive);
        mix_alpha = smoothstep(thr - feather, thr + feather, d);
    } else if (u.transition_kind == 9u) {
        // Luma-key reveal driven by incoming frame luminance.
        float lum = dot(c1, float3(0.2126, 0.7152, 0.0722));
        float n = fbm_noise(pn_base * (6.0 + 4.0*treb) + float2(t*0.5, -t*0.47)) - 0.5;
        float feather = clamp(0.08 - 0.04*clamp(u.onset + u.beat_pulse, 0.0, 1.0), 0.02, 0.1);
        mix_alpha = smoothstep(alpha - feather, alpha + feather, clamp(lum + 0.20*n, 0.0, 1.0));
    } else if (u.transition_kind == 10u) {
        // Flash cut uses regular crossfade gate and a post mix flash.
        mix_alpha = smoothstep(0.20, 0.80, alpha);
    } else if (u.transition_kind == 12u) {
        // Remix mask blends edge waves and noise.
        float n = fbm_noise(pn_base * (5.0 + 4.0*treb) + float2(t*0.9, -t*0.8));
        float edge = 0.5 + 0.5*sin((p.x*9.0 - p.y*7.0) + t*(2.0 + 6.0*mid));
        float m = clamp(0.62*alpha + 0.26*edge + 0.22*(n - 0.5), 0.0, 1.0);
        mix_alpha = smoothstep(0.0, 1.0, m);
    }
    float3 col = mix(c0, c1, clamp(mix_alpha, 0.0, 1.0));

    if (u.transition_kind == 10u) {
        float drive = clamp(u.onset + u.beat_pulse + bass*0.45, 0.0, 1.0);
        float flash = pow(clamp(1.0 - abs(alpha*2.0 - 1.0), 0.0, 1.0), 1.6) * (0.35 + 0.55*drive);
        col = clamp(col + flash, 0.0, 1.0);
    } else if (u.transition_kind == 11u) {
        float split = (0.08 + 0.32*(1.0 - alpha)) * clamp(treb + 0.6*air, 0.0, 1.0);
        col = clamp(float3(col.r + 0.12*split, col.g, col.b + 0.08*split), 0.0, 1.0);
    }

    // Feedback warp for hypnotic motion.
    float2 pn = float2(q.x / aspect, q.y); // back to -1..1
    float2 puv = pn * 0.5 + 0.5;
    float3 prev = float3(0.0);
    if (u.has_prev != 0u) {
        prev = float3(prevTex.sample(s, clamp(puv, 0.0, 1.0)).rgb);
    }

    // Transition preset: datamosh smear (macroblocks + chroma split) using temporal feedback.
    if (u.transition_kind == 2u && u.has_prev != 0u) {
        float drive = clamp(u.onset + beat + treb*0.7, 0.0, 1.0);
        float blocks = mix(10.0, 54.0, clamp(treb + 0.5*air + 0.4*pres, 0.0, 1.0));
        float2 bid = floor(uv * blocks);
        float n = hash21(bid + float2((float)(u.seed & 65535u), (float)(u.seed >> 16)));
        float2 off = (float2(fract(n*23.1), fract(n*91.7)) * 2.0 - 1.0);
        float amp = (0.003 + 0.05*drive) * (1.0 - alpha);
        off *= amp;
        off.x += sin(uv.y*blocks*0.35 + t*(18.0 + 55.0*drive)) * (0.004 + 0.02*(1.0-alpha));
        float2 suv = clamp(puv + off, 0.0, 1.0);

        float ca = (0.002 + 0.012*clamp(treb,0.0,1.0)) * (1.0 - alpha);
        float3 mosh;
        mosh.r = prevTex.sample(s, clamp(suv + float2(ca, 0.0), 0.0, 1.0)).r;
        mosh.g = prevTex.sample(s, suv).g;
        mosh.b = prevTex.sample(s, clamp(suv - float2(ca, 0.0), 0.0, 1.0)).b;

        float m = (1.0 - alpha) * ((u.safe != 0u) ? 0.28 : 0.78) * (0.35 + 0.65*drive);
        col = mix(col, mosh, clamp(m, 0.0, 0.92));
    }
    if (u.transition_kind == 12u && u.has_prev != 0u) {
        float remix = (0.10 + 0.42*(1.0 - alpha)) * clamp(0.35 + 0.65*(energy + beat), 0.0, 1.0);
        float2 rv = clamp(
            puv + 0.008*float2(sin(uv.y*90.0 + t*8.0), cos(uv.x*82.0 - t*7.0))*(1.0 - alpha),
            0.0,
            1.0
        );
        float3 pre = float3(prevTex.sample(s, rv).rgb);
        col = mix(col, pre.bgr, clamp(remix, 0.0, 0.80));
    }
    if (u.transition_kind == 13u && u.has_prev != 0u) {
        float em = (0.14 + 0.58*(1.0 - alpha)) * clamp(0.3 + 0.7*(bass + beat), 0.0, 1.0);
        float2 ev = clamp(
            puv + float2(
                0.012*sin(uv.y*70.0 + t*14.0),
                0.010*cos(uv.x*65.0 - t*13.0)
            ) * (1.0 - alpha),
            0.0,
            1.0
        );
        float3 echo = float3(prevTex.sample(s, ev).rgb);
        col = mix(col, echo, clamp(em, 0.0, 0.88));
    }

    float fb = 0.78 + 0.10*energy + 0.07*sub;
    if (u.transition_kind == 2u || u.transition_kind == 12u || u.transition_kind == 13u) {
        fb = min(0.96, fb + 0.10*(1.0 - alpha));
    }
    float3 out = mix(col, prev, clamp(fb, 0.0, 0.96));

    // Beat strobe (dialed down in safe mode).
    float strobe = (u.safe != 0u) ? 0.18 : 0.32;
    out *= 1.0 + strobe * beat;

    // Treble sparkle / onset glint.
    float spark = smoothstep(0.25, 0.95, treb + 0.6*highmid + air + 0.5*pres) * (0.15 + 0.35*clamp(u.onset,0.0,1.0));
    out += spark * pal(uv.x + uv.y + 0.25*t, float3(0.0), float3(0.7,0.9,1.0), float3(1.0), float3(0.0,0.15,0.33));

    // Soft vignetting.
    float v = smoothstep(1.4, 0.2, length(p));
    out *= (0.45 + 0.65*v);

    // Reactive post-FX: adaptive saturation, soft-knee highlights, and micro-grain.
    float lum = dot(out, float3(0.2126, 0.7152, 0.0722));
    float fx_drive = clamp(0.42*energy + 0.34*transient + 0.24*treb, 0.0, 1.0);
    float sat = (u.safe != 0u) ? (0.94 + 0.20*fx_drive) : (0.98 + 0.32*fx_drive);
    out = mix(float3(lum), out, clamp(sat, 0.82, 1.30));

    float knee = (u.safe != 0u) ? 0.78 : 0.86;
    float soft = (u.safe != 0u) ? 0.16 : 0.24;
    float3 over = max(out - knee, 0.0);
    out = out / (1.0 + over * soft);

    float grain = hash21(float2((float)gid.x * 0.73 + t*17.0, (float)gid.y * 1.19 - t*13.0)) - 0.5;
    float grain_amp = ((u.safe != 0u) ? 0.008 : 0.014) * (0.45 + 0.55*fx_drive);
    out += grain * grain_amp;

    // Clamp for terminal brightness.
    out = clamp(out, 0.0, (u.safe != 0u) ? 0.92 : 1.0);

    outTex.write(half4(half(out.r), half(out.g), half(out.b), half(1.0)), gid);
}
"#;
