mod halfblock;
mod braille;
mod sextant;
mod ascii;
mod kitty;

pub use halfblock::HalfBlockRenderer;
pub use braille::BrailleRenderer;
pub use sextant::SextantRenderer;
pub use ascii::AsciiRenderer;
pub use kitty::KittyRenderer;

use std::io::Write;

pub struct Frame<'a> {
    pub term_cols: u16,
    pub term_rows: u16,
    pub visual_rows: u16,
    pub pixel_width: usize,
    pub pixel_height: usize,
    pub pixels_rgba: &'a [u8],
    pub hud: &'a str,
    pub hud_rows: u16,
    pub hud_highlight: Option<&'a str>,
    pub hud_highlight_phase: bool,
    pub overlay: Option<&'a str>,
    pub sync_updates: bool,
}

pub trait Renderer {
    fn name(&self) -> &'static str;
    fn render(&mut self, frame: &Frame<'_>, out: &mut dyn Write) -> anyhow::Result<()>;
}

pub fn draw_overlay_popup(
    out: &mut dyn Write,
    term_cols: u16,
    term_rows: u16,
    text: &str,
) -> anyhow::Result<()> {
    if text.trim().is_empty() {
        return Ok(());
    }

    let cols = term_cols as usize;
    let rows = term_rows as usize;
    if cols < 8 || rows < 4 {
        return Ok(());
    }

    let max_inner_w = cols.saturating_sub(6).max(1);
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines() {
        if raw.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut cur = String::new();
        let mut cur_len = 0usize;
        for ch in raw.chars() {
            cur.push(ch);
            cur_len += 1;
            if cur_len >= max_inner_w {
                lines.push(cur);
                cur = String::new();
                cur_len = 0;
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
    }
    if lines.is_empty() {
        return Ok(());
    }

    let mut inner_w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    inner_w = inner_w.min(max_inner_w).max(1);

    let box_w = (inner_w + 4).min(cols.saturating_sub(2)).max(4);
    let inner_w = box_w.saturating_sub(4);
    let max_body = rows.saturating_sub(3).max(1);
    let body_h = lines.len().min(max_body);
    let box_h = (body_h + 2).min(rows.saturating_sub(1)).max(3);

    let start_col = (cols.saturating_sub(box_w)) / 2 + 1;
    let start_row = (rows.saturating_sub(box_h)) / 2 + 1;

    let horiz = "-".repeat(box_w.saturating_sub(2));
    let blank = " ".repeat(inner_w);
    // Full-screen high-contrast backdrop so help text stays readable over bright visuals.
    // Use EL2 (`2K`) instead of writing `cols` spaces to avoid edge-wrap artifacts.
    out.write_all(b"\x1b[0m\x1b[38;2;220;228;242m\x1b[48;2;2;4;10m")?;
    for row in 1..=rows {
        write!(out, "\x1b[{};1H\x1b[2K", row)?;
    }

    // Popup box.
    out.write_all(b"\x1b[0m\x1b[38;2;236;242;255m\x1b[48;2;10;14;24m")?;
    write!(out, "\x1b[{};{}H+{}+", start_row, start_col, horiz)?;

    for i in 0..body_h {
        let row = start_row + 1 + i;
        write!(out, "\x1b[{};{}H| {} |", row, start_col, blank)?;
        let line = &lines[i];
        if i == 0 {
            write!(
                out,
                "\x1b[{};{}H\x1b[1m\x1b[38;2;255;236;160m{}\x1b[22m\x1b[38;2;236;242;255m",
                row,
                start_col + 2,
                line
            )?;
        } else {
            write!(out, "\x1b[{};{}H{}", row, start_col + 2, line)?;
        }
    }

    write!(out, "\x1b[{};{}H+{}+", start_row + box_h - 1, start_col, horiz)?;
    out.write_all(b"\x1b[0m")?;
    Ok(())
}

pub fn write_hud_line(
    out: &mut dyn Write,
    row: usize,
    cols: usize,
    line: Option<&str>,
    highlight_keyword: Option<&str>,
    highlight_phase: bool,
) -> anyhow::Result<()> {
    write!(out, "\x1b[{};1H\x1b[0m\x1b[2K", row)?;
    let Some(line) = line else {
        return Ok(());
    };

    let clipped = clip_to_cols(line, cols);
    let keyword = highlight_keyword.map(str::trim).filter(|k| !k.is_empty());
    let Some(keyword) = keyword else {
        out.write_all(b"\x1b[0m")?;
        write!(out, "{}", clipped)?;
        out.write_all(b"\x1b[0m")?;
        return Ok(());
    };

    let segments = clipped.split(" | ").collect::<Vec<_>>();
    let has_match = segments
        .iter()
        .any(|seg| seg.trim_start().starts_with(keyword));
    if !has_match {
        out.write_all(b"\x1b[0m")?;
        write!(out, "{}", clipped)?;
        out.write_all(b"\x1b[0m")?;
        return Ok(());
    }

    let mut matched = false;
    let mut first = true;
    for seg in segments {
        if !first {
            out.write_all(b"\x1b[0m | ")?;
        }
        first = false;
        let highlight_seg = !matched && seg.trim_start().starts_with(keyword);
        if highlight_seg {
            matched = true;
            if highlight_phase {
                out.write_all(b"\x1b[1;38;2;255;244;176m\x1b[48;2;64;46;10m")?;
            } else {
                out.write_all(b"\x1b[1;38;2;255;236;170m\x1b[48;2;48;32;8m")?;
            }
            write!(out, "{}", seg)?;
            out.write_all(b"\x1b[0m")?;
        } else {
            write!(out, "{}", seg)?;
        }
    }
    out.write_all(b"\x1b[0m")?;
    Ok(())
}

/// Write a u8 value as decimal digits into `buf` starting at `pos`. Returns new position.
#[inline(always)]
fn write_u8_digits(buf: &mut [u8], mut pos: usize, val: u8) -> usize {
    if val >= 100 {
        buf[pos] = b'0' + val / 100;
        pos += 1;
        buf[pos] = b'0' + (val / 10) % 10;
        pos += 1;
        buf[pos] = b'0' + val % 10;
        pos += 1;
    } else if val >= 10 {
        buf[pos] = b'0' + val / 10;
        pos += 1;
        buf[pos] = b'0' + val % 10;
        pos += 1;
    } else {
        buf[pos] = b'0' + val;
        pos += 1;
    }
    pos
}

/// Write `\x1b[38;2;R;G;Bm` (foreground) into a stack buffer and flush to `out`.
/// Avoids `write!()` formatting overhead in the hot render loop.
#[inline]
pub(crate) fn write_fg_rgb(out: &mut dyn Write, r: u8, g: u8, b: u8) -> std::io::Result<()> {
    let mut buf = [0u8; 24]; // max: \x1b[38;2;255;255;255m = 19 bytes
    buf[0] = 0x1b;
    buf[1] = b'[';
    buf[2] = b'3';
    buf[3] = b'8';
    buf[4] = b';';
    buf[5] = b'2';
    buf[6] = b';';
    let mut pos = 7;
    pos = write_u8_digits(&mut buf, pos, r);
    buf[pos] = b';';
    pos += 1;
    pos = write_u8_digits(&mut buf, pos, g);
    buf[pos] = b';';
    pos += 1;
    pos = write_u8_digits(&mut buf, pos, b);
    buf[pos] = b'm';
    pos += 1;
    out.write_all(&buf[..pos])
}

/// Write `\x1b[48;2;R;G;Bm` (background) into a stack buffer and flush to `out`.
#[inline]
pub(crate) fn write_bg_rgb(out: &mut dyn Write, r: u8, g: u8, b: u8) -> std::io::Result<()> {
    let mut buf = [0u8; 24];
    buf[0] = 0x1b;
    buf[1] = b'[';
    buf[2] = b'4';
    buf[3] = b'8';
    buf[4] = b';';
    buf[5] = b'2';
    buf[6] = b';';
    let mut pos = 7;
    pos = write_u8_digits(&mut buf, pos, r);
    buf[pos] = b';';
    pos += 1;
    pos = write_u8_digits(&mut buf, pos, g);
    buf[pos] = b';';
    pos += 1;
    pos = write_u8_digits(&mut buf, pos, b);
    buf[pos] = b'm';
    pos += 1;
    out.write_all(&buf[..pos])
}

// ── Shared luma helpers ─────────────────────────────────────────────────────

#[inline]
pub(crate) fn luma_u16(r: u8, g: u8, b: u8) -> u16 {
    // Approx Rec.709 luma using integer math (0..255).
    ((r as u32 * 54 + g as u32 * 183 + b as u32 * 19) >> 8) as u16
}

#[inline]
pub(crate) fn luma_u8(r: u8, g: u8, b: u8) -> u8 {
    ((r as u32 * 54 + g as u32 * 183 + b as u32 * 19) >> 8) as u8
}

// ── Shared text-mode renderer boilerplate ───────────────────────────────────

/// Validate frame dimensions and write the text-mode preamble
/// (sync-update begin, cursor home, SGR reset, disable autowrap).
///
/// `col_mul` / `row_mul` express how many pixels each terminal cell spans
/// (e.g. halfblock = 1×2, braille = 2×4, ascii = 1×1).
///
/// Returns `Ok(Some((cols, visual_rows, w, h)))` when the caller should proceed
/// to paint cells, or `Ok(None)` when the frame must be skipped (zero size,
/// dimension mismatch, or undersized pixel buffer — the latter is reported to
/// the terminal automatically).
pub(crate) fn text_frame_begin(
    frame: &Frame<'_>,
    col_mul: usize,
    row_mul: usize,
    out: &mut dyn Write,
) -> anyhow::Result<Option<(usize, usize, usize, usize)>> {
    let cols = frame.term_cols as usize;
    let visual_rows = frame.visual_rows as usize;
    let w = frame.pixel_width;
    let h = frame.pixel_height;

    if cols == 0 || visual_rows == 0 || w == 0 || h == 0 {
        return Ok(None);
    }
    if w != cols.saturating_mul(col_mul) || h != visual_rows.saturating_mul(row_mul) {
        return Ok(None);
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
        return Ok(None);
    }

    if frame.sync_updates {
        out.write_all(b"\x1b[?2026h")?;
    }
    out.write_all(b"\x1b[H\x1b[0m")?;
    out.write_all(b"\x1b[?7l")?;
    Ok(Some((cols, visual_rows, w, h)))
}

/// Write HUD lines, overlay popup, restore autowrap, sync-update end, and flush.
pub(crate) fn text_frame_end(
    frame: &Frame<'_>,
    cols: usize,
    visual_rows: usize,
    out: &mut dyn Write,
) -> anyhow::Result<()> {
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

fn clip_to_cols(s: &str, cols: usize) -> String {
    if cols == 0 {
        return String::new();
    }
    let mut out = String::new();
    for ch in s.chars().take(cols) {
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame<'a>(
        cols: u16,
        visual_rows: u16,
        pw: usize,
        ph: usize,
        pixels: &'a [u8],
    ) -> Frame<'a> {
        Frame {
            term_cols: cols,
            term_rows: visual_rows + 2,
            visual_rows,
            pixel_width: pw,
            pixel_height: ph,
            pixels_rgba: pixels,
            hud: "FPS 60 | RMS 0.42",
            hud_rows: 1,
            hud_highlight: None,
            hud_highlight_phase: false,
            overlay: None,
            sync_updates: false,
        }
    }

    // ── luma helpers ────────────────────────────────────────────────────

    #[test]
    fn luma_black_is_zero() {
        assert_eq!(luma_u16(0, 0, 0), 0);
        assert_eq!(luma_u8(0, 0, 0), 0);
    }

    #[test]
    fn luma_white_is_near_max() {
        let l16 = luma_u16(255, 255, 255);
        assert!(l16 >= 250 && l16 <= 255, "got {l16}");
        let l8 = luma_u8(255, 255, 255);
        assert!(l8 >= 250, "got {l8}");
    }

    #[test]
    fn luma_green_dominates() {
        let green = luma_u16(0, 255, 0);
        let red = luma_u16(255, 0, 0);
        let blue = luma_u16(0, 0, 255);
        assert!(green > red, "green {green} should exceed red {red}");
        assert!(green > blue, "green {green} should exceed blue {blue}");
    }

    #[test]
    fn luma_u8_matches_u16_truncation() {
        for (r, g, b) in [(128, 64, 200), (0, 255, 0), (255, 128, 0)] {
            assert_eq!(luma_u8(r, g, b), luma_u16(r, g, b) as u8);
        }
    }

    // ── write_fg_rgb / write_bg_rgb ─────────────────────────────────────

    #[test]
    fn write_fg_rgb_produces_correct_escape() {
        let mut buf = Vec::new();
        write_fg_rgb(&mut buf, 1, 128, 255).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s, "\x1b[38;2;1;128;255m");
    }

    #[test]
    fn write_bg_rgb_produces_correct_escape() {
        let mut buf = Vec::new();
        write_bg_rgb(&mut buf, 0, 0, 0).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s, "\x1b[48;2;0;0;0m");
    }

    #[test]
    fn write_fg_rgb_single_digit_values() {
        let mut buf = Vec::new();
        write_fg_rgb(&mut buf, 5, 9, 0).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s, "\x1b[38;2;5;9;0m");
    }

    #[test]
    fn write_fg_rgb_max_values() {
        let mut buf = Vec::new();
        write_fg_rgb(&mut buf, 255, 255, 255).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s, "\x1b[38;2;255;255;255m");
    }

    // ── text_frame_begin ────────────────────────────────────────────────

    #[test]
    fn text_frame_begin_rejects_zero_cols() {
        let pixels = vec![0u8; 4];
        let frame = make_frame(0, 1, 0, 1, &pixels);
        let mut out = Vec::new();
        let result = text_frame_begin(&frame, 1, 1, &mut out).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn text_frame_begin_rejects_zero_visual_rows() {
        let pixels = vec![0u8; 4];
        let frame = make_frame(1, 0, 1, 0, &pixels);
        let mut out = Vec::new();
        let result = text_frame_begin(&frame, 1, 1, &mut out).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn text_frame_begin_rejects_dimension_mismatch() {
        // halfblock expects w=cols, h=rows*2. Give wrong pixel dims.
        let pixels = vec![0u8; 4 * 4 * 4];
        let frame = make_frame(4, 4, 4, 4, &pixels);
        let mut out = Vec::new();
        // col_mul=1, row_mul=2 => expect w=4, h=8 but pixel_height=4
        let result = text_frame_begin(&frame, 1, 2, &mut out).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn text_frame_begin_reports_undersized_buffer() {
        // Frame expects 4x2 = 8 pixels (32 bytes), but give only 16.
        let pixels = vec![0u8; 16];
        let frame = make_frame(4, 1, 4, 2, &pixels);
        let mut out = Vec::new();
        let result = text_frame_begin(&frame, 1, 2, &mut out).unwrap();
        assert!(result.is_none());
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("pixel buffer too small"), "got: {s}");
    }

    #[test]
    fn text_frame_begin_succeeds_for_valid_frame() {
        // ascii: col_mul=1, row_mul=1 => w=cols=4, h=rows=2
        let pixels = vec![0u8; 4 * 2 * 4];
        let frame = make_frame(4, 2, 4, 2, &pixels);
        let mut out = Vec::new();
        let result = text_frame_begin(&frame, 1, 1, &mut out).unwrap();
        assert_eq!(result, Some((4, 2, 4, 2)));
        let s = String::from_utf8_lossy(&out);
        // Should contain home+reset and autowrap-off
        assert!(s.contains("\x1b[H\x1b[0m"), "missing home+reset");
        assert!(s.contains("\x1b[?7l"), "missing autowrap-off");
    }

    #[test]
    fn text_frame_begin_writes_sync_when_enabled() {
        let pixels = vec![0u8; 4 * 2 * 4];
        let mut frame = make_frame(4, 2, 4, 2, &pixels);
        frame.sync_updates = true;
        let mut out = Vec::new();
        let _ = text_frame_begin(&frame, 1, 1, &mut out).unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("\x1b[?2026h"), "missing sync-begin");
    }

    #[test]
    fn text_frame_begin_no_sync_when_disabled() {
        let pixels = vec![0u8; 4 * 2 * 4];
        let frame = make_frame(4, 2, 4, 2, &pixels);
        let mut out = Vec::new();
        let _ = text_frame_begin(&frame, 1, 1, &mut out).unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(!s.contains("\x1b[?2026h"), "sync should not appear");
    }

    // ── text_frame_end ──────────────────────────────────────────────────

    #[test]
    fn text_frame_end_writes_hud_and_epilogue() {
        let pixels = vec![0u8; 4 * 2 * 4];
        let frame = make_frame(4, 2, 4, 2, &pixels);
        let mut out = Vec::new();
        text_frame_end(&frame, 4, 2, &mut out).unwrap();
        let s = String::from_utf8_lossy(&out);
        // Should contain autowrap restore + SGR reset
        assert!(s.contains("\x1b[?7h"), "missing autowrap-on");
        assert!(s.contains("\x1b[0m"), "missing SGR reset");
        // HUD line should be addressed (row 3 = visual_rows + 1)
        assert!(s.contains("\x1b[3;1H"), "missing HUD cursor position");
    }

    #[test]
    fn text_frame_end_writes_sync_when_enabled() {
        let pixels = vec![0u8; 4 * 2 * 4];
        let mut frame = make_frame(4, 2, 4, 2, &pixels);
        frame.sync_updates = true;
        let mut out = Vec::new();
        text_frame_end(&frame, 4, 2, &mut out).unwrap();
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("\x1b[?2026l"), "missing sync-end");
    }

    #[test]
    fn text_frame_end_draws_overlay_when_present() {
        let pixels = vec![0u8; 4 * 20 * 4];
        let mut frame = make_frame(20, 10, 20, 10, &pixels);
        frame.term_rows = 12;
        frame.overlay = Some("Help\nLine2");
        let mut out = Vec::new();
        text_frame_end(&frame, 20, 10, &mut out).unwrap();
        let s = String::from_utf8_lossy(&out);
        // The overlay popup uses + for corners
        assert!(s.contains("Help"), "overlay text missing");
    }

    // ── clip_to_cols ────────────────────────────────────────────────────

    #[test]
    fn clip_to_cols_truncates() {
        assert_eq!(clip_to_cols("abcdef", 3), "abc");
    }

    #[test]
    fn clip_to_cols_no_truncation_needed() {
        assert_eq!(clip_to_cols("ab", 10), "ab");
    }

    #[test]
    fn clip_to_cols_zero() {
        assert_eq!(clip_to_cols("anything", 0), "");
    }
}
