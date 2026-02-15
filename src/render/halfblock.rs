use crate::render::{draw_overlay_popup, Frame, Renderer};
use std::io::Write;

pub struct HalfBlockRenderer {
    last_fg: Option<(u8, u8, u8)>,
    last_bg: Option<(u8, u8, u8)>,
}

impl HalfBlockRenderer {
    pub fn new() -> Self {
        Self {
            last_fg: None,
            last_bg: None,
        }
    }
}

impl Renderer for HalfBlockRenderer {
    fn name(&self) -> &'static str {
        "halfblock"
    }

    fn render(&mut self, frame: &Frame<'_>, out: &mut dyn Write) -> anyhow::Result<()> {
        let cols = frame.term_cols as usize;
        let visual_rows = frame.visual_rows as usize;
        let w = frame.pixel_width;
        let h = frame.pixel_height;

        if cols == 0 || visual_rows == 0 || w == 0 || h == 0 {
            return Ok(());
        }
        if w != cols || h != visual_rows.saturating_mul(2) {
            // Internal mismatch; avoid panics.
            return Ok(());
        }

        let need = w.saturating_mul(h).saturating_mul(4);
        if frame.pixels_rgba.len() < need {
            // Defensive: don't index out of bounds; show a HUD so it's obvious.
            if frame.sync_updates {
                out.write_all(b"\x1b[?2026h")?;
            }
            out.write_all(b"\x1b[H\x1b[0m\x1b[2J")?;
            write!(
                out,
                "pixel buffer too small (need {}, got {})",
                need,
                frame.pixels_rgba.len()
            )?;
            if frame.sync_updates {
                out.write_all(b"\x1b[?2026l")?;
            }
            out.flush()?;
            return Ok(());
        }

        if frame.sync_updates {
            out.write_all(b"\x1b[?2026h")?;
        }

        // Home, reset
        out.write_all(b"\x1b[H\x1b[0m")?;
        // Disable autowrap (DECAWM) while we paint full-width rows; some terminals will otherwise
        // wrap when the last column is written, and the subsequent newline creates visible gaps.
        out.write_all(b"\x1b[?7l")?;
        self.last_fg = None;
        self.last_bg = None;

        const HALF_BLOCK: char = '\u{2580}';

        for row in 0..visual_rows {
            let top_y = row * 2;
            let bot_y = top_y + 1;
            for x in 0..cols {
                let top_i = (top_y * w + x) * 4;
                let bot_i = (bot_y * w + x) * 4;
                let (tr, tg, tb) = (
                    frame.pixels_rgba[top_i],
                    frame.pixels_rgba[top_i + 1],
                    frame.pixels_rgba[top_i + 2],
                );
                let (br, bg, bb) = (
                    frame.pixels_rgba[bot_i],
                    frame.pixels_rgba[bot_i + 1],
                    frame.pixels_rgba[bot_i + 2],
                );

                if self.last_fg != Some((tr, tg, tb)) {
                    write!(out, "\x1b[38;2;{};{};{}m", tr, tg, tb)?;
                    self.last_fg = Some((tr, tg, tb));
                }
                if self.last_bg != Some((br, bg, bb)) {
                    write!(out, "\x1b[48;2;{};{};{}m", br, bg, bb)?;
                    self.last_bg = Some((br, bg, bb));
                }
                write!(out, "{HALF_BLOCK}")?;
            }
            // Next line (CRLF) with autowrap disabled.
            out.write_all(b"\r\n")?;
        }

        // HUD lines (bottom area)
        let mut hud_lines = frame.hud.lines();
        for i in 0..(frame.hud_rows as usize) {
            write!(out, "\x1b[{};1H\x1b[0m\x1b[2K", visual_rows + i + 1)?;
            if let Some(mut line) = hud_lines.next() {
                if line.len() > cols {
                    line = &line[..cols];
                }
                write!(out, "{line}")?;
            }
        }

        if let Some(text) = frame.overlay {
            draw_overlay_popup(out, frame.term_cols, frame.term_rows, text)?;
        }

        // Restore autowrap.
        out.write_all(b"\x1b[?7h")?;

        if frame.sync_updates {
            out.write_all(b"\x1b[?2026l")?;
        }
        out.flush()?;
        Ok(())
    }
}
