use crate::audio::AudioFeatures;
use std::fmt::Write as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypographyMode {
    Off,
    LinePulse,
    WordPulse,
    GlyphFlow,
    MatrixPulse,
}

impl TypographyMode {
    pub fn all() -> [Self; 5] {
        [
            Self::Off,
            Self::LinePulse,
            Self::WordPulse,
            Self::GlyphFlow,
            Self::MatrixPulse,
        ]
    }

    pub fn cycle_non_off(self) -> Self {
        match self {
            Self::Off => Self::LinePulse,
            Self::LinePulse => Self::WordPulse,
            Self::WordPulse => Self::GlyphFlow,
            Self::GlyphFlow => Self::MatrixPulse,
            Self::MatrixPulse => Self::LinePulse,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::LinePulse => "line",
            Self::WordPulse => "word",
            Self::GlyphFlow => "glyph",
            Self::MatrixPulse => "matrix",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Off => 0,
            Self::LinePulse => 1,
            Self::WordPulse => 2,
            Self::GlyphFlow => 3,
            Self::MatrixPulse => 4,
        }
    }

    pub fn from_index(idx: usize) -> Self {
        Self::all().get(idx).copied().unwrap_or(Self::Off)
    }

    pub fn from_unit_interval(v: f32) -> Self {
        let v = v.clamp(0.0, 1.0);
        if v < 0.20 {
            Self::Off
        } else if v < 0.40 {
            Self::LinePulse
        } else if v < 0.60 {
            Self::WordPulse
        } else if v < 0.80 {
            Self::GlyphFlow
        } else {
            Self::MatrixPulse
        }
    }
}

pub fn typography_overlay_text(
    mode: TypographyMode,
    audio: &AudioFeatures,
    beat_pulse: f32,
    t: f32,
    lyric_line: Option<&str>,
    system_token: Option<&str>,
) -> Option<String> {
    let mut base = match mode {
        TypographyMode::Off => None,
        TypographyMode::LinePulse => {
            let level = ((audio.rms * 7.0 + beat_pulse * 4.0).round() as usize).min(10);
            let filled = "=".repeat(level);
            let empty = ".".repeat(10usize.saturating_sub(level));
            let cue = if audio.beat { "PULSE" } else { "FLOW" };
            Some(format!("{}{} {}", filled, empty, cue))
        }
        TypographyMode::WordPulse => {
            let fallback_words = ["BREATHE", "FLOW", "DRIFT", "LIFT", "FOCUS", "GLIDE"];
            let words = lyric_line
                .map(|line| {
                    normalize_typography_text(line, 56)
                        .split(' ')
                        .filter(|x| !x.is_empty())
                        .map(|x| x.to_string())
                        .collect::<Vec<_>>()
                })
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| fallback_words.iter().map(|x| x.to_string()).collect());
            let idx = ((t * 2.4).floor() as usize + if audio.beat { 1 } else { 0 }) % words.len();
            let mut out = String::new();
            for (i, word) in words.into_iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                if i == idx {
                    out.push('[');
                    out.push_str(&word);
                    out.push(']');
                } else {
                    out.push_str(&word);
                }
            }
            Some(out)
        }
        TypographyMode::GlyphFlow => {
            let sweep = ((t * 4.0).floor() as usize) % 10;
            let mut guide = String::from("..........\u{2192}");
            guide.replace_range(sweep..=sweep, "#");
            let stress = (audio.onset * 0.55 + audio.beat_strength * 0.30 + beat_pulse * 0.15)
                .clamp(0.0, 1.0);
            Some(format!(
                "GLYPH {} EDGE:{:>4.2} CENT:{:>4.2}",
                guide, stress, audio.centroid
            ))
        }
        TypographyMode::MatrixPulse => {
            let gate = ((t * 8.0 + audio.onset * 5.0).fract() > 0.72) as u8;
            let scan = ((t * 2.0).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let words = if gate == 1 {
                "TYPE STROBE"
            } else {
                "TYPE FLOW"
            };
            Some(format!(
                "{} | BIN {:03b}{:03b} | SCAN {:>4.2}",
                words,
                ((audio.beat_strength * 7.0).round() as i32).clamp(0, 7),
                ((beat_pulse * 7.0).round() as i32).clamp(0, 7),
                scan
            ))
        }
    }?;

    if let Some(lyric) = lyric_line {
        let text = normalize_typography_text(lyric, 44);
        if !text.is_empty() {
            let _ = write!(base, " | LYR: {}", text);
        }
    }

    if let Some(sys) = system_token {
        let text = normalize_typography_text(sys, 28);
        if !text.is_empty() {
            let _ = write!(base, " | SYS: {}", text);
        }
    }

    Some(base)
}

pub fn typography_reactive_audio(
    mode: TypographyMode,
    mut audio: AudioFeatures,
    beat_pulse: f32,
    t: f32,
) -> (AudioFeatures, f32) {
    let pulse = beat_pulse.clamp(0.0, 1.6);
    match mode {
        TypographyMode::Off => (audio, 1.0),
        TypographyMode::LinePulse => {
            let stripe = (audio.rms * 0.70 + pulse * 0.30).clamp(0.0, 1.0);
            audio.onset = (audio.onset * (1.0 + 0.32 * stripe) + 0.05 * pulse).clamp(0.0, 1.0);
            audio.beat_strength =
                (audio.beat_strength * (1.0 + 0.24 * stripe) + 0.06 * pulse).clamp(0.0, 1.0);
            for b in &mut audio.bands[2..6] {
                *b = (*b * (1.0 + 0.20 * stripe)).clamp(0.0, 1.0);
            }
            (audio, (1.0 + 0.08 * stripe + 0.05 * pulse).clamp(0.90, 1.35))
        }
        TypographyMode::WordPulse => {
            let cadence = ((t * 2.2).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let accent = if audio.beat { 1.0 } else { 0.0 };
            audio.onset = (audio.onset * (1.0 + 0.28 * cadence) + 0.08 * accent).clamp(0.0, 1.0);
            audio.beat_strength =
                (audio.beat_strength * (1.0 + 0.22 * cadence) + 0.05 * pulse).clamp(0.0, 1.0);
            for b in &mut audio.bands[1..5] {
                *b = (*b * (1.0 + 0.18 * cadence + 0.10 * accent)).clamp(0.0, 1.0);
            }
            (audio, (1.0 + 0.10 * cadence + 0.06 * accent).clamp(0.90, 1.45))
        }
        TypographyMode::GlyphFlow => {
            let glide = ((t * 0.63 + audio.centroid * 2.4).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let edge = (audio.centroid * 0.70 + audio.flatness * 0.60).clamp(0.0, 1.0);
            audio.rms = (audio.rms * (1.0 + 0.12 * edge)).clamp(0.0, 1.0);
            audio.onset = (audio.onset * (1.0 + 0.20 * edge) + 0.05 * glide).clamp(0.0, 1.0);
            audio.beat_strength =
                (audio.beat_strength * (1.0 + 0.16 * edge) + 0.04 * pulse).clamp(0.0, 1.0);
            for b in &mut audio.bands[3..8] {
                *b = (*b * (1.0 + 0.24 * glide + 0.18 * edge)).clamp(0.0, 1.0);
            }
            (audio, (1.0 + 0.14 * edge + 0.08 * glide).clamp(0.90, 1.55))
        }
        TypographyMode::MatrixPulse => {
            let scan = (t * 8.6 + audio.onset * 5.0 + audio.centroid * 2.0).fract();
            let gate = if scan > 0.76 { 1.0 } else { 0.0 };
            let glitch = (audio.onset * 0.65 + pulse * 0.35).clamp(0.0, 1.0);
            audio.onset = (audio.onset * (1.0 + 0.42 * glitch) + 0.14 * gate).clamp(0.0, 1.0);
            audio.beat_strength =
                (audio.beat_strength * (1.0 + 0.28 * glitch) + 0.10 * gate).clamp(0.0, 1.0);
            for b in &mut audio.bands[5..8] {
                *b = (*b * (1.0 + 0.45 * glitch + 0.20 * gate)).clamp(0.0, 1.0);
            }
            for b in &mut audio.bands[0..2] {
                *b = (*b * (1.0 - 0.12 * gate)).clamp(0.0, 1.0);
            }
            (audio, (1.0 + 0.18 * glitch + 0.10 * gate).clamp(0.90, 1.65))
        }
    }
}

pub fn apply_typography_overlay_pixels(
    mode: TypographyMode,
    pixels: &mut [u8],
    w: usize,
    h: usize,
    audio: &AudioFeatures,
    beat_pulse: f32,
    t: f32,
    lyric_line: Option<&str>,
    system_token: Option<&str>,
) {
    if mode == TypographyMode::Off || w == 0 || h == 0 || pixels.len() < w.saturating_mul(h).saturating_mul(4) {
        return;
    }

    let beat = beat_pulse.clamp(0.0, 1.6);
    match mode {
        TypographyMode::Off => {}
        TypographyMode::LinePulse => {
            let scale = (1.0 + audio.rms * 2.2 + beat * 1.3).clamp(1.0, 3.0).round() as i32;
            let phrase = "BROTVIZ";
            let phrase_w = text_pixel_width(phrase, scale);
            let spacing = (phrase_w + 8 * scale).max(1);
            let speed = 20.0 + audio.rms * 42.0 + beat * 30.0;
            let scroll = ((t * speed) as i32).rem_euclid(spacing);
            let base_y = (h as f32 * (0.20 + 0.08 * (t * 1.1).sin()) + audio.onset * h as f32 * 0.08) as i32;

            for lane in 0..3 {
                let y = base_y + lane * (7 * scale);
                let hue = (0.54 + lane as f32 * 0.11 + t * 0.07).fract();
                let (r, g, b) = hsv_to_rgb(hue, 0.82, 1.0);
                let alpha = (95.0 + 120.0 * (audio.rms * 0.7 + beat * 0.5).clamp(0.0, 1.0)) as u8;

                let mut x = -spacing - scroll;
                while x < w as i32 + spacing {
                    draw_text_3x5(pixels, w, h, phrase, x, y, scale, (r, g, b), alpha);
                    x += spacing;
                }
            }
        }
        TypographyMode::WordPulse => {
            let words = ["BREATHE", "FLOW", "DRIFT", "GLIDE", "FOCUS", "LIFT"];
            let beat_bump = if audio.beat { 1 } else { 0 };
            let idx = ((t * 1.5).floor() as usize + beat_bump) % words.len();
            let word = words[idx];
            let scale = (1.0 + audio.rms * 2.6 + beat * 2.0).clamp(1.0, 4.0).round() as i32;
            let wobble_x = (t * 1.35).sin() * w as f32 * 0.10;
            let wobble_y = (t * 1.10).cos() * h as f32 * 0.09;
            let text_w = text_pixel_width(word, scale);
            let x = ((w as i32 - text_w) / 2) + wobble_x as i32;
            let y = (h as f32 * 0.50 + wobble_y) as i32;

            let hue = (0.07 + t * 0.05 + audio.onset * 0.12).fract();
            let (r, g, b) = hsv_to_rgb(hue, 0.88, 1.0);
            let alpha = (110.0 + 125.0 * (audio.onset * 0.8 + beat * 0.5).clamp(0.0, 1.0)) as u8;

            draw_text_3x5(
                pixels,
                w,
                h,
                word,
                x + scale,
                y + scale,
                scale.max(1),
                (18, 14, 24),
                (alpha / 2).max(28),
            );
            draw_text_3x5(pixels, w, h, word, x, y, scale.max(1), (r, g, b), alpha);
        }
        TypographyMode::GlyphFlow => {
            let cx = w as f32 * 0.5;
            let cy = h as f32 * 0.5;
            let base_r = (w.min(h) as f32 * (0.19 + audio.rms * 0.20) + beat * 36.0).max(8.0);
            let count = 36usize;
            let glyphs = ['N', 'E', 'O', 'N', 'W', 'A', 'V', 'E', '#', '*', '+', ':'];
            for i in 0..count {
                let fi = i as f32;
                let ang = t * 0.63 + fi * std::f32::consts::TAU / count as f32 + audio.centroid * 1.8;
                let r = base_r + (fi * 1.7 + t * 1.9).sin() * base_r * 0.25;
                let x = (cx + ang.cos() * r) as i32;
                let y = (cy + (ang * 1.27).sin() * r * 0.72) as i32;
                let scale = if (i + (t * 4.0) as usize) % 5 == 0 { 2 } else { 1 };
                let hue = ((ang / std::f32::consts::TAU) + t * 0.09).fract();
                let (r8, g8, b8) = hsv_to_rgb(hue, 0.86, 1.0);
                let alpha = (88.0 + 120.0 * (audio.beat_strength * 0.7 + beat * 0.5).clamp(0.0, 1.0)) as u8;
                let ch = glyphs[(i + (t * 3.0) as usize) % glyphs.len()];
                draw_char_3x5(pixels, w, h, ch, x, y, scale, (r8, g8, b8), alpha);
            }
        }
        TypographyMode::MatrixPulse => {
            let scale = (1.0 + audio.beat_strength * 2.2 + beat * 1.4).clamp(1.0, 3.0).round() as i32;
            let cell = (4 * scale).max(4);
            let cols = ((w as i32 + cell - 1) / cell).max(1);
            let chars = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F'];
            for col in 0..cols {
                let seed = col as u32 ^ 0x9e37_79b9;
                let speed = 0.40 + hash01(seed) * 1.6 + audio.onset * 0.7;
                let phase = hash01(seed ^ 0x51ed_270b) * h as f32;
                let y_head = ((t * 65.0 * speed + phase) as i32).rem_euclid(h as i32);
                let x = col * cell;
                for trail in 0..4i32 {
                    let y = y_head - trail * (6 * scale);
                    if y < -16 || y > h as i32 + 16 {
                        continue;
                    }
                    let idx = (((t * 24.0) as usize) + col as usize * 7 + trail as usize * 3) % chars.len();
                    let ch = chars[idx];
                    let hue = (0.29 + hash01(seed.wrapping_add(trail as u32 * 13)) * 0.18).fract();
                    let (r8, g8, b8) = hsv_to_rgb(hue, 0.82, 1.0);
                    let trail_mul = 1.0 - (trail as f32 / 4.0);
                    let alpha = (70.0 + 170.0 * trail_mul * (0.5 + audio.rms * 0.5)) as u8;
                    draw_char_3x5(pixels, w, h, ch, x, y, scale, (r8, g8, b8), alpha);
                }
            }
        }
    }

    if let Some(line) = lyric_line {
        let phrase = normalize_typography_text(line, 40);
        if !phrase.is_empty() {
            let scale = (1.0 + audio.rms * 1.2 + beat * 0.8).clamp(1.0, 2.0).round() as i32;
            let y = (h as i32 - (7 * scale)).max(0);
            let x = ((w as i32 - text_pixel_width(&phrase, scale)) / 2).max(0);
            let hue = (0.10 + t * 0.03 + audio.beat_strength * 0.15).fract();
            let (r, g, b) = hsv_to_rgb(hue, 0.88, 1.0);
            draw_text_3x5(pixels, w, h, &phrase, x, y, scale, (10, 8, 18), 80);
            draw_text_3x5(pixels, w, h, &phrase, x, y, scale, (r, g, b), 150);
        }
    }

    if let Some(token) = system_token {
        let phrase = normalize_typography_text(token, 26);
        if !phrase.is_empty() {
            let scale = 1;
            let x = 2;
            let y = 2;
            let hue = (0.62 + t * 0.05 + audio.centroid * 0.20).fract();
            let (r, g, b) = hsv_to_rgb(hue, 0.82, 1.0);
            draw_text_3x5(pixels, w, h, &phrase, x, y, scale, (r, g, b), 120);
        }
    }
}

fn normalize_typography_text(input: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut prev_space = true;
    for ch in input.chars() {
        if out.chars().count() >= max_chars {
            break;
        }
        let mapped = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_uppercase()
        } else if matches!(ch, ' ' | '-' | '_' | '/' | ':' | '.' | '#' | '!' | '?') {
            ch
        } else {
            ' '
        };
        if mapped == ' ' {
            if prev_space {
                continue;
            }
            prev_space = true;
            out.push(' ');
        } else {
            prev_space = false;
            out.push(mapped);
        }
    }
    out.trim().to_string()
}

fn text_pixel_width(text: &str, scale: i32) -> i32 {
    let scale = scale.max(1);
    let count = text.chars().count() as i32;
    if count <= 0 {
        0
    } else {
        count * (4 * scale) - scale
    }
}

fn draw_text_3x5(
    pixels: &mut [u8],
    w: usize,
    h: usize,
    text: &str,
    x: i32,
    y: i32,
    scale: i32,
    color: (u8, u8, u8),
    alpha: u8,
) {
    let mut cursor_x = x;
    let step = (4 * scale.max(1)).max(1);
    for ch in text.chars() {
        draw_char_3x5(pixels, w, h, ch, cursor_x, y, scale, color, alpha);
        cursor_x += step;
    }
}

fn draw_char_3x5(
    pixels: &mut [u8],
    w: usize,
    h: usize,
    ch: char,
    x: i32,
    y: i32,
    scale: i32,
    color: (u8, u8, u8),
    alpha: u8,
) {
    let scale = scale.max(1);
    let rows = glyph_3x5(ch);
    for (ry, row_bits) in rows.iter().enumerate() {
        for rx in 0..3usize {
            if (row_bits & (1u8 << (2 - rx))) == 0 {
                continue;
            }
            let px = x + rx as i32 * scale;
            let py = y + ry as i32 * scale;
            for oy in 0..scale {
                for ox in 0..scale {
                    blend_add_rgb(
                        pixels,
                        w,
                        h,
                        px + ox,
                        py + oy,
                        color.0,
                        color.1,
                        color.2,
                        alpha,
                    );
                }
            }
        }
    }
}

fn glyph_3x5(ch: char) -> [u8; 5] {
    match ch.to_ascii_uppercase() {
        'A' => [0b111, 0b101, 0b111, 0b101, 0b101],
        'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        'C' => [0b111, 0b100, 0b100, 0b100, 0b111],
        'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
        'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
        'G' => [0b111, 0b100, 0b101, 0b101, 0b111],
        'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
        'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
        'J' => [0b001, 0b001, 0b001, 0b101, 0b111],
        'K' => [0b101, 0b101, 0b110, 0b101, 0b101],
        'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'M' => [0b101, 0b111, 0b111, 0b101, 0b101],
        'N' => [0b101, 0b111, 0b111, 0b111, 0b101],
        'O' => [0b111, 0b101, 0b101, 0b101, 0b111],
        'P' => [0b111, 0b101, 0b111, 0b100, 0b100],
        'Q' => [0b111, 0b101, 0b101, 0b111, 0b001],
        'R' => [0b111, 0b101, 0b111, 0b101, 0b101],
        'S' => [0b111, 0b100, 0b111, 0b001, 0b111],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'U' => [0b101, 0b101, 0b101, 0b101, 0b111],
        'V' => [0b101, 0b101, 0b101, 0b101, 0b010],
        'W' => [0b101, 0b101, 0b111, 0b111, 0b101],
        'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
        'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        'Z' => [0b111, 0b001, 0b010, 0b100, 0b111],
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b001, 0b001],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        ':' => [0b000, 0b010, 0b000, 0b010, 0b000],
        '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        '#' => [0b101, 0b111, 0b101, 0b111, 0b101],
        '*' => [0b101, 0b010, 0b111, 0b010, 0b101],
        ' ' => [0b000, 0b000, 0b000, 0b000, 0b000],
        _ => [0b111, 0b101, 0b111, 0b101, 0b111],
    }
}

fn blend_add_rgb(
    pixels: &mut [u8],
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    r: u8,
    g: u8,
    b: u8,
    alpha: u8,
) {
    if x < 0 || y < 0 {
        return;
    }
    let x = x as usize;
    let y = y as usize;
    if x >= w || y >= h {
        return;
    }
    let idx = (y * w + x) * 4;
    if idx + 3 >= pixels.len() {
        return;
    }
    let a = alpha as u16;
    let add_r = (r as u16 * a) / 255;
    let add_g = (g as u16 * a) / 255;
    let add_b = (b as u16 * a) / 255;
    pixels[idx] = pixels[idx].saturating_add(add_r as u8);
    pixels[idx + 1] = pixels[idx + 1].saturating_add(add_g as u8);
    pixels[idx + 2] = pixels[idx + 2].saturating_add(add_b as u8);
    pixels[idx + 3] = 255;
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let h = ((h % 1.0) + 1.0) % 1.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let i = (h * 6.0).floor() as i32;
    let f = h * 6.0 - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

fn hash01(x: u32) -> f32 {
    let mut v = x;
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846c_a68b);
    v ^= v >> 16;
    (v as f32) / (u32::MAX as f32)
}
