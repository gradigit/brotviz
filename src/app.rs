use crate::audio::AudioSystem;
use crate::config::{Config, EngineMode, Quality, RendererMode, SwitchMode};
use crate::render::{BrailleRenderer, Frame, HalfBlockRenderer, KittyRenderer, Renderer};
use crate::terminal::TerminalGuard;
use crate::visual::{make_presets, PresetEngine, RenderCtx, VisualEngine};
use anyhow::Context;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::fmt::Write as _;
use std::fs;
use std::io::BufWriter;
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

pub fn run(cfg: Config) -> anyhow::Result<()> {
    let _term = TerminalGuard::new()?;
    let mut out = BufWriter::new(TerminalGuard::stdout());

    let mut renderer: Box<dyn Renderer> = match cfg.renderer {
        RendererMode::HalfBlock => Box::new(HalfBlockRenderer::new()),
        RendererMode::Braille => Box::new(BrailleRenderer::new()),
        RendererMode::Kitty => Box::new(KittyRenderer::new()),
    };

    let (px_w_mul, px_h_mul) = match cfg.renderer {
        RendererMode::HalfBlock => (1usize, 2usize),
        // Render at 2x4 pixels per cell for Kitty to look materially sharper than half-block.
        RendererMode::Kitty => (2usize, 4usize),
        RendererMode::Braille => (2usize, 4usize),
    };

    let audio = AudioSystem::new(cfg.source, cfg.device.as_deref())
        .with_context(|| format!("start audio (source={:?})", cfg.source))?;
    let audio_features = audio.features();

    let presets = make_presets();
    let preset_names = presets.iter().map(|p| p.name()).collect::<Vec<_>>();
    let active = select_preset(&cfg.preset, &presets).unwrap_or(0);

    let mut engine: Box<dyn VisualEngine> = if cfg.engine == EngineMode::Cpu {
        Box::new(PresetEngine::new(
            presets,
            active,
            cfg.shuffle,
            cfg.switch,
            cfg.beats_per_switch,
            cfg.seconds_per_switch,
        ))
    } else {
        #[cfg(target_os = "macos")]
        {
            Box::new(crate::visual::MetalEngine::new(
                preset_names.clone(),
                active,
                cfg.shuffle,
                cfg.switch,
                cfg.beats_per_switch,
                cfg.seconds_per_switch,
            )?)
        }

        #[cfg(not(target_os = "macos"))]
        {
            return Err(anyhow::anyhow!("--engine metal is only supported on macOS"));
        }
    };

    let playlist_store = playlist_storage_path();
    let mut playlists = load_playlists(playlist_store.as_deref(), preset_names.len());
    let mut active_playlist = 0usize;
    let mut playlist_ui = PlaylistUi::new();
    engine.set_playlist_indices(&playlists[active_playlist].preset_indices);

    let mut last_size = crossterm::terminal::size().context("get terminal size")?;
    if last_size.1 < 2 || last_size.0 < 4 {
        return Err(anyhow::anyhow!(
            "terminal too small (need at least 4x2, got {}x{})",
            last_size.0,
            last_size.1
        ));
    }

    let mut show_hud = true;
    let mut show_help = false;
    let mut hud_rows = hud_rows_for_size(last_size, show_hud);

    let mut runtime = RuntimeTuning::new(cfg.quality, cfg.adaptive_quality);
    resize_engine(&mut *engine, last_size, px_w_mul, px_h_mul, hud_rows)?;

    let start = Instant::now();
    let mut last_frame = start;

    let mut fps = FpsCounter::new();
    let mut beat_pulse = 0.0f32;
    let mut intensity = 1.0f32;
    let mut zoom_drive = 1.0f32;
    let mut last_engine_ms = 0.0f32;
    let mut last_render_ms = 0.0f32;
    let mut last_total_ms = 0.0f32;
    let mut lat_stats = LatencyStats::new();
    let source_label = format!("{:?}", cfg.source);
    let engine_label = format!("{:?}", cfg.engine);

    loop {
        let now = Instant::now();

        // Drain input events (non-blocking).
        while event::poll(Duration::from_millis(0))? {
            match event::read()? {
                Event::Key(k) if k.kind != KeyEventKind::Release => {
                    let old_hud = show_hud;
                    let should_quit = if playlist_ui.open {
                        handle_playlist_key(
                            k.code,
                            k.modifiers,
                            &mut *engine,
                            &mut playlist_ui,
                            &mut playlists,
                            &mut active_playlist,
                            preset_names.len(),
                            playlist_store.as_deref(),
                        )
                    } else {
                        handle_key(
                            k.code,
                            k.modifiers,
                            &mut *engine,
                            &mut intensity,
                            &mut zoom_drive,
                            &mut show_hud,
                            &mut show_help,
                            &mut playlist_ui.open,
                        )
                    };
                    if should_quit {
                        return Ok(());
                    }
                    if playlist_ui.open {
                        show_help = false;
                    }

                    if show_hud != old_hud {
                        hud_rows = hud_rows_for_size(last_size, show_hud);
                        resize_engine(&mut *engine, last_size, px_w_mul, px_h_mul, hud_rows)?;
                    }
                }
                Event::Resize(c, r) => {
                    last_size = (c, r);
                    hud_rows = hud_rows_for_size(last_size, show_hud);
                    resize_engine(&mut *engine, last_size, px_w_mul, px_h_mul, hud_rows)?;
                }
                _ => {}
            }
        }

        // Size check once per frame (resize events can be missed in some terminals).
        let sz = crossterm::terminal::size()?;
        if sz != last_size {
            last_size = sz;
            hud_rows = hud_rows_for_size(last_size, show_hud);
            resize_engine(&mut *engine, last_size, px_w_mul, px_h_mul, hud_rows)?;
        }

        let dt = now.duration_since(last_frame).as_secs_f32().max(1e-6);
        last_frame = now;

        let raw_audio = audio_features.load();
        let audio_age_ms = audio_features.age_ms();
        if raw_audio.beat {
            beat_pulse = (beat_pulse + 0.65 + raw_audio.beat_strength * 0.7).min(1.6);
        }
        // Exponential decay; tuned for hypnotic "breathing" rather than a hard flash.
        beat_pulse *= (0.1f32).powf(dt);

        engine.update_auto_switch(now, &raw_audio);

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
        let renderer_name = renderer.name();
        let (lat_now, lat_avg, lat_p95) = lat_stats.snapshot();

        let hud = if show_hud {
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
                    intensity,
                &zoom_mode,
                zoom_drive,
                zoom_enabled,
                fractal_bias,
                if show_help { "on" } else { "off" },
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

        let target_hud_rows = hud_rows_for_text(term_rows, show_hud, &hud);
        if target_hud_rows != hud_rows {
            hud_rows = target_hud_rows;
            resize_engine(&mut *engine, last_size, px_w_mul, px_h_mul, hud_rows)?;
        }
        let visual_rows = term_rows.saturating_sub(hud_rows).max(1);
        let w = (term_cols as usize).saturating_mul(px_w_mul);
        let h = (visual_rows as usize).saturating_mul(px_h_mul);

        let audio = apply_intensity(raw_audio, intensity);

        let ctx = RenderCtx {
            now,
            t: now.duration_since(start).as_secs_f32(),
            dt,
            w,
            h,
            audio,
            beat_pulse: (beat_pulse * intensity).clamp(0.0, 1.0),
            fractal_zoom_mul: 1.0,
            safe: cfg.safe,
            quality: runtime.quality,
            scale: runtime.scale,
        };

        let engine_start = Instant::now();
        let pixels = engine.render(ctx, runtime.quality, runtime.scale);
        let engine_ms = engine_start.elapsed().as_secs_f32() * 1000.0;
        last_engine_ms = engine_ms;

        let playlist_overlay = if playlist_ui.open {
            Some(build_playlist_popup(
                term_cols,
                term_rows,
                &playlists,
                active_playlist,
                &playlist_ui,
                &preset_names,
            ))
        } else {
            None
        };
        let overlay = if let Some(ref text) = playlist_overlay {
            Some(text.as_str())
        } else if show_help {
            Some(help_popup_text())
        } else {
            None
        };

        let frame = Frame {
            term_cols,
            term_rows,
            visual_rows,
            pixel_width: w,
            pixel_height: h,
            pixels_rgba: pixels,
            hud: &hud,
            hud_rows,
            overlay,
            sync_updates: cfg.sync_updates,
        };

        let frame_start = Instant::now();
        renderer.render(&frame, &mut out)?;
        let render_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
        let total_ms = now.elapsed().as_secs_f32() * 1000.0;
        let end_to_end_latency_ms = audio_age_ms + total_ms;
        lat_stats.push(end_to_end_latency_ms);

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

fn handle_key(
    code: KeyCode,
    mods: KeyModifiers,
    engine: &mut dyn VisualEngine,
    intensity: &mut f32,
    zoom_drive: &mut f32,
    show_hud: &mut bool,
    show_help: &mut bool,
    show_playlist: &mut bool,
) -> bool {
    if mods.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c')) {
        return true;
    }

    match code {
        KeyCode::Esc => true,
        KeyCode::Char('q') | KeyCode::Char('Q') => true,
        KeyCode::Up => {
            *intensity = (*intensity + 0.05).min(2.5);
            false
        }
        KeyCode::Down => {
            *intensity = (*intensity - 0.05).max(0.10);
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
        KeyCode::Char(' ') => {
            engine.toggle_auto_switch();
            false
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            *show_hud = !*show_hud;
            false
        }
        KeyCode::Char('?')
        | KeyCode::Char('/')
        | KeyCode::Char('h')
        | KeyCode::Char('H')
        | KeyCode::F(1)
        | KeyCode::Tab => {
            *show_help = !*show_help;
            if *show_help {
                *show_playlist = false;
            }
            false
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            *show_playlist = !*show_playlist;
            if *show_playlist {
                *show_help = false;
            }
            false
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            engine.toggle_shuffle();
            false
        }
        KeyCode::Char('t') | KeyCode::Char('T') => {
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
        KeyCode::Char('f') | KeyCode::Char('F') => {
            engine.toggle_fractal_bias();
            false
        }
        KeyCode::Char('z') | KeyCode::Char('Z') => {
            engine.cycle_fractal_zoom_mode();
            false
        }
        KeyCode::Char('x') => {
            *zoom_drive = (*zoom_drive * 1.25).clamp(0.12, 8.0);
            engine.set_fractal_zoom_drive(*zoom_drive);
            false
        }
        KeyCode::Char('X') => {
            *zoom_drive = (*zoom_drive / 1.25).clamp(0.12, 8.0);
            engine.set_fractal_zoom_drive(*zoom_drive);
            false
        }
        KeyCode::Char('v') | KeyCode::Char('V') => {
            engine.toggle_fractal_zoom_enabled();
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
    let _ = fs::write(path, content);
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
            "Preset: {} | Mode: {} | Shuffle: {} | Playlist: {} ({}) | TransMode: {} | TransSel: {}{} | TransFX: {} | Int: {:>4.2} | Zoom: {} | ZoomDrive: {:>4.2} | ZoomFx: {} | FractalBias: {} | Help: {} | FPS: {:>4.1}",
            preset_name,
            mode_label,
            if shuffle { "on" } else { "off" },
            playlist_name,
            playlist_count,
            transition_mode,
            transition_selection,
            if transition_locked { " (fixed)" } else { "" },
            transition_kind,
            intensity,
            zoom_mode,
            zoom_drive,
            if zoom_enabled { "on" } else { "off" },
            if fractal_bias { "on" } else { "off" },
            help_on,
            fps,
        ),
        format!(
            "Lat(ms n/a/p95): {:>4.1}/{:>4.1}/{:>4.1} | ms(E/R/T): {:>4.1}/{:>4.1}/{:>4.1}",
            lat_now, lat_avg, lat_p95, engine_ms, render_ms, total_ms
        ),
        format!(
            "Source: {} | Engine: {} | Renderer: {}",
            source_label, engine_label, renderer_name
        ),
        "Keys: ←/→ preset | p playlists | space auto | [/ ] transition sel | t transition mode | up/down intensity | z zoom-mode | x/X zoom-speed | v zoom on/off | s shuffle | f bias | i HUD | ?/h/F1/tab help | q quit".to_string(),
    ];

    wrap_hud_lines(cols, &logical_lines).join("\n")
}

fn wrap_hud_lines(cols: usize, lines: &[String]) -> Vec<String> {
    let width = cols.max(1);
    let mut out = Vec::new();
    for line in lines {
        out.extend(hard_wrap_line(line, width));
    }
    out
}

fn hard_wrap_line(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }

    let mut out = Vec::new();
    let mut cur = String::new();
    let mut cur_len = 0usize;
    for ch in line.chars() {
        cur.push(ch);
        cur_len += 1;
        if cur_len >= width {
            out.push(cur);
            cur = String::new();
            cur_len = 0;
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

fn help_popup_text() -> &'static str {
    "TUI Visualizer Hotkeys\n\
←/→  previous/next preset\n\
space  toggle auto mode (manual/adaptive)\n\
1/2/3/4/5  switch mode: manual/beat/energy/time/adaptive\n\
s  toggle shuffle\n\
t  cycle transition mode: auto/smooth/punchy/morph/remix/cuts\n\
[ / ]  step transition selection (Auto -> specific FX -> Auto)\n\
p  open/close Playlist Manager\n\
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
up/down  intensity\n\
i  show/hide HUD\n\
? or / or h or F1 or tab  toggle this help\n\
q or esc  quit"
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
    ema_ms: f32,
}

impl RuntimeTuning {
    fn new(base_quality: Quality, adaptive: bool) -> Self {
        Self {
            base_quality,
            quality: base_quality,
            scale: 1,
            adaptive,
            ema_ms: 0.0,
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

        if self.ema_ms > target_ms * 1.22 {
            if self.scale == 1 {
                self.scale = 2;
            } else {
                self.quality = self.quality.lower();
            }
            return;
        }

        if self.ema_ms < target_ms * 0.72 {
            if quality_rank(self.quality) < quality_rank(self.base_quality) {
                self.quality = self.quality.higher();
                if quality_rank(self.quality) > quality_rank(self.base_quality) {
                    self.quality = self.base_quality;
                }
            } else if self.scale > 1 {
                self.scale = 1;
            }
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
