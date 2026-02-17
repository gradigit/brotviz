use crate::render::{
    luma_u16, text_frame_begin, text_frame_end, write_bg_rgb, write_fg_rgb, Frame, Renderer,
};
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
        let Some((cols, visual_rows, w, _h)) = text_frame_begin(frame, 2, 4, out)? else {
            return Ok(());
        };

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
                    write_fg_rgb(out, fgc.0, fgc.1, fgc.2)?;
                    self.last_fg = Some(fgc);
                }
                if self.last_bg != Some(bgc) {
                    write_bg_rgb(out, bgc.0, bgc.1, bgc.2)?;
                    self.last_bg = Some(bgc);
                }

                let mut ch_buf = [0u8; 4];
                let ch_str = ch.encode_utf8(&mut ch_buf);
                out.write_all(ch_str.as_bytes())?;
            }
            out.write_all(b"\r\n")?;
        }

        text_frame_end(frame, cols, visual_rows, out)
    }
}
