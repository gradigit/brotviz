use crate::config::{EngineMode, RendererMode};

#[derive(Debug, Clone)]
pub struct CapabilityReport {
    pub auto_probe: bool,
    pub requested_engine: EngineMode,
    pub requested_renderer: RendererMode,
    pub engine: EngineMode,
    pub renderer: RendererMode,
    notes: Vec<String>,
}

impl CapabilityReport {
    pub fn changed(&self) -> bool {
        self.engine != self.requested_engine || self.renderer != self.requested_renderer
    }

    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    pub fn push_note(&mut self, note: impl Into<String>) {
        self.notes.push(note.into());
    }

    pub fn record_engine_fallback(&mut self, to: EngineMode, reason: impl Into<String>) {
        self.engine = to;
        self.notes.push(reason.into());
    }

    pub fn status_label(&self) -> String {
        if !self.auto_probe {
            return format!(
                "off (engine={:?}, renderer={:?})",
                self.engine, self.renderer
            );
        }
        if self.changed() {
            return format!(
                "fallback eng {:?}->{:?}, ren {:?}->{:?}",
                self.requested_engine, self.engine, self.requested_renderer, self.renderer
            );
        }
        format!("ok eng={:?}, ren={:?}", self.engine, self.renderer)
    }
}

pub fn probe_runtime(
    requested_engine: EngineMode,
    requested_renderer: RendererMode,
    auto_probe: bool,
) -> CapabilityReport {
    let mut report = CapabilityReport {
        auto_probe,
        requested_engine,
        requested_renderer,
        engine: requested_engine,
        renderer: requested_renderer,
        notes: Vec::new(),
    };

    if !auto_probe {
        report.push_note("capability probe disabled by --auto-probe=false");
        return report;
    }

    if requested_renderer == RendererMode::Kitty && !kitty_graphics_available() {
        report.renderer = RendererMode::HalfBlock;
        report.push_note("kitty graphics unavailable in this terminal; falling back to half-block renderer");
    }

    if requested_engine == EngineMode::Metal {
        #[cfg(not(target_os = "macos"))]
        {
            report.engine = EngineMode::Cpu;
            report.push_note("metal engine unsupported on this platform; falling back to cpu engine");
        }

        #[cfg(target_os = "macos")]
        {
            report.push_note("metal runtime validation pending during engine initialization");
        }
    }

    if report.notes.is_empty() {
        report.push_note("probe selected requested engine/renderer with no fallback");
    }

    report
}

fn kitty_graphics_available() -> bool {
    if let Ok(v) = std::env::var("TUIVIZ_FORCE_KITTY") {
        let s = v.trim().to_ascii_lowercase();
        if s == "1" || s == "true" || s == "yes" || s == "on" {
            return true;
        }
        if s == "0" || s == "false" || s == "no" || s == "off" {
            return false;
        }
    }

    if std::env::var("KITTY_WINDOW_ID").is_ok() {
        return true;
    }

    let term = std::env::var("TERM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if term.contains("kitty") {
        return true;
    }

    let term_program = std::env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if term_program.contains("ghostty") || term_program.contains("kitty") {
        return true;
    }

    false
}
