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
