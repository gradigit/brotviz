use crate::audio::AudioSystem;
use crate::capability::probe_runtime;
use crate::config::{Config, EngineMode, Quality, RendererMode, SwitchMode, SystemDataMode};
use crate::control_matrix::{ControlMatrix, ControlState};
use crate::lyrics::LyricsTrack;
use crate::prefs::{self, AppPrefs};
use crate::render::{AsciiRenderer, BrailleRenderer, Frame, HalfBlockRenderer, KittyRenderer, Renderer, SextantRenderer};
use crate::system_data::SystemDataFeed;
use crate::theme_pack::ThemePackManifest;
use crate::terminal::TerminalGuard;
use crate::typography::{
    apply_typography_overlay_pixels, typography_overlay_text, typography_reactive_audio,
    TypographyMode,
};
use crate::visual::{make_presets, CameraPathMode, PresetEngine, RenderCtx, VisualEngine};
use anyhow::Context;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::io::{BufWriter, IsTerminal};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Playlist {
    name: String,
    preset_indices: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlaylistFocus {
    Playlists,
    Presets,
}

#[derive(Clone, Debug)]
struct PlaylistUi {
    open: bool,
    focus: PlaylistFocus,
    playlist_cursor: usize,
    preset_cursor: usize,
}

impl PlaylistUi {
    fn new() -> Self {
        Self {
            open: false,
            focus: PlaylistFocus::Playlists,
            playlist_cursor: 0,
            preset_cursor: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectorKind {
    Theme,
    Graph,
    Lyrics,
    Typography,
}

impl SelectorKind {
    fn label(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Graph => "graph",
            Self::Lyrics => "lyrics",
            Self::Typography => "typography",
        }
    }
}

#[derive(Clone, Debug)]
struct SelectorUi {
    open: bool,
    kind: SelectorKind,
    cursor: usize,
}

impl SelectorUi {
    fn new() -> Self {
        Self {
            open: false,
            kind: SelectorKind::Theme,
            cursor: 0,
        }
    }

    fn open(&mut self, kind: SelectorKind, cursor: usize) {
        self.open = true;
        self.kind = kind;
        self.cursor = cursor;
    }

    fn close(&mut self) {
        self.open = false;
    }
}

#[derive(Clone, Debug)]
struct ThemeOption {
    label: String,
    pack: Option<ThemePackManifest>,
    preset_indices: Vec<usize>,
}

#[derive(Clone, Debug)]
struct GraphOption {
    label: String,
    preset_indices: Vec<usize>,
    entry_preset: Option<usize>,
}

#[derive(Clone, Debug)]
struct LyricOption {
    label: String,
    path: Option<PathBuf>,
}

enum SelectorAction {
    None,
    Quit,
    ApplyTheme(usize),
    ApplyGraph(usize),
    ApplyLyrics(usize),
    ApplyTypography(TypographyMode),
}

struct HudFlash {
    key: &'static str,
    until: Instant,
}

impl HudFlash {
    fn new(now: Instant, key: &'static str) -> Self {
        Self {
            key,
            until: now + Duration::from_millis(900),
        }
    }

    fn active(&self, now: Instant) -> bool {
        now < self.until
    }

    fn blink_phase(&self, now: Instant) -> bool {
        if now >= self.until {
            return false;
        }
        (self.until.duration_since(now).as_millis() / 120) % 2 == 0
    }
}

struct LatencyCalibration {
    enabled: bool,
    manual_offset_ms: f32,
    auto_offset_ms: f32,
    prev_audio: Option<crate::audio::AudioFeatures>,
}

impl LatencyCalibration {
    fn new(enabled: bool, manual_offset_ms: f32) -> Self {
        Self {
            enabled,
            manual_offset_ms: manual_offset_ms.clamp(-240.0, 240.0),
            auto_offset_ms: 0.0,
            prev_audio: None,
        }
    }

    fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    fn nudge_manual_offset(&mut self, delta_ms: f32) {
        self.manual_offset_ms = (self.manual_offset_ms + delta_ms).clamp(-240.0, 240.0);
    }

    fn reset_manual_offset(&mut self) {
        self.manual_offset_ms = 0.0;
    }

    fn observe_latency(&mut self, end_to_end_ms: f32) {
        if !self.enabled {
            return;
        }
        let target_ms = 28.0f32;
        let suggested = (end_to_end_ms - target_ms).clamp(-160.0, 160.0);
        self.auto_offset_ms = if self.auto_offset_ms == 0.0 {
            suggested
        } else {
            self.auto_offset_ms * 0.92 + suggested * 0.08
        };
    }

    fn effective_offset_ms(&self) -> f32 {
        let auto = if self.enabled { self.auto_offset_ms } else { 0.0 };
        (self.manual_offset_ms + auto).clamp(-240.0, 240.0)
    }

    fn status_label(&self) -> String {
        format!(
            "{} man:{:+.0}ms auto:{:+.0}ms eff:{:+.0}ms",
            if self.enabled { "on" } else { "off" },
            self.manual_offset_ms,
            if self.enabled { self.auto_offset_ms } else { 0.0 },
            self.effective_offset_ms()
        )
    }

    fn apply_phase_correction(
        &mut self,
        audio: crate::audio::AudioFeatures,
        dt: f32,
    ) -> crate::audio::AudioFeatures {
        let mut out = audio;
        let offset_ms = self.effective_offset_ms();
        if offset_ms > 0.5 {
            if let Some(prev) = self.prev_audio {
                let horizon = (offset_ms / 1000.0).min(0.20);
                let inv_dt = 1.0 / dt.max(1e-3);
                let onset_slope = (audio.onset - prev.onset) * inv_dt;
                let beat_slope = (audio.beat_strength - prev.beat_strength) * inv_dt;
                out.onset = (audio.onset + onset_slope * horizon).clamp(0.0, 1.0);
                out.beat_strength = (audio.beat_strength + beat_slope * horizon).clamp(0.0, 1.0);
                let rising = audio.onset - prev.onset;
                if !out.beat && rising > 0.07 && out.beat_strength > 0.45 {
                    out.beat = true;
                    out.beat_strength = out.beat_strength.max((rising * 4.0).clamp(0.0, 1.0));
                }
            }
        } else if offset_ms < -0.5 {
            let lag_s = (-offset_ms / 1000.0).min(0.25);
            let decay = (0.1f32).powf((lag_s / 0.12).clamp(0.0, 2.0));
            out.onset = (out.onset * decay).clamp(0.0, 1.0);
            out.beat_strength = (out.beat_strength * decay).clamp(0.0, 1.0);
            if lag_s > 0.08 {
                out.beat = false;
            }
        }
        self.prev_audio = Some(audio);
        out
    }
}

struct AppState {
    intensity: f32,
    zoom_drive: f32,
    show_hud: bool,
    show_help: bool,
    stage_mode: bool,
    playlist_ui: PlaylistUi,
    selector_ui: SelectorUi,
    theme_selected: usize,
    graph_selected: usize,
    lyrics_selected: usize,
    typography_mode: TypographyMode,
    typography_last_non_off: TypographyMode,
    latency_calibration: LatencyCalibration,
    system_data_mode: SystemDataMode,
    system_data_feed: Option<SystemDataFeed>,
}

#[derive(Default)]
struct ControlRuntime {
    state: ControlState,
}

pub fn run(cfg: Config) -> anyhow::Result<()> {
    let mut startup_warnings = Vec::new();

    let prefs_store = prefs::prefs_storage_path();
    let mut app_prefs = match AppPrefs::load(prefs_store.as_deref()) {
        Ok(p) => p,
        Err(err) => {
            push_warning(
                &mut startup_warnings,
                format!("prefs load failed (continuing with defaults): {err}"),
            );
            AppPrefs::default()
        }
    };

    let stage_mode = if cfg.stage_mode {
        true
    } else {
        app_prefs.stage_mode
    };

    let mut capability = probe_runtime(cfg.engine, cfg.renderer, cfg.auto_probe);
    for note in capability.notes().iter().cloned() {
        push_warning(&mut startup_warnings, note);
    }
    if stage_mode != app_prefs.stage_mode {
        app_prefs.stage_mode = stage_mode;
        if let Err(err) = app_prefs.save(prefs_store.as_deref()) {
            push_warning(
                &mut startup_warnings,
                format!("prefs save failed (stage_mode init): {err}"),
            );
        }
    }

    let _term = TerminalGuard::new()?;
    let mut out = BufWriter::new(TerminalGuard::stdout());

    let mut renderer: Box<dyn Renderer> = match capability.renderer {
        RendererMode::Ascii => Box::new(AsciiRenderer::new()),
        RendererMode::HalfBlock => Box::new(HalfBlockRenderer::new()),
        RendererMode::Braille => Box::new(BrailleRenderer::new()),
        RendererMode::Sextant => Box::new(SextantRenderer::new()),
        RendererMode::Kitty => Box::new(KittyRenderer::new()),
    };

    let (px_w_mul, px_h_mul) = match capability.renderer {
        RendererMode::Ascii => (1usize, 1usize),
        RendererMode::HalfBlock => (1usize, 2usize),
        // Render at 2x4 pixels per cell for Kitty to look materially sharper than half-block.
        RendererMode::Kitty => (2usize, 4usize),
        RendererMode::Braille => (2usize, 4usize),
        RendererMode::Sextant => (2usize, 3usize),
    };

    let audio = AudioSystem::new(cfg.source, cfg.device.as_deref())
        .with_context(|| format!("start audio (source={:?})", cfg.source))?;
    let audio_features = audio.features();

    let presets = make_presets();
    let preset_names = presets.iter().map(|p| p.name()).collect::<Vec<_>>();
    let preset_count = preset_names.len();
    let mut requested_active = select_preset(&cfg.preset, &presets);

    let mut intensity = 1.0f32;
    let mut zoom_drive = 1.0f32;
    let mut loaded_theme_name = String::new();
    let mut loaded_graph_name = String::new();
    let mut default_playlist_name: Option<String> = None;
    let mut default_playlist_indices: Option<Vec<usize>> = None;
    let mut control_matrix: Option<ControlMatrix> = None;
    let mut control_runtime = ControlRuntime::default();

    if let Some(path) = cfg.preset_graph.as_deref() {
        match crate::preset_graph::PresetGraph::load(path).and_then(|g| g.compile()) {
            Ok(graph) => {
                let mut indices = Vec::new();
                for node in &graph.nodes {
                    if node.preset_index < preset_count && !indices.contains(&node.preset_index) {
                        indices.push(node.preset_index);
                    }
                }
                if indices.is_empty() {
                    push_warning(
                        &mut startup_warnings,
                        format!(
                            "preset graph '{}' parsed but contains no in-range preset indices",
                            path
                        ),
                    );
                } else {
                    default_playlist_name = Some("Graph Defaults".to_string());
                    default_playlist_indices = Some(indices);
                    loaded_graph_name = format!("{} nodes", graph.nodes.len());
                    if cfg.preset.is_none() {
                        if let Some(entry) = graph.nodes.get(graph.entry) {
                            if entry.preset_index < preset_count {
                                requested_active = Some(entry.preset_index);
                            }
                        }
                    }
                }
            }
            Err(err) => {
                push_warning(
                    &mut startup_warnings,
                    format!("failed to load preset graph '{}': {err}", path),
                );
            }
        }
    }

    if let Some(path) = cfg.theme_pack.as_deref() {
        match ThemePackManifest::load(path) {
            Ok(pack) => {
                let mut indices = pack
                    .preset_indices
                    .iter()
                    .copied()
                    .filter(|&idx| idx < preset_count)
                    .collect::<Vec<_>>();
                indices.sort_unstable();
                indices.dedup();
                if indices.is_empty() {
                    push_warning(
                        &mut startup_warnings,
                        format!(
                            "theme pack '{}' parsed but contains no in-range preset indices",
                            path
                        ),
                    );
                } else {
                    loaded_theme_name = pack.name.clone();
                    default_playlist_name = Some(format!("Theme: {}", pack.name));
                    default_playlist_indices = Some(indices.clone());
                    intensity = pack.intensity_default.clamp(0.10, 2.5);
                    zoom_drive = pack.zoom_default.clamp(0.12, 8.0);
                    if cfg.preset.is_none() {
                        match requested_active {
                            Some(active_idx) if indices.contains(&active_idx) => {}
                            _ => {
                                requested_active = Some(indices[0]);
                            }
                        }
                    }
                }
            }
            Err(err) => {
                push_warning(
                    &mut startup_warnings,
                    format!("failed to load theme pack '{}': {err}", path),
                );
            }
        }
    }

    if let Some(path) = cfg.control_matrix.as_deref() {
        match ControlMatrix::load(path) {
            Ok(matrix) => {
                let unsupported = matrix
                    .routes()
                    .iter()
                    .filter(|route| !supports_control_name(route.control.as_str()))
                    .count();
                if unsupported > 0 {
                    push_warning(
                        &mut startup_warnings,
                        format!(
                            "control matrix '{}' has {} unsupported route(s); unsupported controls are ignored",
                            path, unsupported
                        ),
                    );
                }
                control_matrix = Some(matrix);
            }
            Err(err) => {
                push_warning(
                    &mut startup_warnings,
                    format!("failed to load control matrix '{}': {err}", path),
                );
            }
        }
    }

    let (theme_options, mut theme_selected) =
        discover_theme_options(cfg.theme_pack.as_deref(), preset_count);
    if loaded_theme_name.is_empty() {
        theme_selected = 0;
    }
    let (graph_options, mut graph_selected) =
        discover_graph_options(cfg.preset_graph.as_deref(), preset_count);
    if loaded_graph_name.is_empty() {
        graph_selected = 0;
    }
    let (lyrics_options, mut lyrics_selected) = discover_lyrics_options(cfg.lyrics_file.as_deref());

    let active = requested_active
        .unwrap_or(0)
        .min(preset_count.saturating_sub(1));

    let mut engine: Box<dyn VisualEngine> = match capability.engine {
        EngineMode::Cpu => Box::new(PresetEngine::new(
            presets,
            active,
            cfg.shuffle,
            cfg.switch,
            cfg.beats_per_switch,
            cfg.seconds_per_switch,
        )),
        EngineMode::Metal => {
            #[cfg(target_os = "macos")]
            {
                match crate::visual::MetalEngine::new(
                    preset_names.clone(),
                    active,
                    cfg.shuffle,
                    cfg.switch,
                    cfg.beats_per_switch,
                    cfg.seconds_per_switch,
                ) {
                    Ok(engine) => Box::new(engine),
                    Err(err) => {
                        capability.record_engine_fallback(
                            EngineMode::Cpu,
                            format!("metal init failed at runtime; falling back to cpu ({err})"),
                        );
                        push_warning(
                            &mut startup_warnings,
                            format!("metal engine unavailable ({err}); using cpu engine"),
                        );
                        Box::new(PresetEngine::new(
                            make_presets(),
                            active,
                            cfg.shuffle,
                            cfg.switch,
                            cfg.beats_per_switch,
                            cfg.seconds_per_switch,
                        ))
                    }
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                capability.record_engine_fallback(
                    EngineMode::Cpu,
                    "metal unsupported on this platform; using cpu engine",
                );
                push_warning(
                    &mut startup_warnings,
                    "metal engine unsupported on this platform; using cpu engine".to_string(),
                );
                Box::new(PresetEngine::new(
                    make_presets(),
                    active,
                    cfg.shuffle,
                    cfg.switch,
                    cfg.beats_per_switch,
                    cfg.seconds_per_switch,
                ))
            }
        }
    };
    engine.set_fractal_zoom_drive(zoom_drive);

    let playlist_store = playlist_storage_path();
    let mut playlists = load_playlists(playlist_store.as_deref(), preset_names.len());
    let mut active_playlist = 0usize;
    if let Some(indices) = default_playlist_indices {
        if !indices.is_empty() {
            playlists.push(Playlist {
                name: default_playlist_name.unwrap_or_else(|| "Loaded Defaults".to_string()),
                preset_indices: indices,
            });
            active_playlist = playlists.len().saturating_sub(1);
        }
    }
    let playlist_ui = PlaylistUi::new();
    let selector_ui = SelectorUi::new();
    engine.set_playlist_indices(&playlists[active_playlist].preset_indices);

    let mut last_size = crossterm::terminal::size().context("get terminal size")?;
    if last_size.1 < 2 || last_size.0 < 4 {
        return Err(anyhow::anyhow!(
            "terminal too small (need at least 4x2, got {}x{})",
            last_size.0,
            last_size.1
        ));
    }

    let show_hud = !stage_mode;
    let mut hud_rows = hud_rows_for_size(last_size, show_hud);

    let mut runtime = RuntimeTuning::new(cfg.quality, cfg.adaptive_quality, stage_mode);
    resize_engine(&mut *engine, last_size, px_w_mul, px_h_mul, hud_rows)?;

    let start = Instant::now();
    let mut last_frame = start;

    let mut fps = FpsCounter::new();
    let mut beat_pulse = 0.0f32;
    let mut last_engine_ms = 0.0f32;
    let mut last_render_ms = 0.0f32;
    let mut last_total_ms = 0.0f32;
    let mut lat_stats = LatencyStats::new();
    let latency_calibration =
        LatencyCalibration::new(cfg.latency_calibration, cfg.latency_offset_ms);
    let typography_mode = TypographyMode::Off;
    let typography_last_non_off = TypographyMode::WordPulse;
    let mut typography_pixels = Vec::<u8>::new();
    let mut lyric_track = if let Some(path) = cfg.lyrics_file.as_deref() {
        match LyricsTrack::load(path) {
            Ok(track) => Some(track),
            Err(err) => {
                push_warning(
                    &mut startup_warnings,
                    format!("failed to load lyrics file '{}': {err}", path),
                );
                None
            }
        }
    } else {
        None
    };
    let mut lyrics_label = lyric_track
        .as_ref()
        .map(|x| format!("{} lines", x.line_count()))
        .unwrap_or_else(|| "none".to_string());
    if let Some(path) = cfg.lyrics_file.as_deref() {
        if let Some((idx, _)) = lyrics_options
            .iter()
            .enumerate()
            .find(|(_, opt)| opt.path.as_deref().and_then(|p| p.to_str()) == Some(path))
        {
            lyrics_selected = idx;
        }
    }
    let lyrics_loop = cfg.lyrics_loop;
    let lyrics_offset_s = cfg.lyrics_offset_ms * 0.001;
    let system_data_mode = cfg.system_data;
    let system_data_feed = if system_data_mode == SystemDataMode::Off {
        None
    } else {
        Some(SystemDataFeed::capture(system_data_mode))
    };
    let mut state = AppState {
        intensity,
        zoom_drive,
        show_hud,
        show_help: false,
        stage_mode,
        playlist_ui,
        selector_ui,
        theme_selected,
        graph_selected,
        lyrics_selected,
        typography_mode,
        typography_last_non_off,
        latency_calibration,
        system_data_mode,
        system_data_feed,
    };

    let mut hud_flash: Option<HudFlash> = None;
    let source_label = format!("{:?}", cfg.source);
    let engine_label = format!("{:?}", capability.engine);
    let stdin_is_tty = std::io::stdin().is_terminal();
    let mut input_enabled = true;
    let mut input_error_streak: u8 = 0;
    if !stdin_is_tty {
        push_warning(
            &mut startup_warnings,
            "stdin is not a TTY; attempting terminal input fallback".to_string(),
        );
    }

    loop {
        let now = Instant::now();

        // Drain input events (non-blocking).
        while input_enabled {
            let has_event = match event::poll(Duration::from_millis(0)) {
                Ok(v) => {
                    input_error_streak = 0;
                    v
                }
                Err(err) => {
                    input_error_streak = input_error_streak.saturating_add(1);
                    if input_error_streak >= 4 {
                        input_enabled = false;
                        push_warning(
                            &mut startup_warnings,
                            format!(
                                "input disabled: failed to initialize input reader repeatedly ({err})"
                            ),
                        );
                    } else {
                        push_warning(
                            &mut startup_warnings,
                            format!(
                                "input warning: initialize input reader failed (will retry) ({err})"
                            ),
                        );
                    }
                    false
                }
            };
            if !has_event {
                break;
            }
            let ev = match event::read() {
                Ok(ev) => {
                    input_error_streak = 0;
                    ev
                }
                Err(err) => {
                    input_error_streak = input_error_streak.saturating_add(1);
                    if input_error_streak >= 4 {
                        input_enabled = false;
                        push_warning(
                            &mut startup_warnings,
                            format!(
                                "input disabled: failed reading terminal events repeatedly ({err})"
                            ),
                        );
                    } else {
                        push_warning(
                            &mut startup_warnings,
                            format!(
                                "input warning: failed reading terminal events (will retry) ({err})"
                            ),
                        );
                    }
                    break;
                }
            };
            match ev {
                Event::Key(k) if k.kind == KeyEventKind::Press || k.kind == KeyEventKind::Repeat => {
                    let old_hud = state.show_hud;
                    let old_stage = state.stage_mode;
                    let key_now = Instant::now();
                    let should_quit = if state.playlist_ui.open {
                        handle_playlist_key(
                            k.code,
                            k.modifiers,
                            &mut *engine,
                            &mut state.playlist_ui,
                            &mut playlists,
                            &mut active_playlist,
                            preset_names.len(),
                            playlist_store.as_deref(),
                        )
                    } else if state.selector_ui.open {
                        match handle_selector_key(
                            k.code,
                            k.modifiers,
                            &mut state.selector_ui,
                            theme_options.len(),
                            graph_options.len(),
                            lyrics_options.len(),
                        ) {
                            SelectorAction::Quit => true,
                            SelectorAction::ApplyTheme(idx) => {
                                state.theme_selected = idx.min(theme_options.len().saturating_sub(1));
                                apply_theme_option(
                                    state.theme_selected,
                                    &theme_options,
                                    &mut *engine,
                                    &mut playlists,
                                    &mut active_playlist,
                                    preset_count,
                                    &mut state.intensity,
                                    &mut state.zoom_drive,
                                    &mut loaded_theme_name,
                                );
                                false
                            }
                            SelectorAction::ApplyGraph(idx) => {
                                state.graph_selected = idx.min(graph_options.len().saturating_sub(1));
                                apply_graph_option(
                                    state.graph_selected,
                                    &graph_options,
                                    &mut *engine,
                                    &mut playlists,
                                    &mut active_playlist,
                                    &mut loaded_graph_name,
                                );
                                false
                            }
                            SelectorAction::ApplyLyrics(idx) => {
                                state.lyrics_selected = idx.min(lyrics_options.len().saturating_sub(1));
                                apply_lyrics_option(
                                    state.lyrics_selected,
                                    &lyrics_options,
                                    &mut lyric_track,
                                    &mut lyrics_label,
                                    &mut startup_warnings,
                                );
                                false
                            }
                            SelectorAction::ApplyTypography(mode) => {
                                state.typography_mode = mode;
                                if state.typography_mode != TypographyMode::Off {
                                    state.typography_last_non_off = state.typography_mode;
                                }
                                false
                            }
                            SelectorAction::None => false,
                        }
                    } else {
                        state.handle_key(k.code, k.modifiers, k.kind, &mut *engine)
                    };
                    if should_quit {
                        return Ok(());
                    }
                    if !state.playlist_ui.open && !state.selector_ui.open {
                        if let Some(key) = hotkey_highlight_key(k.code) {
                            hud_flash = Some(HudFlash::new(key_now, key));
                        }
                    }
                    if state.playlist_ui.open || state.selector_ui.open {
                        state.show_help = false;
                    }

                    if old_stage != state.stage_mode {
                        runtime.set_stage_mode(state.stage_mode);
                        app_prefs.stage_mode = state.stage_mode;
                        if let Err(err) = app_prefs.save(prefs_store.as_deref()) {
                            push_warning(
                                &mut startup_warnings,
                                format!("prefs save failed (stage_mode not persisted): {err}"),
                            );
                        }
                    }
                    if state.show_hud != old_hud || old_stage != state.stage_mode {
                        hud_rows = hud_rows_for_size(last_size, state.show_hud);
                        resize_engine(&mut *engine, last_size, px_w_mul, px_h_mul, hud_rows)?;
                    }
                }
                Event::Resize(c, r) => {
                    last_size = (c, r);
                    hud_rows = hud_rows_for_size(last_size, state.show_hud);
                    resize_engine(&mut *engine, last_size, px_w_mul, px_h_mul, hud_rows)?;
                }
                _ => {}
            }
        }

        // Size check once per frame (resize events can be missed in some terminals).
        let sz = crossterm::terminal::size()?;
        if sz != last_size {
            last_size = sz;
            hud_rows = hud_rows_for_size(last_size, state.show_hud);
            resize_engine(&mut *engine, last_size, px_w_mul, px_h_mul, hud_rows)?;
        }

        let dt = now.duration_since(last_frame).as_secs_f32().max(1e-6);
        last_frame = now;
        let t = now.duration_since(start).as_secs_f32();

        let raw_audio = audio_features.load();
        let audio_age_ms = audio_features.age_ms();
        let corrected_audio = state.latency_calibration.apply_phase_correction(raw_audio, dt);

        if let Some(matrix) = control_matrix.as_ref() {
            let controls = matrix.evaluate(&corrected_audio, &mut control_runtime.state);
            apply_control_routes(
                &controls,
                &mut *engine,
                &mut state.intensity,
                &mut state.zoom_drive,
                &mut state.typography_mode,
            );
            if state.typography_mode != TypographyMode::Off {
                state.typography_last_non_off = state.typography_mode;
            }
        }

        if corrected_audio.beat {
            beat_pulse = (beat_pulse + 0.65 + corrected_audio.beat_strength * 0.7).min(1.6);
        }
        // Exponential decay; tuned for hypnotic "breathing" rather than a hard flash.
        beat_pulse *= (0.1f32).powf(dt);

        engine.update_auto_switch(now, &corrected_audio);

        let (term_cols, term_rows) = last_size;
        let preset_name = engine.preset_name().to_string();
        let switch_mode = engine.switch_mode();
        let auto_switch = engine.auto_switch();
        let shuffle = engine.shuffle();
        let transition_mode = engine.transition_mode();
        let transition_kind = engine.transition_kind_name();
        let transition_selection = engine.transition_selection_name();
        let transition_locked = engine.transition_selection_locked();
        let active_playlist_name = playlists
            .get(active_playlist)
            .map(|p| p.name.as_str())
            .unwrap_or("All Presets");
        let active_playlist_count = playlists
            .get(active_playlist)
            .map(|p| p.preset_indices.len())
            .unwrap_or(0);
        let zoom_mode = format!("{:?}", engine.fractal_zoom_mode());
        let zoom_enabled = engine.fractal_zoom_enabled();
        let fractal_bias = engine.fractal_bias();
        let scene_section = engine.scene_section_name();
        let camera_mode = engine.camera_path_mode_name();
        let camera_speed = engine.camera_path_speed();
        let renderer_name = renderer.name();
        let (lat_now, lat_avg, lat_p95) = lat_stats.snapshot();
        let probe_status = capability.status_label();
        let latency_status = state.latency_calibration.status_label();
        let lyric_line = lyric_track
            .as_ref()
            .and_then(|track| track.current_line(t + lyrics_offset_s, lyrics_loop));
        let system_token = state.system_data_feed
            .as_ref()
            .and_then(|feed| feed.token_at(t, &corrected_audio, beat_pulse));
        let typography_text = typography_overlay_text(
            state.typography_mode,
            &corrected_audio,
            beat_pulse,
            t,
            lyric_line,
            system_token,
        );
        let theme_label = if loaded_theme_name.is_empty() {
            "none"
        } else {
            loaded_theme_name.as_str()
        };
        let graph_label = if loaded_graph_name.is_empty() {
            "none"
        } else {
            loaded_graph_name.as_str()
        };
        let system_data_label = state.system_data_feed
            .as_ref()
            .map(|x| x.label())
            .unwrap_or(system_data_mode_label(state.system_data_mode));
        let warning_status = latest_warning(&startup_warnings);
        if hud_flash.as_ref().is_some_and(|f| !f.active(now)) {
            hud_flash = None;
        }
        let hud_highlight = hud_flash.as_ref().map(|f| f.key);
        let hud_highlight_phase = hud_flash
            .as_ref()
            .map(|f| f.blink_phase(now))
            .unwrap_or(false);

        let hud = if state.show_hud {
                build_wrapped_hud(
                    term_cols as usize,
                    &preset_name,
                    &format!("{:?}{}", switch_mode, if auto_switch { " (auto)" } else { "" }),
                    shuffle,
                transition_mode.label(),
                transition_selection,
                transition_locked,
                transition_kind,
                active_playlist_name,
                active_playlist_count,
                    state.intensity,
                &zoom_mode,
                state.zoom_drive,
                    zoom_enabled,
                    fractal_bias,
                    state.stage_mode,
                    scene_section,
                    camera_mode,
                    camera_speed,
                    state.typography_mode.label(),
                    typography_text.as_deref().unwrap_or("-"),
                    &latency_status,
                    &probe_status,
                    theme_label,
                    graph_label,
                    &lyrics_label,
                    system_data_label,
                    warning_status,
                    if state.show_help { "on" } else { "off" },
                    fps.fps(),
                lat_now,
                lat_avg,
                lat_p95,
                last_engine_ms,
                last_render_ms,
                last_total_ms,
                &source_label,
                &engine_label,
                renderer_name,
            )
        } else {
            String::new()
        };

        let target_hud_rows = hud_rows_for_text(term_rows, state.show_hud, &hud);
        if target_hud_rows != hud_rows {
            hud_rows = target_hud_rows;
            resize_engine(&mut *engine, last_size, px_w_mul, px_h_mul, hud_rows)?;
        }
        let visual_rows = term_rows.saturating_sub(hud_rows).max(1);
        let w = (term_cols as usize).saturating_mul(px_w_mul);
        let h = (visual_rows as usize).saturating_mul(px_h_mul);

        let (typo_audio, typo_intensity_mul) = typography_reactive_audio(
            state.typography_mode,
            corrected_audio,
            beat_pulse,
            t,
        );
        let audio = apply_intensity(typo_audio, (state.intensity * typo_intensity_mul).clamp(0.10, 2.5));

        let ctx = RenderCtx {
            now,
            t,
            dt,
            w,
            h,
            audio,
            beat_pulse: (beat_pulse * state.intensity).clamp(0.0, 1.0),
            fractal_zoom_mul: 1.0,
            safe: cfg.safe,
            quality: runtime.quality,
            scale: runtime.scale,
        };

        let engine_start = Instant::now();
        let pixels = engine.render(ctx, runtime.quality, runtime.scale);
        let pixels_rgba = if state.typography_mode == TypographyMode::Off {
            pixels
        } else {
            if typography_pixels.len() != pixels.len() {
                typography_pixels.resize(pixels.len(), 0);
            }
            typography_pixels.copy_from_slice(pixels);
            apply_typography_overlay_pixels(
                state.typography_mode,
                &mut typography_pixels,
                w,
                h,
                &audio,
                beat_pulse,
                t,
                lyric_line,
                system_token,
            );
            typography_pixels.as_slice()
        };
        let engine_ms = engine_start.elapsed().as_secs_f32() * 1000.0;
        last_engine_ms = engine_ms;

        let playlist_overlay = if !state.stage_mode && state.playlist_ui.open {
            Some(build_playlist_popup(
                term_cols,
                term_rows,
                &playlists,
                active_playlist,
                &state.playlist_ui,
                &preset_names,
            ))
        } else {
            None
        };
        let selector_overlay = if !state.stage_mode && state.selector_ui.open {
            Some(build_selector_popup(
                term_cols,
                term_rows,
                &state.selector_ui,
                &theme_options,
                state.theme_selected,
                &graph_options,
                state.graph_selected,
                &lyrics_options,
                state.lyrics_selected,
                state.typography_mode,
            ))
        } else {
            None
        };
        let help_overlay = if !state.stage_mode && state.show_help {
            Some(help_popup_text(
                &probe_status,
                &latency_status,
                state.typography_mode,
                warning_status,
            ))
        } else {
            None
        };
        let overlay = if let Some(ref text) = playlist_overlay {
            Some(text.as_str())
        } else if let Some(ref text) = selector_overlay {
            Some(text.as_str())
        } else if let Some(ref text) = help_overlay {
            Some(text.as_str())
        } else {
            None
        };

        let frame = Frame {
            term_cols,
            term_rows,
            visual_rows,
            pixel_width: w,
            pixel_height: h,
            pixels_rgba,
            hud: &hud,
            hud_rows,
            hud_highlight,
            hud_highlight_phase,
            overlay,
            sync_updates: cfg.sync_updates,
        };

        let frame_start = Instant::now();
        renderer.render(&frame, &mut out)?;
        let render_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
        let total_ms = now.elapsed().as_secs_f32() * 1000.0;
        let end_to_end_latency_ms = audio_age_ms + total_ms;
        lat_stats.push(end_to_end_latency_ms);
        state.latency_calibration.observe_latency(end_to_end_latency_ms);

        fps.tick();
        runtime.update(total_ms, 1000.0 / cfg.fps as f32);
        last_render_ms = render_ms;
        last_total_ms = total_ms;

        // Frame pacing.
        let target = Duration::from_secs_f32(1.0 / cfg.fps.max(1) as f32);
        let elapsed = now.elapsed();
        if elapsed < target {
            std::thread::sleep(target - elapsed);
        }
    }
}

fn resize_engine(
    engine: &mut dyn VisualEngine,
    size: (u16, u16),
    px_w_mul: usize,
    px_h_mul: usize,
    hud_rows: u16,
) -> anyhow::Result<()> {
    let (cols, rows) = size;
    let visual_rows = rows.saturating_sub(hud_rows).max(1);
    let w = (cols as usize).saturating_mul(px_w_mul);
    let h = (visual_rows as usize).saturating_mul(px_h_mul);
    engine.resize(w, h);
    Ok(())
}

fn select_preset(preset: &Option<String>, presets: &[Box<dyn crate::visual::Preset>]) -> Option<usize> {
    let p = preset.as_deref()?.trim();
    if p.is_empty() {
        return None;
    }
    if let Ok(i) = p.parse::<usize>() {
        return (i < presets.len()).then_some(i);
    }
    let p_l = p.to_lowercase();
    presets
        .iter()
        .position(|x| x.name().to_lowercase().contains(&p_l))
}

impl AppState {
    fn handle_key(
        &mut self,
        code: KeyCode,
        mods: KeyModifiers,
        kind: KeyEventKind,
        engine: &mut dyn VisualEngine,
    ) -> bool {
        if mods.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c')) {
            return true;
        }

        let is_repeat = kind == KeyEventKind::Repeat;

        match code {
            KeyCode::Esc => true,
            KeyCode::Char('q') | KeyCode::Char('Q') => true,
            KeyCode::Up => {
                self.intensity = (self.intensity + 0.05).min(2.5);
                false
            }
            KeyCode::Down => {
                self.intensity = (self.intensity - 0.05).max(0.10);
                false
            }
            KeyCode::Left => {
                engine.prev_preset();
                false
            }
            KeyCode::Right => {
                engine.next_preset();
                false
            }
            KeyCode::Char(' ') if !is_repeat => {
                engine.toggle_auto_switch();
                false
            }
            KeyCode::Char('i') | KeyCode::Char('I') if !is_repeat => {
                if self.stage_mode {
                    return false;
                }
                self.show_hud = !self.show_hud;
                false
            }
            KeyCode::Char('g') | KeyCode::Char('G') if !is_repeat => {
                self.stage_mode = !self.stage_mode;
                if self.stage_mode {
                    self.show_hud = false;
                    self.show_help = false;
                    self.playlist_ui.open = false;
                    self.selector_ui.close();
                } else {
                    self.show_hud = true;
                }
                false
            }
            KeyCode::Char('?')
            | KeyCode::Char('/')
            | KeyCode::Char('h')
            | KeyCode::Char('H')
            | KeyCode::F(1)
            | KeyCode::Tab if !is_repeat => {
                if self.stage_mode {
                    self.stage_mode = false;
                    self.show_hud = true;
                    self.show_help = true;
                    self.playlist_ui.open = false;
                    self.selector_ui.close();
                    return false;
                }
                self.show_help = !self.show_help;
                if self.show_help {
                    self.playlist_ui.open = false;
                    self.selector_ui.close();
                    self.show_hud = true;
                }
                false
            }
            KeyCode::Char('p') | KeyCode::Char('P') if !is_repeat => {
                if self.stage_mode {
                    self.stage_mode = false;
                    self.show_hud = true;
                    self.playlist_ui.open = true;
                    self.show_help = false;
                    self.selector_ui.close();
                    return false;
                }
                self.playlist_ui.open = !self.playlist_ui.open;
                if self.playlist_ui.open {
                    self.show_help = false;
                    self.selector_ui.close();
                    self.show_hud = true;
                }
                false
            }
            KeyCode::Char('m') | KeyCode::Char('M') => {
                if self.stage_mode {
                    self.stage_mode = false;
                }
                self.show_hud = true;
                self.show_help = false;
                self.playlist_ui.open = false;
                self.selector_ui.open(SelectorKind::Theme, self.theme_selected);
                false
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                if self.stage_mode {
                    self.stage_mode = false;
                }
                self.show_hud = true;
                self.show_help = false;
                self.playlist_ui.open = false;
                self.selector_ui.open(SelectorKind::Graph, self.graph_selected);
                false
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                if self.stage_mode {
                    self.stage_mode = false;
                }
                self.show_hud = true;
                self.show_help = false;
                self.playlist_ui.open = false;
                self.selector_ui.open(SelectorKind::Typography, self.typography_mode.index());
                false
            }
            KeyCode::Char('k') | KeyCode::Char('K') => {
                if self.stage_mode {
                    self.stage_mode = false;
                }
                self.show_hud = true;
                self.show_help = false;
                self.playlist_ui.open = false;
                self.selector_ui.open(SelectorKind::Lyrics, self.lyrics_selected);
                false
            }
            KeyCode::Char('s') | KeyCode::Char('S') if !is_repeat => {
                engine.toggle_shuffle();
                false
            }
            KeyCode::Char(';') | KeyCode::Char(':') => {
                self.system_data_mode = cycle_system_data_mode(self.system_data_mode);
                self.system_data_feed = if self.system_data_mode == SystemDataMode::Off {
                    None
                } else {
                    Some(SystemDataFeed::capture(self.system_data_mode))
                };
                false
            }
            KeyCode::Char('t') | KeyCode::Char('T') if !is_repeat => {
                engine.cycle_transition_mode();
                false
            }
            KeyCode::Char(']') => {
                engine.next_transition_kind();
                false
            }
            KeyCode::Char('[') => {
                engine.prev_transition_kind();
                false
            }
            KeyCode::Char('f') | KeyCode::Char('F') if !is_repeat => {
                engine.toggle_fractal_bias();
                false
            }
            KeyCode::Char('z') | KeyCode::Char('Z') => {
                engine.cycle_fractal_zoom_mode();
                false
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                engine.cycle_camera_path_mode();
                false
            }
            KeyCode::Char('.') => {
                engine.step_camera_path_speed(0.10);
                false
            }
            KeyCode::Char(',') => {
                engine.step_camera_path_speed(-0.10);
                false
            }
            KeyCode::Char('x') => {
                self.zoom_drive = (self.zoom_drive * 1.25).clamp(0.12, 8.0);
                engine.set_fractal_zoom_drive(self.zoom_drive);
                false
            }
            KeyCode::Char('X') => {
                self.zoom_drive = (self.zoom_drive / 1.25).clamp(0.12, 8.0);
                engine.set_fractal_zoom_drive(self.zoom_drive);
                false
            }
            KeyCode::Char('v') | KeyCode::Char('V') if !is_repeat => {
                engine.toggle_fractal_zoom_enabled();
                false
            }
            KeyCode::Char('l') | KeyCode::Char('L') if !is_repeat => {
                self.latency_calibration.toggle();
                false
            }
            KeyCode::Char('-') => {
                self.latency_calibration.nudge_manual_offset(-5.0);
                false
            }
            KeyCode::Char('=') | KeyCode::Char('+') => {
                self.latency_calibration.nudge_manual_offset(5.0);
                false
            }
            KeyCode::Char('0') => {
                self.latency_calibration.reset_manual_offset();
                false
            }
            KeyCode::Char('y') | KeyCode::Char('Y') if !is_repeat => {
                let cycle = mods.contains(KeyModifiers::SHIFT) || matches!(code, KeyCode::Char('Y'));
                if cycle {
                    let next = if self.typography_mode == TypographyMode::Off {
                        if self.typography_last_non_off == TypographyMode::Off {
                            TypographyMode::LinePulse
                        } else {
                            self.typography_last_non_off.cycle_non_off()
                        }
                    } else {
                        self.typography_mode.cycle_non_off()
                    };
                    self.typography_mode = next;
                    self.typography_last_non_off = next;
                } else if self.typography_mode == TypographyMode::Off {
                    self.typography_mode = if self.typography_last_non_off == TypographyMode::Off {
                        TypographyMode::LinePulse
                    } else {
                        self.typography_last_non_off
                    };
                } else {
                    self.typography_last_non_off = self.typography_mode;
                    self.typography_mode = TypographyMode::Off;
                }
                false
            }
            KeyCode::Char('1') => {
                engine.set_switch_mode(SwitchMode::Manual);
                false
            }
            KeyCode::Char('2') => {
                engine.set_switch_mode(SwitchMode::Beat);
                false
            }
            KeyCode::Char('3') => {
                engine.set_switch_mode(SwitchMode::Energy);
                false
            }
            KeyCode::Char('4') => {
                engine.set_switch_mode(SwitchMode::Time);
                false
            }
            KeyCode::Char('5') => {
                engine.set_switch_mode(SwitchMode::Adaptive);
                false
            }
            _ => false,
        }
    }
}

fn selector_options_len(
    kind: SelectorKind,
    theme_len: usize,
    graph_len: usize,
    lyrics_len: usize,
) -> usize {
    match kind {
        SelectorKind::Theme => theme_len,
        SelectorKind::Graph => graph_len,
        SelectorKind::Lyrics => lyrics_len,
        SelectorKind::Typography => TypographyMode::all().len(),
    }
    .max(1)
}

fn handle_selector_key(
    code: KeyCode,
    mods: KeyModifiers,
    ui: &mut SelectorUi,
    theme_len: usize,
    graph_len: usize,
    lyrics_len: usize,
) -> SelectorAction {
    if mods.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c')) {
        return SelectorAction::Quit;
    }

    let clamp_cursor = |ui: &mut SelectorUi| {
        let len = selector_options_len(ui.kind, theme_len, graph_len, lyrics_len);
        ui.cursor = ui.cursor.min(len.saturating_sub(1));
    };
    clamp_cursor(ui);

    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') => SelectorAction::Quit,
        KeyCode::Esc => {
            ui.close();
            SelectorAction::None
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            ui.close();
            SelectorAction::None
        }
        KeyCode::Char('m') | KeyCode::Char('M') => {
            ui.kind = SelectorKind::Theme;
            clamp_cursor(ui);
            SelectorAction::None
        }
        KeyCode::Char('o') | KeyCode::Char('O') => {
            ui.kind = SelectorKind::Graph;
            clamp_cursor(ui);
            SelectorAction::None
        }
        KeyCode::Char('u') | KeyCode::Char('U') => {
            ui.kind = SelectorKind::Typography;
            clamp_cursor(ui);
            SelectorAction::None
        }
        KeyCode::Char('k') | KeyCode::Char('K') => {
            ui.kind = SelectorKind::Lyrics;
            clamp_cursor(ui);
            SelectorAction::None
        }
        KeyCode::Tab | KeyCode::Right => {
            ui.kind = match ui.kind {
                SelectorKind::Theme => SelectorKind::Graph,
                SelectorKind::Graph => SelectorKind::Lyrics,
                SelectorKind::Lyrics => SelectorKind::Typography,
                SelectorKind::Typography => SelectorKind::Theme,
            };
            clamp_cursor(ui);
            SelectorAction::None
        }
        KeyCode::Left => {
            ui.kind = match ui.kind {
                SelectorKind::Theme => SelectorKind::Typography,
                SelectorKind::Graph => SelectorKind::Theme,
                SelectorKind::Lyrics => SelectorKind::Graph,
                SelectorKind::Typography => SelectorKind::Lyrics,
            };
            clamp_cursor(ui);
            SelectorAction::None
        }
        KeyCode::Up => {
            ui.cursor = ui.cursor.saturating_sub(1);
            SelectorAction::None
        }
        KeyCode::Down => {
            let len = selector_options_len(ui.kind, theme_len, graph_len, lyrics_len);
            ui.cursor = (ui.cursor + 1).min(len.saturating_sub(1));
            SelectorAction::None
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            ui.close();
            match ui.kind {
                SelectorKind::Theme => SelectorAction::ApplyTheme(ui.cursor),
                SelectorKind::Graph => SelectorAction::ApplyGraph(ui.cursor),
                SelectorKind::Lyrics => SelectorAction::ApplyLyrics(ui.cursor),
                SelectorKind::Typography => {
                    SelectorAction::ApplyTypography(TypographyMode::from_index(ui.cursor))
                }
            }
        }
        _ => SelectorAction::None,
    }
}

fn handle_playlist_key(
    code: KeyCode,
    mods: KeyModifiers,
    engine: &mut dyn VisualEngine,
    ui: &mut PlaylistUi,
    playlists: &mut Vec<Playlist>,
    active_playlist: &mut usize,
    preset_count: usize,
    playlist_store: Option<&Path>,
) -> bool {
    if mods.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c')) {
        return true;
    }
    if playlists.is_empty() {
        return false;
    }

    ui.playlist_cursor = ui.playlist_cursor.min(playlists.len().saturating_sub(1));
    ui.preset_cursor = ui.preset_cursor.min(preset_count.saturating_sub(1));

    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') => true,
        KeyCode::Esc | KeyCode::Char('p') | KeyCode::Char('P') => {
            ui.open = false;
            false
        }
        KeyCode::Tab | KeyCode::Left | KeyCode::Right => {
            ui.focus = match ui.focus {
                PlaylistFocus::Playlists => PlaylistFocus::Presets,
                PlaylistFocus::Presets => PlaylistFocus::Playlists,
            };
            false
        }
        KeyCode::Up => {
            match ui.focus {
                PlaylistFocus::Playlists => {
                    ui.playlist_cursor = ui.playlist_cursor.saturating_sub(1);
                }
                PlaylistFocus::Presets => {
                    ui.preset_cursor = ui.preset_cursor.saturating_sub(1);
                }
            }
            false
        }
        KeyCode::Down => {
            match ui.focus {
                PlaylistFocus::Playlists => {
                    ui.playlist_cursor =
                        (ui.playlist_cursor + 1).min(playlists.len().saturating_sub(1));
                }
                PlaylistFocus::Presets => {
                    if preset_count > 0 {
                        ui.preset_cursor = (ui.preset_cursor + 1).min(preset_count - 1);
                    }
                }
            }
            false
        }
        KeyCode::Enter => {
            match ui.focus {
                PlaylistFocus::Playlists => {
                    *active_playlist = ui.playlist_cursor.min(playlists.len().saturating_sub(1));
                    engine.set_playlist_indices(&playlists[*active_playlist].preset_indices);
                }
                PlaylistFocus::Presets => {
                    toggle_playlist_preset(playlists, ui.playlist_cursor, ui.preset_cursor);
                    if *active_playlist == ui.playlist_cursor {
                        engine.set_playlist_indices(&playlists[*active_playlist].preset_indices);
                    }
                    save_playlists(playlists, playlist_store);
                }
            }
            false
        }
        KeyCode::Char(' ') => {
            toggle_playlist_preset(playlists, ui.playlist_cursor, ui.preset_cursor);
            if *active_playlist == ui.playlist_cursor {
                engine.set_playlist_indices(&playlists[*active_playlist].preset_indices);
            }
            save_playlists(playlists, playlist_store);
            false
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            add_playlist_preset(playlists, ui.playlist_cursor, ui.preset_cursor);
            if *active_playlist == ui.playlist_cursor {
                engine.set_playlist_indices(&playlists[*active_playlist].preset_indices);
            }
            save_playlists(playlists, playlist_store);
            false
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            remove_playlist_preset(playlists, ui.playlist_cursor, ui.preset_cursor);
            if *active_playlist == ui.playlist_cursor {
                engine.set_playlist_indices(&playlists[*active_playlist].preset_indices);
            }
            save_playlists(playlists, playlist_store);
            false
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            let next_num = playlists.len();
            let seed = playlists
                .get(*active_playlist)
                .map(|p| p.preset_indices.clone())
                .unwrap_or_else(|| (0..preset_count).collect());
            let mut new_indices = if seed.is_empty() {
                vec![ui.preset_cursor.min(preset_count.saturating_sub(1))]
            } else {
                seed
            };
            new_indices.sort_unstable();
            new_indices.dedup();
            playlists.push(Playlist {
                name: format!("Playlist {}", next_num),
                preset_indices: new_indices,
            });
            ui.playlist_cursor = playlists.len() - 1;
            *active_playlist = ui.playlist_cursor;
            engine.set_playlist_indices(&playlists[*active_playlist].preset_indices);
            save_playlists(playlists, playlist_store);
            false
        }
        KeyCode::Char('x') | KeyCode::Char('X') | KeyCode::Char('d') | KeyCode::Char('D') => {
            // Keep All Presets immutable at index 0.
            if ui.playlist_cursor > 0 && playlists.len() > 1 {
                let removed_idx = ui.playlist_cursor;
                playlists.remove(removed_idx);
                if *active_playlist == removed_idx {
                    *active_playlist = 0;
                    engine.set_playlist_indices(&playlists[*active_playlist].preset_indices);
                } else if *active_playlist > removed_idx {
                    *active_playlist -= 1;
                }
                ui.playlist_cursor = ui.playlist_cursor.min(playlists.len().saturating_sub(1));
                save_playlists(playlists, playlist_store);
            }
            false
        }
        _ => false,
    }
}

fn toggle_playlist_preset(playlists: &mut [Playlist], playlist_idx: usize, preset_idx: usize) {
    if playlist_idx == 0 {
        return;
    }
    let Some(pl) = playlists.get_mut(playlist_idx) else {
        return;
    };
    if let Some(pos) = pl.preset_indices.iter().position(|&i| i == preset_idx) {
        if pl.preset_indices.len() > 1 {
            pl.preset_indices.remove(pos);
        }
    } else {
        pl.preset_indices.push(preset_idx);
        pl.preset_indices.sort_unstable();
        pl.preset_indices.dedup();
    }
}

fn add_playlist_preset(playlists: &mut [Playlist], playlist_idx: usize, preset_idx: usize) {
    if playlist_idx == 0 {
        return;
    }
    let Some(pl) = playlists.get_mut(playlist_idx) else {
        return;
    };
    if !pl.preset_indices.contains(&preset_idx) {
        pl.preset_indices.push(preset_idx);
        pl.preset_indices.sort_unstable();
        pl.preset_indices.dedup();
    }
}

fn remove_playlist_preset(playlists: &mut [Playlist], playlist_idx: usize, preset_idx: usize) {
    if playlist_idx == 0 {
        return;
    }
    let Some(pl) = playlists.get_mut(playlist_idx) else {
        return;
    };
    if pl.preset_indices.len() <= 1 {
        return;
    }
    if let Some(pos) = pl.preset_indices.iter().position(|&i| i == preset_idx) {
        pl.preset_indices.remove(pos);
    }
}

fn build_playlist_popup(
    term_cols: u16,
    term_rows: u16,
    playlists: &[Playlist],
    active_playlist: usize,
    ui: &PlaylistUi,
    preset_names: &[&'static str],
) -> String {
    let cols = term_cols as usize;
    let rows = term_rows as usize;
    let body_rows = rows.saturating_sub(11).clamp(6, 24);
    let left_w = cols.saturating_mul(42) / 100;
    let left_w = left_w.clamp(18, 44);
    let right_w = cols.saturating_sub(left_w + 7).max(18);

    let pl_cursor = ui.playlist_cursor.min(playlists.len().saturating_sub(1));
    let pr_cursor = ui.preset_cursor.min(preset_names.len().saturating_sub(1));
    let pl_start = centered_window_start(pl_cursor, playlists.len(), body_rows);
    let pr_start = centered_window_start(pr_cursor, preset_names.len(), body_rows);
    let selected_playlist = playlists
        .get(pl_cursor)
        .or_else(|| playlists.first());

    let mut lines = Vec::new();
    lines.push("Playlist Manager".to_string());
    lines.push(format!(
        "Focus: {:?} | Active: {} | Presets: {}",
        ui.focus,
        playlists
            .get(active_playlist)
            .map(|p| p.name.as_str())
            .unwrap_or("All Presets"),
        selected_playlist
            .map(|p| p.preset_indices.len())
            .unwrap_or(0)
    ));
    lines.push("Keys: tab switch pane | up/down move | enter apply/toggle | space toggle | n new | x delete | a add | r remove | p/esc close".to_string());
    lines.push(format!(
        "{:<left_w$} | {}",
        "Playlists",
        "Preset Membership",
        left_w = left_w
    ));

    for row in 0..body_rows {
        let left_line = if let Some(pl) = playlists.get(pl_start + row) {
            let cursor = if ui.focus == PlaylistFocus::Playlists && pl_start + row == pl_cursor {
                '>'
            } else {
                ' '
            };
            let active = if pl_start + row == active_playlist {
                '*'
            } else {
                ' '
            };
            let name_w = left_w.saturating_sub(8);
            let nm = truncate_for_width(&pl.name, name_w);
            format!("{cursor}{active} {nm:<name_w$} ({:>2})", pl.preset_indices.len())
        } else {
            " ".repeat(left_w)
        };

        let right_line = if let Some(name) = preset_names.get(pr_start + row) {
            let idx = pr_start + row;
            let cursor = if ui.focus == PlaylistFocus::Presets && idx == pr_cursor {
                '>'
            } else {
                ' '
            };
            let in_pl = selected_playlist
                .map(|pl| pl.preset_indices.contains(&idx))
                .unwrap_or(false);
            let check = if in_pl { 'x' } else { ' ' };
            let nm = truncate_for_width(name, right_w.saturating_sub(5));
            format!("{cursor}[{check}] {nm}")
        } else {
            String::new()
        };

        lines.push(format!(
            "{:<left_w$} | {}",
            truncate_for_width(&left_line, left_w),
            truncate_for_width(&right_line, right_w),
            left_w = left_w
        ));
    }

    lines.push("Tips: Playlist 0 is immutable (All Presets). Select a playlist and press Enter to make it active.".to_string());
    lines.join("\n")
}

fn build_selector_popup(
    term_cols: u16,
    term_rows: u16,
    ui: &SelectorUi,
    theme_options: &[ThemeOption],
    theme_selected: usize,
    graph_options: &[GraphOption],
    graph_selected: usize,
    lyric_options: &[LyricOption],
    lyric_selected: usize,
    typography_mode: TypographyMode,
) -> String {
    let cols = term_cols as usize;
    let rows = term_rows as usize;
    let body_rows = rows.saturating_sub(10).clamp(6, 20);
    let width = cols.saturating_sub(6).max(20);

    let (entries, selected, active_label) = match ui.kind {
        SelectorKind::Theme => (
            theme_options
                .iter()
                .map(|x| x.label.clone())
                .collect::<Vec<_>>(),
            theme_selected.min(theme_options.len().saturating_sub(1)),
            theme_options
                .get(theme_selected)
                .map(|x| x.label.as_str())
                .unwrap_or("none"),
        ),
        SelectorKind::Graph => (
            graph_options
                .iter()
                .map(|x| x.label.clone())
                .collect::<Vec<_>>(),
            graph_selected.min(graph_options.len().saturating_sub(1)),
            graph_options
                .get(graph_selected)
                .map(|x| x.label.as_str())
                .unwrap_or("none"),
        ),
        SelectorKind::Lyrics => (
            lyric_options
                .iter()
                .map(|x| x.label.clone())
                .collect::<Vec<_>>(),
            lyric_selected.min(lyric_options.len().saturating_sub(1)),
            lyric_options
                .get(lyric_selected)
                .map(|x| x.label.as_str())
                .unwrap_or("none"),
        ),
        SelectorKind::Typography => (
            TypographyMode::all()
                .iter()
                .map(|mode| {
                    format!(
                        "{} - {} [exp]",
                        mode.label(),
                        typography_mode_description(*mode)
                    )
                })
                .collect::<Vec<_>>(),
            typography_mode.index(),
            typography_mode.label(),
        ),
    };

    let cursor = ui.cursor.min(entries.len().saturating_sub(1));
    let start = centered_window_start(cursor, entries.len(), body_rows);

    let mut lines = Vec::new();
    lines.push(format!("{} selector", ui.kind.label()));
    lines.push(format!(
        "Active: {} | Selectors: m theme | o graph | k lyrics | u typography | tab/←/→ switch",
        active_label
    ));
    lines.push("Keys: up/down move | enter/space apply | esc close".to_string());

    for row in 0..body_rows {
        if let Some(entry) = entries.get(start + row) {
            let cur = if start + row == cursor { '>' } else { ' ' };
            let act = if start + row == selected { '*' } else { ' ' };
            lines.push(format!(
                "{cur}{act} {}",
                truncate_for_width(entry, width.saturating_sub(3))
            ));
        } else {
            lines.push(String::new());
        }
    }

    if ui.kind == SelectorKind::Typography {
        lines.push(
            "Tip: y toggles typography on/off. Shift+y cycles style without turning it off."
                .to_string(),
        );
    } else if ui.kind == SelectorKind::Lyrics {
        lines.push(
            "Tip: put .lrc/.txt files in assets/samples or ~/.config/tui_visualizer/lyrics/."
                .to_string(),
        );
    } else {
        lines.push(
            "Tip: place custom files in assets/theme or assets/graph (or ~/.config/tui_visualizer/)."
                .to_string(),
        );
    }
    lines.join("\n")
}

fn typography_mode_description(mode: TypographyMode) -> &'static str {
    match mode {
        TypographyMode::Off => "disabled",
        TypographyMode::LinePulse => "meter pulse + subtle mid lift",
        TypographyMode::WordPulse => "phrase cadence + beat accent",
        TypographyMode::GlyphFlow => "high-mid shimmer + centroid sweep",
        TypographyMode::MatrixPulse => "gated strobe + transient spikes",
    }
}

fn discover_theme_options(explicit_path: Option<&str>, preset_count: usize) -> (Vec<ThemeOption>, usize) {
    let mut options = vec![ThemeOption {
        label: format!("none (all presets, {} entries)", preset_count),
        pack: None,
        preset_indices: (0..preset_count).collect(),
    }];
    let mut selected = 0usize;
    let explicit_key = explicit_path
        .filter(|s| !s.trim().is_empty())
        .map(|s| path_identity(Path::new(s)));
    let mut seen = HashSet::new();

    for path in candidate_config_files(explicit_path, Path::new("assets/theme"), "theme", "theme") {
        let key = path_identity(path.as_path());
        if !seen.insert(key.clone()) {
            continue;
        }

        let Ok(pack) = ThemePackManifest::load(&path) else {
            continue;
        };
        let mut indices = pack
            .preset_indices
            .iter()
            .copied()
            .filter(|&idx| idx < preset_count)
            .collect::<Vec<_>>();
        indices.sort_unstable();
        indices.dedup();
        if indices.is_empty() {
            continue;
        }

        if explicit_key.as_ref().is_some_and(|k| k == &key) {
            selected = options.len();
        }

        options.push(ThemeOption {
            label: format!("{} [{} presets]", pack.name, indices.len()),
            pack: Some(pack),
            preset_indices: indices,
        });
    }

    let selected = selected.min(options.len().saturating_sub(1));
    (options, selected)
}

fn discover_graph_options(explicit_path: Option<&str>, preset_count: usize) -> (Vec<GraphOption>, usize) {
    let mut options = vec![GraphOption {
        label: "none (disabled)".to_string(),
        preset_indices: Vec::new(),
        entry_preset: None,
    }];
    let mut selected = 0usize;
    let explicit_key = explicit_path
        .filter(|s| !s.trim().is_empty())
        .map(|s| path_identity(Path::new(s)));
    let mut seen = HashSet::new();

    for path in candidate_config_files(explicit_path, Path::new("assets/graph"), "graph", "graph") {
        let key = path_identity(path.as_path());
        if !seen.insert(key.clone()) {
            continue;
        }

        let Ok(graph) = crate::preset_graph::PresetGraph::load(&path).and_then(|g| g.compile()) else {
            continue;
        };

        let mut indices = Vec::new();
        for node in &graph.nodes {
            if node.preset_index < preset_count && !indices.contains(&node.preset_index) {
                indices.push(node.preset_index);
            }
        }
        if indices.is_empty() {
            continue;
        }

        let entry_preset = graph
            .nodes
            .get(graph.entry)
            .map(|n| n.preset_index)
            .filter(|&idx| idx < preset_count);

        if explicit_key.as_ref().is_some_and(|k| k == &key) {
            selected = options.len();
        }

        let stem = path
            .file_stem()
            .and_then(|x| x.to_str())
            .unwrap_or("graph")
            .to_string();
        options.push(GraphOption {
            label: format!("{} [{} nodes / {} presets]", stem, graph.nodes.len(), indices.len()),
            preset_indices: indices,
            entry_preset,
        });
    }

    let selected = selected.min(options.len().saturating_sub(1));
    (options, selected)
}

fn discover_lyrics_options(explicit_path: Option<&str>) -> (Vec<LyricOption>, usize) {
    let mut options = vec![LyricOption {
        label: "none (disabled)".to_string(),
        path: None,
    }];
    let mut selected = 0usize;
    let explicit_key = explicit_path
        .filter(|s| !s.trim().is_empty())
        .map(|s| path_identity(Path::new(s)));
    let mut seen = HashSet::new();
    let mut candidates = Vec::<PathBuf>::new();

    if let Some(path) = explicit_path {
        if !path.trim().is_empty() {
            candidates.push(PathBuf::from(path));
        }
    }
    collect_files_with_ext(Path::new("assets/samples"), "lrc", &mut candidates);
    collect_files_with_ext(Path::new("assets/samples"), "txt", &mut candidates);
    if let Some(cfg_dir) = app_config_dir() {
        let lyrics_dir = cfg_dir.join("lyrics");
        collect_files_with_ext(&lyrics_dir, "lrc", &mut candidates);
        collect_files_with_ext(&lyrics_dir, "txt", &mut candidates);
    }

    for path in candidates {
        if !path.is_file() {
            continue;
        }
        let key = path_identity(path.as_path());
        if !seen.insert(key.clone()) {
            continue;
        }

        if explicit_key.as_ref().is_some_and(|k| k == &key) {
            selected = options.len();
        }

        let stem = path
            .file_stem()
            .and_then(|x| x.to_str())
            .unwrap_or("lyrics")
            .to_string();
        let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("file");
        options.push(LyricOption {
            label: format!("{} (.{})", stem, ext),
            path: Some(path),
        });
    }

    let selected = selected.min(options.len().saturating_sub(1));
    (options, selected)
}

fn candidate_config_files(
    explicit_path: Option<&str>,
    bundled_dir: &Path,
    ext: &str,
    user_subdir: &str,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(path) = explicit_path {
        if !path.trim().is_empty() {
            out.push(PathBuf::from(path));
        }
    }
    collect_files_with_ext(bundled_dir, ext, &mut out);
    if let Some(cfg_dir) = app_config_dir() {
        collect_files_with_ext(&cfg_dir.join(user_subdir), ext, &mut out);
    }
    out
}

fn collect_files_with_ext(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    let mut entries = read_dir
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| x.eq_ignore_ascii_case(ext))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    out.extend(entries);
}

fn app_config_dir() -> Option<PathBuf> {
    playlist_storage_path().and_then(|p| p.parent().map(|x| x.to_path_buf()))
}

fn path_identity(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn remove_runtime_playlists(playlists: &mut Vec<Playlist>, active_playlist: &mut usize, prefix: &str) {
    let mut idx = 1usize;
    while idx < playlists.len() {
        if playlists[idx].name.starts_with(prefix) {
            playlists.remove(idx);
            if *active_playlist == idx {
                *active_playlist = 0;
            } else if *active_playlist > idx {
                *active_playlist -= 1;
            }
        } else {
            idx += 1;
        }
    }
    if playlists.is_empty() {
        playlists.push(Playlist {
            name: "All Presets".to_string(),
            preset_indices: Vec::new(),
        });
        *active_playlist = 0;
    } else {
        *active_playlist = (*active_playlist).min(playlists.len().saturating_sub(1));
    }
}

fn apply_theme_option(
    option_idx: usize,
    theme_options: &[ThemeOption],
    engine: &mut dyn VisualEngine,
    playlists: &mut Vec<Playlist>,
    active_playlist: &mut usize,
    preset_count: usize,
    intensity: &mut f32,
    zoom_drive: &mut f32,
    loaded_theme_name: &mut String,
) {
    let Some(option) = theme_options.get(option_idx) else {
        return;
    };

    if let Some(pack) = option.pack.as_ref() {
        let before_preset = engine.preset_name().to_string();
        remove_runtime_playlists(playlists, active_playlist, "[Theme] ");
        let mut indices = option.preset_indices.clone();
        if indices.is_empty() {
            indices = (0..preset_count).collect();
        }
        playlists.push(Playlist {
            name: format!("[Theme] {}", pack.name),
            preset_indices: indices.clone(),
        });
        *active_playlist = playlists.len().saturating_sub(1);

        if let Some(&entry) = indices.first() {
            engine.set_playlist_indices(&[entry]);
        }
        engine.set_playlist_indices(&indices);
        if engine.preset_name() == before_preset && indices.len() > 1 {
            engine.next_preset();
        }

        *intensity = pack.intensity_default.clamp(0.10, 2.5);
        *zoom_drive = pack.zoom_default.clamp(0.12, 8.0);
        engine.set_fractal_zoom_drive(*zoom_drive);
        *loaded_theme_name = pack.name.clone();
    } else {
        remove_runtime_playlists(playlists, active_playlist, "[Theme] ");
        if let Some(all) = playlists.get_mut(0) {
            all.preset_indices = (0..preset_count).collect();
        }
        *active_playlist = 0;
        if let Some(all) = playlists.get(0) {
            engine.set_playlist_indices(&all.preset_indices);
        }
        loaded_theme_name.clear();
    }
}

fn apply_graph_option(
    option_idx: usize,
    graph_options: &[GraphOption],
    engine: &mut dyn VisualEngine,
    playlists: &mut Vec<Playlist>,
    active_playlist: &mut usize,
    loaded_graph_name: &mut String,
) {
    let Some(option) = graph_options.get(option_idx) else {
        return;
    };

    if !option.preset_indices.is_empty() {
        remove_runtime_playlists(playlists, active_playlist, "[Graph] ");
        let indices = option.preset_indices.clone();
        playlists.push(Playlist {
            name: format!("[Graph] {}", option.label),
            preset_indices: indices.clone(),
        });
        *active_playlist = playlists.len().saturating_sub(1);

        if let Some(entry) = option.entry_preset.filter(|idx| indices.contains(idx)) {
            engine.set_playlist_indices(&[entry]);
        }
        engine.set_playlist_indices(&indices);
        *loaded_graph_name = option.label.clone();
    } else {
        remove_runtime_playlists(playlists, active_playlist, "[Graph] ");
        if *active_playlist >= playlists.len() {
            *active_playlist = playlists.len().saturating_sub(1);
        }
        if let Some(active) = playlists.get(*active_playlist) {
            engine.set_playlist_indices(&active.preset_indices);
        }
        loaded_graph_name.clear();
    }
}

fn apply_lyrics_option(
    option_idx: usize,
    lyric_options: &[LyricOption],
    lyric_track: &mut Option<LyricsTrack>,
    lyrics_label: &mut String,
    warnings: &mut Vec<String>,
) {
    let Some(option) = lyric_options.get(option_idx) else {
        return;
    };

    if let Some(path) = option.path.as_ref() {
        match LyricsTrack::load(path) {
            Ok(track) => {
                let lines = track.line_count();
                *lyric_track = Some(track);
                *lyrics_label = format!("{} [{}]", option.label, lines);
            }
            Err(err) => {
                push_warning(
                    warnings,
                    format!("failed to load lyrics file '{}': {err}", path.display()),
                );
            }
        }
    } else {
        *lyric_track = None;
        *lyrics_label = "none".to_string();
    }
}

fn cycle_system_data_mode(mode: SystemDataMode) -> SystemDataMode {
    match mode {
        SystemDataMode::Off => SystemDataMode::Subtle,
        SystemDataMode::Subtle => SystemDataMode::Creep,
        SystemDataMode::Creep => SystemDataMode::Off,
    }
}

fn system_data_mode_label(mode: SystemDataMode) -> &'static str {
    match mode {
        SystemDataMode::Off => "off",
        SystemDataMode::Subtle => "subtle",
        SystemDataMode::Creep => "creep",
    }
}

fn centered_window_start(cursor: usize, total: usize, window: usize) -> usize {
    if total <= window {
        return 0;
    }
    cursor.saturating_sub(window / 2).min(total - window)
}

fn truncate_for_width(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if s.len() <= width {
        return s.to_string();
    }
    if width <= 1 {
        return s[..1].to_string();
    }
    let mut out = s[..width - 1].to_string();
    out.push('~');
    out
}

fn default_playlists(preset_count: usize) -> Vec<Playlist> {
    vec![Playlist {
        name: "All Presets".to_string(),
        preset_indices: (0..preset_count).collect(),
    }]
}

fn playlist_storage_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.trim().is_empty() {
            return Some(
                PathBuf::from(xdg)
                    .join("tui_visualizer")
                    .join("playlists.txt"),
            );
        }
    }
    let home = std::env::var("HOME").ok()?;
    if home.trim().is_empty() {
        return None;
    }
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("tui_visualizer")
            .join("playlists.txt"),
    )
}

fn load_playlists(path: Option<&Path>, preset_count: usize) -> Vec<Playlist> {
    let mut playlists = default_playlists(preset_count);
    let Some(path) = path else {
        return playlists;
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return playlists;
    };

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name_raw, idx_raw)) = line.split_once('\t') else {
            continue;
        };

        let mut indices = idx_raw
            .split(',')
            .filter_map(|x| x.trim().parse::<usize>().ok())
            .filter(|&i| i < preset_count)
            .collect::<Vec<_>>();
        indices.sort_unstable();
        indices.dedup();
        if indices.is_empty() {
            continue;
        }

        let name = name_raw.trim();
        if name.is_empty() || name.eq_ignore_ascii_case("all presets") {
            continue;
        }
        playlists.push(Playlist {
            name: name.to_string(),
            preset_indices: indices,
        });
    }

    playlists
}

fn save_playlists(playlists: &[Playlist], path: Option<&Path>) {
    let Some(path) = path else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }

    let mut content = String::from("# tui_visualizer playlists v1\n");
    for pl in playlists {
        let mut name = pl.name.replace(['\n', '\r', '\t'], " ");
        if name.trim().is_empty() {
            name = "Playlist".to_string();
        }
        let indices = pl
            .preset_indices
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let _ = writeln!(&mut content, "{}\t{}", name, indices);
    }
    let tmp = path.with_extension("tmp");
    if fs::write(&tmp, content).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

fn hud_rows_for_size(size: (u16, u16), show_hud: bool) -> u16 {
    if !show_hud {
        return 0;
    }
    let rows = size.1;
    if rows <= 1 {
        return 0;
    }
    (rows - 1).min(4)
}

fn hud_rows_for_text(term_rows: u16, show_hud: bool, hud: &str) -> u16 {
    if !show_hud {
        return 0;
    }
    let max_rows = term_rows.saturating_sub(1);
    let wanted = hud.lines().count() as u16;
    wanted.min(max_rows)
}

fn build_wrapped_hud(
    cols: usize,
    preset_name: &str,
    mode_label: &str,
    shuffle: bool,
    transition_mode: &str,
    transition_selection: &str,
    transition_locked: bool,
    transition_kind: &str,
    playlist_name: &str,
    playlist_count: usize,
    intensity: f32,
    zoom_mode: &str,
    zoom_drive: f32,
    zoom_enabled: bool,
    fractal_bias: bool,
    stage_mode: bool,
    scene_section: &str,
    camera_mode: &str,
    camera_speed: f32,
    typography_mode: &str,
    typography_text: &str,
    latency_mode: &str,
    probe_status: &str,
    theme_label: &str,
    graph_label: &str,
    lyrics_label: &str,
    system_data_label: &str,
    warning_status: &str,
    help_on: &str,
    fps: f32,
    lat_now: f32,
    lat_avg: f32,
    lat_p95: f32,
    engine_ms: f32,
    render_ms: f32,
    total_ms: f32,
    source_label: &str,
    engine_label: &str,
    renderer_name: &str,
) -> String {
    let logical_lines = vec![
        format!(
            "Preset: {} | Mode: {} | Shuffle: {} | Playlist: {} ({}) | TransMode: {} | TransSel: {}{} | TransFX: {} | Scene: {} | Cam: {} @ {:>4.2} | Int: {:>4.2} | Zoom: {} | ZoomDrive: {:>4.2} | ZoomFx: {} | FractalBias: {} | Typo(exp): {}",
            preset_name,
            mode_label,
            if shuffle { "on" } else { "off" },
            playlist_name,
            playlist_count,
            transition_mode,
            transition_selection,
            if transition_locked { " (fixed)" } else { "" },
            transition_kind,
            scene_section,
            camera_mode,
            camera_speed,
            intensity,
            zoom_mode,
            zoom_drive,
            if zoom_enabled { "on" } else { "off" },
            if fractal_bias { "on" } else { "off" },
            typography_mode,
        ),
        format!(
            "Lat(ms n/a/p95): {:>4.1}/{:>4.1}/{:>4.1} | Cal: {} | TypoText: {}",
            lat_now, lat_avg, lat_p95, latency_mode, typography_text
        ),
        format!(
            "ms(E/R/T): {:>4.1}/{:>4.1}/{:>4.1} | Source: {} | Engine: {} | Renderer: {} | Probe: {}",
            engine_ms, render_ms, total_ms, source_label, engine_label, renderer_name, probe_status
        ),
        format!(
            "Theme: {} | Graph: {} | Lyrics: {} | SysData: {} | Warning: {} | Stage: {} | Help: {} | FPS: {:>4.1}",
            theme_label,
            graph_label,
            lyrics_label,
            system_data_label,
            warning_status,
            if stage_mode { "on" } else { "off" },
            help_on,
            fps
        ),
        "Keys: ←/→ preset | p playlists | m themes | o graphs | k lyrics | u typography menu (exp) | ; sysdata | space auto | [/ ] transition sel | t transition mode | c cam mode | ,/. cam speed | up/down intensity | z zoom-mode | x/X zoom-speed | v zoom on/off | y typo on/off | Y typo style | l latency-cal | -/= latency offset | 0 reset offset | s shuffle | f bias | i HUD | g stage | ?/h/F1/tab help (exits stage) | q quit".to_string(),
    ];

    wrap_hud_lines(cols, &logical_lines).join("\n")
}

fn wrap_hud_lines(cols: usize, lines: &[String]) -> Vec<String> {
    let width = cols.max(1);
    let mut out = Vec::new();
    for line in lines {
        out.extend(smart_wrap_line(line, width));
    }
    out
}

fn smart_wrap_line(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }
    if line.chars().count() <= width {
        return vec![line.to_string()];
    }

    let mut out = Vec::new();
    let mut cur = String::new();
    for part in line.split(" | ") {
        if part.is_empty() {
            continue;
        }
        if cur.is_empty() {
            push_hud_segment(&mut out, &mut cur, part, width);
            continue;
        }
        let needed = 3 + part.chars().count();
        if cur.chars().count() + needed <= width {
            cur.push_str(" | ");
            cur.push_str(part);
        } else {
            out.push(std::mem::take(&mut cur));
            push_hud_segment(&mut out, &mut cur, part, width);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn push_hud_segment(out: &mut Vec<String>, cur: &mut String, part: &str, width: usize) {
    if part.chars().count() <= width {
        cur.push_str(part);
        return;
    }
    let mut chunk = String::new();
    let mut count = 0usize;
    for ch in part.chars() {
        chunk.push(ch);
        count += 1;
        if count >= width {
            if !cur.is_empty() {
                out.push(std::mem::take(cur));
            }
            out.push(std::mem::take(&mut chunk));
            count = 0;
        }
    }
    if !chunk.is_empty() {
        if !cur.is_empty() {
            out.push(std::mem::take(cur));
        }
        cur.push_str(&chunk);
    }
}

fn hotkey_highlight_key(code: KeyCode) -> Option<&'static str> {
    match code {
        KeyCode::Up | KeyCode::Down => Some("Int:"),
        KeyCode::Left | KeyCode::Right => Some("Preset:"),
        KeyCode::Char(' ') => Some("Mode:"),
        KeyCode::Char('i') | KeyCode::Char('I') => None,
        KeyCode::Char('g') | KeyCode::Char('G') => Some("Stage:"),
        KeyCode::Char('?')
        | KeyCode::Char('/')
        | KeyCode::Char('h')
        | KeyCode::Char('H')
        | KeyCode::F(1)
        | KeyCode::Tab => Some("Help:"),
        KeyCode::Char('p') | KeyCode::Char('P') => Some("Playlist:"),
        KeyCode::Char('m') | KeyCode::Char('M') => Some("Theme:"),
        KeyCode::Char('o') | KeyCode::Char('O') => Some("Graph:"),
        KeyCode::Char('u') | KeyCode::Char('U') => Some("Typo:"),
        KeyCode::Char('k') | KeyCode::Char('K') => Some("Lyrics:"),
        KeyCode::Char(';') | KeyCode::Char(':') => Some("SysData:"),
        KeyCode::Char('s') | KeyCode::Char('S') => Some("Shuffle:"),
        KeyCode::Char('t') | KeyCode::Char('T') => Some("TransMode:"),
        KeyCode::Char(']') | KeyCode::Char('[') => Some("TransSel:"),
        KeyCode::Char('f') | KeyCode::Char('F') => Some("FractalBias:"),
        KeyCode::Char('z') | KeyCode::Char('Z') => Some("Zoom:"),
        KeyCode::Char('c') | KeyCode::Char('C') => Some("Cam:"),
        KeyCode::Char('.') | KeyCode::Char(',') => Some("Cam:"),
        KeyCode::Char('x') | KeyCode::Char('X') => Some("ZoomDrive:"),
        KeyCode::Char('v') | KeyCode::Char('V') => Some("ZoomFx:"),
        KeyCode::Char('l')
        | KeyCode::Char('L')
        | KeyCode::Char('-')
        | KeyCode::Char('=')
        | KeyCode::Char('+')
        | KeyCode::Char('0') => Some("Cal:"),
        KeyCode::Char('y') | KeyCode::Char('Y') => Some("Typo:"),
        KeyCode::Char('1')
        | KeyCode::Char('2')
        | KeyCode::Char('3')
        | KeyCode::Char('4')
        | KeyCode::Char('5') => Some("Mode:"),
        _ => None,
    }
}

fn help_popup_text(
    probe_status: &str,
    latency_status: &str,
    typography_mode: TypographyMode,
    warning_status: &str,
) -> String {
    format!(
        "TUI Visualizer Hotkeys\n\
Probe: {probe_status}\n\
Latency calibration: {latency_status}\n\
Typography mode (experimental WIP): {}\n\
Warning: {warning_status}\n\
←/→  previous/next preset\n\
space  toggle auto mode (manual/adaptive)\n\
1/2/3/4/5  switch mode: manual/beat/energy/time/adaptive\n\
s  toggle shuffle\n\
t  cycle transition mode: auto/smooth/punchy/morph/remix/cuts\n\
[ / ]  step transition selection (Auto -> specific FX -> Auto)\n\
c  cycle camera path mode\n\
, / .  camera path speed down / up\n\
p  open/close Playlist Manager (in stage mode: exits stage and opens)\n\
m  open theme selector popup\n\
o  open preset-graph selector popup\n\
k  open lyrics selector popup\n\
u  open typography selector popup (experimental)\n\
Selector popup keys:\n\
  up/down  move cursor\n\
  enter or space  apply current option\n\
  tab/left/right  switch selector group\n\
  m/o/k/u  jump selector group directly\n\
  esc  close selector\n\
Playlist Manager keys:\n\
  tab/left/right  switch pane\n\
  up/down  move cursor\n\
  enter  apply selected playlist (left) / toggle preset membership (right)\n\
  space  toggle preset membership\n\
  n  new playlist from current active selection\n\
  x or d  delete selected playlist (except All Presets)\n\
  a / r  add / remove highlighted preset\n\
  esc or p  close manager\n\
f  toggle calm-section fractal auto-bias\n\
z  cycle fractal zoom mode: hypnotic/balanced/wormhole\n\
x / X  zoom speed up / down\n\
v  toggle zoom motion on/off\n\
y  toggle typography on/off (experimental)\n\
Y (shift+y)  cycle typography style (line/word/glyph/matrix, experimental)\n\
;  cycle system-data feed mode: off -> subtle -> creep\n\
Typography modes (experimental):\n\
  off: typography layer disabled\n\
  line: scrolling BROTVIZ ribbons synced to rhythm\n\
  word: center-word pulses with beat-driven motion\n\
  glyph: orbiting glyph swarm (neon ring flow)\n\
  matrix: reactive alphanumeric rain columns\n\
CLI-only typography inputs:\n\
  --lyrics-file <path>  load .lrc/.txt synced lyric lines\n\
  --lyrics-loop true|false  loop lyric timeline\n\
  --lyrics-offset-ms <ms>  nudge lyric timing\n\
  --system-data off|subtle|creep  local-only data tokens for typography\n\
l  toggle latency auto-calibration\n\
- / =  latency offset down / up (ms)\n\
0  reset manual latency offset\n\
up/down  intensity\n\
i  show/hide HUD\n\
g  toggle stage mode (HUD + popup overlays off, performance-biased governor; persisted)\n\
? or / or h or F1 or tab  toggle this help\n\
q or esc  quit",
        typography_mode.label()
    )
}

fn push_warning(warnings: &mut Vec<String>, message: impl Into<String>) {
    let message = message.into();
    if warnings.iter().any(|w| w == &message) {
        return;
    }
    warnings.push(message);
    if warnings.len() > 8 {
        warnings.remove(0);
    }
}

fn latest_warning(warnings: &[String]) -> &str {
    warnings.last().map(|s| s.as_str()).unwrap_or("none")
}

fn supports_control_name(name: &str) -> bool {
    matches!(
        name,
        "intensity"
            | "zoom"
            | "zoom_drive"
            | "camera_speed"
            | "camera_mode"
            | "fractal_bias"
            | "zoom_enabled"
            | "typography_mode"
    )
}

fn apply_control_routes(
    controls: &std::collections::BTreeMap<String, f32>,
    engine: &mut dyn VisualEngine,
    intensity: &mut f32,
    zoom_drive: &mut f32,
    typography_mode: &mut TypographyMode,
) {
    if let Some(v) = controls.get("intensity").copied() {
        *intensity = v.clamp(0.10, 2.5);
    }

    if let Some(v) = controls
        .get("zoom_drive")
        .copied()
        .or_else(|| controls.get("zoom").copied())
    {
        *zoom_drive = v.clamp(0.12, 8.0);
        engine.set_fractal_zoom_drive(*zoom_drive);
    }

    if let Some(v) = controls.get("camera_speed").copied() {
        let target = v.clamp(0.15, 4.0);
        let current = engine.camera_path_speed();
        let delta = (target - current).clamp(-0.20, 0.20);
        if delta.abs() > 0.01 {
            engine.step_camera_path_speed(delta);
        }
    }

    if let Some(v) = controls.get("camera_mode").copied() {
        let target = ((v.clamp(0.0, 1.0) * 5.0).round() as usize).min(5);
        set_engine_camera_mode(engine, target);
    }

    if let Some(v) = controls.get("fractal_bias").copied() {
        if v >= 0.66 && !engine.fractal_bias() {
            engine.toggle_fractal_bias();
        } else if v <= 0.33 && engine.fractal_bias() {
            engine.toggle_fractal_bias();
        }
    }

    if let Some(v) = controls.get("zoom_enabled").copied() {
        if v >= 0.66 && !engine.fractal_zoom_enabled() {
            engine.toggle_fractal_zoom_enabled();
        } else if v <= 0.33 && engine.fractal_zoom_enabled() {
            engine.toggle_fractal_zoom_enabled();
        }
    }

    if let Some(v) = controls.get("typography_mode").copied() {
        *typography_mode = TypographyMode::from_unit_interval(v);
    }
}

fn set_engine_camera_mode(engine: &mut dyn VisualEngine, target_idx: usize) {
    let mut current_idx = camera_mode_index(engine.camera_path_mode());
    let target_idx = target_idx.min(5);
    if current_idx == target_idx {
        return;
    }
    for _ in 0..6 {
        if current_idx == target_idx {
            break;
        }
        let forward_steps = (target_idx + 6 - current_idx) % 6;
        let backward_steps = (current_idx + 6 - target_idx) % 6;
        if forward_steps <= backward_steps {
            engine.step_camera_path_mode(true);
            current_idx = (current_idx + 1) % 6;
        } else {
            engine.step_camera_path_mode(false);
            current_idx = (current_idx + 5) % 6;
        }
    }
}

fn camera_mode_index(mode: CameraPathMode) -> usize {
    match mode {
        CameraPathMode::Auto => 0,
        CameraPathMode::Orbit => 1,
        CameraPathMode::Dolly => 2,
        CameraPathMode::Helix => 3,
        CameraPathMode::Spiral => 4,
        CameraPathMode::Drift => 5,
    }
}

struct FpsCounter {
    last: Instant,
    frames: u32,
    fps: f32,
}

impl FpsCounter {
    fn new() -> Self {
        Self {
            last: Instant::now(),
            frames: 0,
            fps: 0.0,
        }
    }

    fn tick(&mut self) {
        self.frames += 1;
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f32();
        if dt >= 0.5 {
            self.fps = (self.frames as f32) / dt;
            self.frames = 0;
            self.last = now;
        }
    }

    fn fps(&self) -> f32 {
        self.fps
    }
}

struct RuntimeTuning {
    base_quality: Quality,
    quality: Quality,
    scale: usize,
    adaptive: bool,
    stage_mode: bool,
    ema_ms: f32,
    over_budget_streak: u8,
    under_budget_streak: u8,
    cooldown_frames: u16,
}

impl RuntimeTuning {
    fn new(base_quality: Quality, adaptive: bool, stage_mode: bool) -> Self {
        Self {
            base_quality,
            quality: base_quality,
            scale: 1,
            adaptive,
            stage_mode,
            ema_ms: 0.0,
            over_budget_streak: 0,
            under_budget_streak: 0,
            cooldown_frames: 0,
        }
    }

    fn set_stage_mode(&mut self, on: bool) {
        self.stage_mode = on;
        self.over_budget_streak = 0;
        self.under_budget_streak = 0;
        if self.cooldown_frames > 18 {
            self.cooldown_frames = 18;
        }
    }

    fn update(&mut self, frame_ms: f32, target_ms: f32) {
        if !self.adaptive {
            return;
        }
        self.ema_ms = if self.ema_ms == 0.0 {
            frame_ms
        } else {
            self.ema_ms * 0.95 + frame_ms * 0.05
        };
        let (
            downscale_hi,
            upscale_lo,
            downscale_votes,
            upscale_votes,
            cooldown_after_change,
            cooldown_after_recover,
        ) = if self.stage_mode {
            (1.07, 0.70, 3u8, 24u8, 20u16, 28u16)
        } else {
            (1.17, 0.76, 4u8, 32u8, 24u16, 34u16)
        };

        if self.cooldown_frames > 0 {
            self.cooldown_frames -= 1;
        }

        if self.ema_ms > target_ms * downscale_hi {
            self.over_budget_streak = self.over_budget_streak.saturating_add(1);
            self.under_budget_streak = 0;
        } else if self.ema_ms < target_ms * upscale_lo {
            self.under_budget_streak = self.under_budget_streak.saturating_add(1);
            self.over_budget_streak = 0;
        } else {
            self.over_budget_streak = self.over_budget_streak.saturating_sub(1);
            self.under_budget_streak = self.under_budget_streak.saturating_sub(1);
        }

        if self.cooldown_frames == 0 && self.over_budget_streak >= downscale_votes {
            if self.scale == 1 {
                self.scale = 2;
            } else {
                self.quality = self.quality.lower();
            }
            self.cooldown_frames = cooldown_after_change;
            self.over_budget_streak = 0;
            self.under_budget_streak = 0;
            return;
        }

        if self.cooldown_frames == 0 && self.under_budget_streak >= upscale_votes {
            if quality_rank(self.quality) < quality_rank(self.base_quality) {
                self.quality = self.quality.higher();
                if quality_rank(self.quality) > quality_rank(self.base_quality) {
                    self.quality = self.base_quality;
                }
            } else if self.scale > 1 {
                self.scale = 1;
            } else {
                return;
            }
            self.cooldown_frames = cooldown_after_recover;
            self.over_budget_streak = 0;
            self.under_budget_streak = 0;
        }
    }
}

fn quality_rank(q: Quality) -> u8 {
    match q {
        Quality::Fast => 0,
        Quality::Balanced => 1,
        Quality::High => 2,
        Quality::Ultra => 3,
    }
}

fn apply_intensity(mut a: crate::audio::AudioFeatures, intensity: f32) -> crate::audio::AudioFeatures {
    let s = intensity.clamp(0.0, 8.0);
    if (s - 1.0).abs() < 1e-3 {
        return a;
    }
    a.rms = (a.rms * s).clamp(0.0, 1.0);
    for b in &mut a.bands {
        *b = (*b * s).clamp(0.0, 1.0);
    }
    a.onset = (a.onset * s).clamp(0.0, 1.0);
    a.beat_strength = (a.beat_strength * s).clamp(0.0, 1.0);
    a
}

struct LatencyStats {
    vals: [f32; 256],
    len: usize,
    pos: usize,
}

impl LatencyStats {
    fn new() -> Self {
        Self {
            vals: [0.0; 256],
            len: 0,
            pos: 0,
        }
    }

    fn push(&mut self, v: f32) {
        self.vals[self.pos] = v.max(0.0);
        self.pos = (self.pos + 1) % self.vals.len();
        if self.len < self.vals.len() {
            self.len += 1;
        }
    }

    fn snapshot(&self) -> (f32, f32, f32) {
        if self.len == 0 {
            return (0.0, 0.0, 0.0);
        }

        let mut n = 0usize;
        let mut sum = 0.0f32;
        for i in 0..self.len {
            sum += self.vals[i];
            n += 1;
        }
        let avg = if n == 0 { 0.0 } else { sum / n as f32 };
        let last_idx = (self.pos + self.vals.len() - 1) % self.vals.len();
        let now = self.vals[last_idx];

        let mut tmp = [0.0f32; 256];
        tmp[..self.len].copy_from_slice(&self.vals[..self.len]);
        tmp[..self.len].sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let p95_idx = ((self.len as f32 - 1.0) * 0.95).round() as usize;
        let p95 = tmp[p95_idx.min(self.len.saturating_sub(1))];

        (now, avg, p95)
    }
}
