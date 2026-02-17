use tui_visualizer::render::{
    AsciiRenderer, BrailleRenderer, Frame, HalfBlockRenderer, Renderer, SextantRenderer,
};

/// Build a solid-color RGBA pixel buffer.
fn solid_pixels(w: usize, h: usize, r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut buf = vec![0u8; w * h * 4];
    for px in buf.chunks_exact_mut(4) {
        px[0] = r;
        px[1] = g;
        px[2] = b;
        px[3] = 255;
    }
    buf
}

/// Build a gradient pixel buffer (varies across x).
fn gradient_pixels(w: usize, h: usize) -> Vec<u8> {
    let mut buf = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let t = (x as f32 / w.max(1) as f32 * 255.0) as u8;
            buf[i] = t;
            buf[i + 1] = 128;
            buf[i + 2] = 255 - t;
            buf[i + 3] = 255;
        }
    }
    buf
}

fn make_frame<'a>(
    cols: u16,
    visual_rows: u16,
    pw: usize,
    ph: usize,
    pixels: &'a [u8],
    sync: bool,
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
        sync_updates: sync,
    }
}

// ── ASCII renderer ──────────────────────────────────────────────────────────

#[test]
fn ascii_renders_solid_frame() {
    let cols = 10u16;
    let rows = 5u16;
    let pixels = solid_pixels(cols as usize, rows as usize, 200, 200, 200);
    let frame = make_frame(cols, rows, cols as usize, rows as usize, &pixels, false);
    let mut out = Vec::new();
    let mut renderer = AsciiRenderer::new();
    renderer.render(&frame, &mut out).unwrap();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[H"), "missing home cursor");
    assert!(s.contains("\x1b[?7l"), "missing autowrap-off");
    assert!(s.contains("\x1b[?7h"), "missing autowrap-on");
    // Should have FG color escapes for 200,200,200
    assert!(s.contains("38;2;200;200;200"), "missing FG color");
    // HUD should be present
    assert!(s.contains("FPS 60"), "HUD text missing");
}

#[test]
fn ascii_name() {
    assert_eq!(AsciiRenderer::new().name(), "ascii");
}

#[test]
fn ascii_skips_zero_size() {
    let pixels = solid_pixels(1, 1, 0, 0, 0);
    let frame = make_frame(0, 0, 0, 0, &pixels, false);
    let mut out = Vec::new();
    AsciiRenderer::new().render(&frame, &mut out).unwrap();
    assert!(out.is_empty(), "expected empty output for zero-size frame");
}

// ── HalfBlock renderer ─────────────────────────────────────────────────────

#[test]
fn halfblock_renders_gradient_frame() {
    let cols = 8u16;
    let rows = 4u16;
    let pw = cols as usize;
    let ph = (rows as usize) * 2;
    let pixels = gradient_pixels(pw, ph);
    let frame = make_frame(cols, rows, pw, ph, &pixels, true);
    let mut out = Vec::new();
    let mut renderer = HalfBlockRenderer::new();
    renderer.render(&frame, &mut out).unwrap();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("\x1b[?2026h"), "missing sync-begin");
    assert!(s.contains("\x1b[?2026l"), "missing sync-end");
    // Should use upper-half-block character
    assert!(s.contains("\u{2580}"), "missing half-block char");
    // Should have both FG and BG colors
    assert!(s.contains("38;2;"), "missing FG escape");
    assert!(s.contains("48;2;"), "missing BG escape");
}

#[test]
fn halfblock_name() {
    assert_eq!(HalfBlockRenderer::new().name(), "halfblock");
}

#[test]
fn halfblock_skips_dimension_mismatch() {
    // pixel_height should be visual_rows*2, but give visual_rows*1
    let cols = 4u16;
    let rows = 4u16;
    let pixels = solid_pixels(4, 4, 100, 100, 100);
    let frame = make_frame(cols, rows, 4, 4, &pixels, false);
    let mut out = Vec::new();
    HalfBlockRenderer::new().render(&frame, &mut out).unwrap();
    assert!(out.is_empty(), "expected empty output for dimension mismatch");
}

// ── Braille renderer ────────────────────────────────────────────────────────

#[test]
fn braille_renders_gradient_frame() {
    let cols = 6u16;
    let rows = 3u16;
    let pw = (cols as usize) * 2;
    let ph = (rows as usize) * 4;
    let pixels = gradient_pixels(pw, ph);
    let frame = make_frame(cols, rows, pw, ph, &pixels, false);
    let mut out = Vec::new();
    let mut renderer = BrailleRenderer::new();
    renderer.render(&frame, &mut out).unwrap();
    let s = String::from_utf8_lossy(&out);
    // Braille characters are in U+2800..U+28FF range
    assert!(
        s.chars().any(|c| ('\u{2800}'..='\u{28FF}').contains(&c) || c == ' '),
        "no braille characters found"
    );
    assert!(s.contains("FPS 60"), "HUD text missing");
}

#[test]
fn braille_name() {
    assert_eq!(BrailleRenderer::new().name(), "braille");
}

// ── Sextant renderer ────────────────────────────────────────────────────────

#[test]
fn sextant_renders_gradient_frame() {
    let cols = 6u16;
    let rows = 3u16;
    let pw = (cols as usize) * 2;
    let ph = (rows as usize) * 3;
    let pixels = gradient_pixels(pw, ph);
    let frame = make_frame(cols, rows, pw, ph, &pixels, false);
    let mut out = Vec::new();
    let mut renderer = SextantRenderer::new();
    renderer.render(&frame, &mut out).unwrap();
    let s = String::from_utf8_lossy(&out);
    assert!(!out.is_empty(), "output should not be empty");
    assert!(s.contains("FPS 60"), "HUD text missing");
}

#[test]
fn sextant_name() {
    assert_eq!(SextantRenderer::new().name(), "sextant");
}

// ── Overlay rendering ───────────────────────────────────────────────────────

#[test]
fn ascii_renders_overlay_popup() {
    let cols = 40u16;
    let rows = 20u16;
    let pixels = solid_pixels(cols as usize, rows as usize, 50, 50, 50);
    let mut frame = make_frame(cols, rows, cols as usize, rows as usize, &pixels, false);
    frame.term_rows = rows + 2;
    frame.overlay = Some("Test Overlay\nSecond line");
    let mut out = Vec::new();
    let mut renderer = AsciiRenderer::new();
    renderer.render(&frame, &mut out).unwrap();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("Test Overlay"), "overlay text missing");
}

// ── Multiple frames (color cache reset) ─────────────────────────────────────

#[test]
fn halfblock_resets_color_cache_each_frame() {
    let cols = 4u16;
    let rows = 2u16;
    let pw = 4;
    let ph = 4;

    // Frame 1: red
    let pixels1 = solid_pixels(pw, ph, 255, 0, 0);
    let frame1 = make_frame(cols, rows, pw, ph, &pixels1, false);
    let mut out1 = Vec::new();
    let mut renderer = HalfBlockRenderer::new();
    renderer.render(&frame1, &mut out1).unwrap();
    let s1 = String::from_utf8_lossy(&out1);
    assert!(s1.contains("38;2;255;0;0"), "first frame missing red FG");

    // Frame 2: blue - color cache should reset so new color is emitted
    let pixels2 = solid_pixels(pw, ph, 0, 0, 255);
    let frame2 = make_frame(cols, rows, pw, ph, &pixels2, false);
    let mut out2 = Vec::new();
    renderer.render(&frame2, &mut out2).unwrap();
    let s2 = String::from_utf8_lossy(&out2);
    assert!(s2.contains("38;2;0;0;255"), "second frame missing blue FG");
}

// ── HUD highlight ───────────────────────────────────────────────────────────

#[test]
fn hud_highlight_appears_in_output() {
    let cols = 40u16;
    let rows = 5u16;
    let pixels = solid_pixels(cols as usize, rows as usize, 30, 30, 30);
    let mut frame = make_frame(cols, rows, cols as usize, rows as usize, &pixels, false);
    frame.hud = "FPS 60 | RMS 0.42";
    frame.hud_highlight = Some("FPS");
    frame.hud_highlight_phase = true;
    let mut out = Vec::new();
    let mut renderer = AsciiRenderer::new();
    renderer.render(&frame, &mut out).unwrap();
    let s = String::from_utf8_lossy(&out);
    // Highlighted segment should have bold + color escape before "FPS"
    assert!(s.contains("FPS"), "HUD keyword missing");
    // Should have the highlight color (bright yellow variant)
    assert!(s.contains("255;244;176"), "highlight color missing");
}
