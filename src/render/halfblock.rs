use crate::render::{text_frame_begin, text_frame_end, write_bg_rgb, write_fg_rgb, Frame, Renderer};
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
        let Some((cols, visual_rows, w, _h)) = text_frame_begin(frame, 1, 2, out)? else {
            return Ok(());
        };

        self.last_fg = None;
        self.last_bg = None;

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
                    write_fg_rgb(out, tr, tg, tb)?;
                    self.last_fg = Some((tr, tg, tb));
                }
                if self.last_bg != Some((br, bg, bb)) {
                    write_bg_rgb(out, br, bg, bb)?;
                    self.last_bg = Some((br, bg, bb));
                }
                out.write_all("\u{2580}".as_bytes())?;
            }
            out.write_all(b"\r\n")?;
        }

        text_frame_end(frame, cols, visual_rows, out)
    }
}
