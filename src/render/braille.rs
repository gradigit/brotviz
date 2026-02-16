use crate::render::{draw_overlay_popup, write_hud_line, Frame, Renderer};
use std::io::Write;

pub struct BrailleRenderer {
    last_fg: Option<(u8, u8, u8)>,
    last_bg: Option<(u8, u8, u8)>,
}

impl BrailleRenderer {
    pub fn new() -> Self {
        Self {
            last_fg: None,
            last_bg: None,
        }
    }
}

impl Renderer for BrailleRenderer {
    fn name(&self) -> &'static str {
        "braille"
    }

    fn render(&mut self, frame: &Frame<'_>, out: &mut dyn Write) -> anyhow::Result<()> {
        let cols = frame.term_cols as usize;
        let visual_rows = frame.visual_rows as usize;
        let w = frame.pixel_width;
        let h = frame.pixel_height;

        if cols == 0 || visual_rows == 0 || w == 0 || h == 0 {
            return Ok(());
        }
        if w != cols.saturating_mul(2) || h != visual_rows.saturating_mul(4) {
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
        // Disable autowrap while painting full-width rows.
        out.write_all(b"\x1b[?7l")?;
        self.last_fg = None;
        self.last_bg = None;

        const DOT_BITS: [u8; 8] = [0x01, 0x08, 0x02, 0x10, 0x04, 0x20, 0x40, 0x80];

        for row in 0..visual_rows {
            let base_y = row * 4;
            for col in 0..cols {
                let base_x = col * 2;

                let mut lum = [0u16; 8];
                let mut rgb = [(0u8, 0u8, 0u8); 8];

                for dy in 0..4usize {
                    for dx in 0..2usize {
                        let i = dy * 2 + dx;
                        let px = base_x + dx;
                        let py = base_y + dy;
                        let idx = (py * w + px) * 4;
                        let r = frame.pixels_rgba[idx];
                        let g = frame.pixels_rgba[idx + 1];
                        let b = frame.pixels_rgba[idx + 2];
                        rgb[i] = (r, g, b);
                        lum[i] = luma_u16(r, g, b);
                    }
                }

                let mut min_l = lum[0];
                let mut max_l = lum[0];
                for &v in lum.iter().skip(1) {
                    if v < min_l {
                        min_l = v;
                    }
                    if v > max_l {
                        max_l = v;
                    }
                }
                let thr = (min_l + max_l) / 2;

                let mut bits: u8 = 0;
                let mut fr: u32 = 0;
                let mut fg: u32 = 0;
                let mut fb: u32 = 0;
                let mut fc: u32 = 0;
                let mut br: u32 = 0;
                let mut bg: u32 = 0;
                let mut bb: u32 = 0;
                let mut bc: u32 = 0;

                for i in 0..8usize {
                    let (r, g, b) = rgb[i];
                    if lum[i] > thr {
                        bits |= DOT_BITS[i];
                        fr += r as u32;
                        fg += g as u32;
                        fb += b as u32;
                        fc += 1;
                    } else {
                        br += r as u32;
                        bg += g as u32;
                        bb += b as u32;
                        bc += 1;
                    }
                }

                // Choose fg/bg from "on" and "off" pixels.
                let (fgc, bgc, ch) = if bits == 0 {
                    let (r, g, b) = if bc > 0 {
                        (
                            (br / bc) as u8,
                            (bg / bc) as u8,
                            (bb / bc) as u8,
                        )
                    } else {
                        (0, 0, 0)
                    };
                    ((r, g, b), (r, g, b), ' ')
                } else {
                    let fgc = if fc > 0 {
                        ((fr / fc) as u8, (fg / fc) as u8, (fb / fc) as u8)
                    } else {
                        (0, 0, 0)
                    };
                    let bgc = if bc > 0 {
                        ((br / bc) as u8, (bg / bc) as u8, (bb / bc) as u8)
                    } else {
                        fgc
                    };
                    let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
                    (fgc, bgc, ch)
                };

                if self.last_fg != Some(fgc) {
                    write!(out, "\x1b[38;2;{};{};{}m", fgc.0, fgc.1, fgc.2)?;
                    self.last_fg = Some(fgc);
                }
                if self.last_bg != Some(bgc) {
                    write!(out, "\x1b[48;2;{};{};{}m", bgc.0, bgc.1, bgc.2)?;
                    self.last_bg = Some(bgc);
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

        // Restore autowrap.
        out.write_all(b"\x1b[?7h")?;

        if frame.sync_updates {
            out.write_all(b"\x1b[?2026l")?;
        }
        out.flush()?;
        Ok(())
    }
}

#[inline]
fn luma_u16(r: u8, g: u8, b: u8) -> u16 {
    // Approx Rec.709 luma using integer math (0..255).
    let y = (r as u32 * 54 + g as u32 * 183 + b as u32 * 19) >> 8;
    y as u16
}
