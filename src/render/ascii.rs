use crate::render::{luma_u8, text_frame_begin, text_frame_end, write_fg_rgb, Frame, Renderer};
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
        let Some((cols, visual_rows, w, _h)) = text_frame_begin(frame, 1, 1, out)? else {
            return Ok(());
        };

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
                    write_fg_rgb(out, fg.0, fg.1, fg.2)?;
                    self.last_fg = Some(fg);
                }
                out.write_all(&[ch as u8])?;
            }
            out.write_all(b"\r\n")?;
        }

        text_frame_end(frame, cols, visual_rows, out)
    }
}
