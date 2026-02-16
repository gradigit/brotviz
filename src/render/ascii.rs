use crate::render::{draw_overlay_popup, write_hud_line, Frame, Renderer};
use std::io::Write;

pub struct AsciiRenderer {
    last_fg: Option<(u8, u8, u8)>,
}

impl AsciiRenderer {
    pub fn new() -> Self {
        Self { last_fg: None }
    }
}

impl Renderer for AsciiRenderer {
    fn name(&self) -> &'static str {
        "ascii"
    }

    fn render(&mut self, frame: &Frame<'_>, out: &mut dyn Write) -> anyhow::Result<()> {
        let cols = frame.term_cols as usize;
        let visual_rows = frame.visual_rows as usize;
        let w = frame.pixel_width;
        let h = frame.pixel_height;

        if cols == 0 || visual_rows == 0 || w == 0 || h == 0 {
            return Ok(());
        }
        if w != cols || h != visual_rows {
            return Ok(());
        }

        let need = w.saturating_mul(h).saturating_mul(4);
        if frame.pixels_rgba.len() < need {
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

        out.write_all(b"\x1b[H\x1b[0m")?;
        out.write_all(b"\x1b[?7l")?;
        self.last_fg = None;

        // Dark -> bright ramp. Keep it ASCII-safe and compact.
        const RAMP: &[u8] = b" .,:;irsXA253hMHGS#9B&@";

        for y in 0..visual_rows {
            for x in 0..cols {
                let idx = (y * w + x) * 4;
                let r = frame.pixels_rgba[idx];
                let g = frame.pixels_rgba[idx + 1];
                let b = frame.pixels_rgba[idx + 2];

                let l = luma_u8(r, g, b) as usize;
                let ridx = l * (RAMP.len() - 1) / 255;
                let ch = RAMP[ridx] as char;

                let fg = (r, g, b);
                if self.last_fg != Some(fg) {
                    write!(out, "\x1b[38;2;{};{};{}m", fg.0, fg.1, fg.2)?;
                    self.last_fg = Some(fg);
                }
                write!(out, "{ch}")?;
            }
            out.write_all(b"\r\n")?;
        }

        // HUD lines (bottom area)
        let mut hud_lines = frame.hud.lines();
        for i in 0..(frame.hud_rows as usize) {
            write_hud_line(
                out,
                visual_rows + i + 1,
                cols,
                hud_lines.next(),
                frame.hud_highlight,
                frame.hud_highlight_phase,
            )?;
        }

        if let Some(text) = frame.overlay {
            draw_overlay_popup(out, frame.term_cols, frame.term_rows, text)?;
        }

        out.write_all(b"\x1b[?7h\x1b[0m")?;

        if frame.sync_updates {
            out.write_all(b"\x1b[?2026l")?;
        }
        out.flush()?;
        Ok(())
    }
}

#[inline]
fn luma_u8(r: u8, g: u8, b: u8) -> u8 {
    ((r as u32 * 54 + g as u32 * 183 + b as u32 * 19) >> 8) as u8
}
