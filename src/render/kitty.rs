use crate::render::{draw_overlay_popup, write_hud_line, Frame, Renderer};
use anyhow::{Context, anyhow};
use base64::Engine;
use nix::sys::mman::{MapFlags, ProtFlags, mmap, munmap, shm_open, shm_unlink};
use nix::sys::stat::Mode;
use nix::unistd::ftruncate;
use std::fs;
use std::io::Write;
use std::num::NonZeroUsize;
use std::ptr::NonNull;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KittyTransport {
    Shm,
    File,
    Direct,
}

impl KittyTransport {
    fn label(self) -> &'static str {
        match self {
            Self::Shm => "shm",
            Self::File => "file",
            Self::Direct => "direct",
        }
    }
}

pub struct KittyRenderer {
    image_id: u32,
    placement_id: u32,

    shm_name: String,
    shm_payload_b64: String,
    shm_ptr: Option<NonNull<std::ffi::c_void>>,
    shm_len: usize,

    temp_path: String,
    temp_payload_b64: String,

    transports: Vec<KittyTransport>,
    active_transport_idx: usize,
    rolling_ids: bool,
    b64_buf: Vec<u8>,
    overlay_visible_last: bool,
    last_hud_rows: u16,
}

impl KittyRenderer {
    pub fn new() -> Self {
        let pid = std::process::id();
        // Keep shared-memory name short (some environments enforce strict limits).
        let shm_name = format!("/tv{pid}");
        let shm_payload_b64 =
            base64::engine::general_purpose::STANDARD.encode(shm_name.as_bytes());

        let temp_path = format!("/tmp/tv-{pid}.rgba");
        let temp_payload_b64 =
            base64::engine::general_purpose::STANDARD.encode(temp_path.as_bytes());

        let mut transports = pick_transport_chain();
        if transports.is_empty() {
            transports.push(KittyTransport::Direct);
        }
        let rolling_ids = pick_rolling_ids(transports[0]);

        Self {
            image_id: 1,
            placement_id: 1,
            shm_name,
            shm_payload_b64,
            shm_ptr: None,
            shm_len: 0,
            temp_path,
            temp_payload_b64,
            transports,
            active_transport_idx: 0,
            rolling_ids,
            b64_buf: Vec::new(),
            overlay_visible_last: false,
            last_hud_rows: 0,
        }
    }

    fn ensure_shm(&mut self, len: usize) -> anyhow::Result<()> {
        if len == 0 {
            return Err(anyhow!("empty pixel buffer"));
        }
        if self.shm_len == len && self.shm_ptr.is_some() {
            return Ok(());
        }

        if let Some(ptr) = self.shm_ptr.take() {
            unsafe {
                let _ = munmap(ptr, self.shm_len);
            }
        }
        self.shm_len = 0;

        let fd = shm_open(
            self.shm_name.as_str(),
            nix::fcntl::OFlag::O_CREAT | nix::fcntl::OFlag::O_RDWR,
            Mode::from_bits_truncate(0o600),
        )
        .with_context(|| format!("shm_open({})", self.shm_name))?;
        ftruncate(&fd, len as i64).context("ftruncate shm")?;

        let len_nz = NonZeroUsize::new(len).context("empty pixel buffer")?;
        let ptr = unsafe {
            mmap(
                None,
                len_nz,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_SHARED,
                fd,
                0,
            )
        }
        .context("mmap shm")?;

        self.shm_ptr = Some(ptr);
        self.shm_len = len;
        Ok(())
    }

    fn write_frame_with_transport(
        &mut self,
        transport: KittyTransport,
        frame: &Frame<'_>,
        out: &mut dyn Write,
        cols: usize,
        visual_rows: usize,
        w: usize,
        h: usize,
        image_id: u32,
        placement_id: u32,
    ) -> anyhow::Result<()> {
        match transport {
            KittyTransport::Direct => write_kitty_direct_rgba(
                out,
                frame.pixels_rgba,
                w,
                h,
                cols,
                visual_rows,
                image_id,
                placement_id,
                &mut self.b64_buf,
            ),
            KittyTransport::File => {
                fs::write(self.temp_path.as_str(), frame.pixels_rgba)
                    .with_context(|| format!("write kitty temp file {}", self.temp_path))?;

                write!(
                    out,
                    "\x1b_Ga=T,f=32,s={},v={},t=f,i={},p={},c={},r={},C=1,q=2,z=-1;{}\x1b\\",
                    w,
                    h,
                    image_id,
                    placement_id,
                    cols,
                    visual_rows,
                    self.temp_payload_b64.as_str()
                )?;
                Ok(())
            }
            KittyTransport::Shm => {
                let len = frame.pixels_rgba.len();
                self.ensure_shm(len)?;
                let ptr = self.shm_ptr.context("shm not mapped (internal error)")?;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        frame.pixels_rgba.as_ptr(),
                        ptr.as_ptr().cast::<u8>(),
                        len,
                    );
                }

                write!(
                    out,
                    "\x1b_Ga=T,f=32,s={},v={},t=s,i={},p={},c={},r={},C=1,q=2,z=-1;{}\x1b\\",
                    w,
                    h,
                    image_id,
                    placement_id,
                    cols,
                    visual_rows,
                    self.shm_payload_b64.as_str()
                )?;
                Ok(())
            }
        }
    }
}

impl Renderer for KittyRenderer {
    fn name(&self) -> &'static str {
        "kitty"
    }

    fn render(&mut self, frame: &Frame<'_>, out: &mut dyn Write) -> anyhow::Result<()> {
        let cols = frame.term_cols as usize;
        let visual_rows = frame.visual_rows as usize;
        let w = frame.pixel_width;
        let h = frame.pixel_height;

        if cols == 0 || visual_rows == 0 || w == 0 || h == 0 {
            return Ok(());
        }

        if frame.sync_updates {
            out.write_all(b"\x1b[?2026h")?;
        }

        if let Some(text) = frame.overlay {
            write!(out, "\x1b_Ga=d,d=I,i={}\x1b\\", self.image_id)?;
            clear_visual_text_layer(out, frame.term_rows as usize)?;

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

            draw_overlay_popup(out, frame.term_cols, frame.term_rows, text)?;
            self.overlay_visible_last = true;
            self.last_hud_rows = frame.hud_rows;

            if frame.sync_updates {
                out.write_all(b"\x1b[?2026l")?;
            }
            out.flush()?;
            return Ok(());
        }

        let (image_id, placement_id, prev_image_id) = if self.rolling_ids {
            let prev = self.image_id;
            self.image_id = self.image_id.wrapping_add(1);
            if self.image_id == 0 {
                self.image_id = 1;
            }
            self.placement_id = self.placement_id.wrapping_add(1);
            if self.placement_id == 0 {
                self.placement_id = 1;
            }
            (self.image_id, self.placement_id, Some(prev))
        } else {
            (self.image_id, self.placement_id, None)
        };

        out.write_all(b"\x1b[H")?;

        let start_idx = self
            .active_transport_idx
            .min(self.transports.len().saturating_sub(1));
        let mut rendered = false;
        let mut last_err: Option<anyhow::Error> = None;
        for step in 0..self.transports.len() {
            let idx = (start_idx + step) % self.transports.len();
            let transport = self.transports[idx];
            match self.write_frame_with_transport(
                transport,
                frame,
                out,
                cols,
                visual_rows,
                w,
                h,
                image_id,
                placement_id,
            ) {
                Ok(()) => {
                    self.active_transport_idx = idx;
                    rendered = true;
                    break;
                }
                Err(err) => {
                    // If terminal IO itself failed, fallback won't help.
                    if err.downcast_ref::<std::io::Error>().is_some() {
                        return Err(err);
                    }
                    last_err = Some(err.context(format!(
                        "kitty transport '{}' failed",
                        transport.label()
                    )));
                }
            }
        }
        if !rendered {
            return Err(last_err.unwrap_or_else(|| anyhow!("no kitty transport succeeded")));
        }

        if let Some(prev_image_id) = prev_image_id {
            write!(out, "\x1b_Ga=d,d=I,i={}\x1b\\", prev_image_id)?;
        }

        if frame.hud_rows != self.last_hud_rows {
            clear_visual_text_layer(out, frame.term_rows as usize)?;
        }

        if self.overlay_visible_last && frame.overlay.is_none() {
            clear_visual_text_layer(out, visual_rows)?;
        }

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

        self.overlay_visible_last = false;
        self.last_hud_rows = frame.hud_rows;

        if frame.sync_updates {
            out.write_all(b"\x1b[?2026l")?;
        }
        out.flush()?;
        Ok(())
    }
}

impl Drop for KittyRenderer {
    fn drop(&mut self) {
        if let Some(ptr) = self.shm_ptr.take() {
            unsafe {
                let _ = munmap(ptr, self.shm_len);
            }
        }
        let _ = shm_unlink(self.shm_name.as_str());
        let _ = fs::remove_file(self.temp_path.as_str());
    }
}

fn pick_transport_chain() -> Vec<KittyTransport> {
    if let Ok(v) = std::env::var("TUIVIZ_KITTY_TRANSPORT") {
        let token = v.trim().to_ascii_lowercase();
        if token == "direct" || token == "d" {
            return vec![KittyTransport::Direct];
        }
        if token == "file" || token == "f" || token == "temp" || token == "tempfile" {
            return vec![KittyTransport::File];
        }
        if token == "shm" || token == "s" || token == "shared" {
            return vec![KittyTransport::Shm];
        }
    }

    let term_program = std::env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if term_program.contains("ghostty") {
        vec![
            KittyTransport::Direct,
            KittyTransport::File,
            KittyTransport::Shm,
        ]
    } else {
        vec![
            KittyTransport::Shm,
            KittyTransport::Direct,
            KittyTransport::File,
        ]
    }
}

fn pick_rolling_ids(transport: KittyTransport) -> bool {
    if let Ok(v) = std::env::var("TUIVIZ_KITTY_ROLLING_IDS") {
        let s = v.trim().to_ascii_lowercase();
        if s == "1" || s == "true" || s == "yes" || s == "on" {
            return true;
        }
        if s == "0" || s == "false" || s == "no" || s == "off" {
            return false;
        }
    }

    // Conservative default: stable IDs prevent growth in long sessions.
    let _ = transport;
    false
}

fn write_kitty_direct_rgba(
    out: &mut dyn Write,
    rgba: &[u8],
    w: usize,
    h: usize,
    cols: usize,
    rows: usize,
    image_id: u32,
    placement_id: u32,
    b64_buf: &mut Vec<u8>,
) -> anyhow::Result<()> {
    const RAW_CHUNK: usize = 3 * 1024; // 3072 -> 4096 bytes base64

    if rgba.is_empty() {
        return Ok(());
    }

    let mut off = 0usize;
    let len = rgba.len();
    let mut first = true;
    while off < len {
        let mut end = (off + RAW_CHUNK).min(len);
        if end < len {
            let rem = (end - off) % 3;
            if rem != 0 {
                end -= rem;
            }
            if end == off {
                end = (off + RAW_CHUNK).min(len);
            }
        }

        let chunk = &rgba[off..end];
        let b64_len = ((chunk.len() + 2) / 3) * 4;
        if b64_buf.len() < b64_len {
            b64_buf.resize(b64_len, 0);
        }

        let written = base64::engine::general_purpose::STANDARD
            .encode_slice(chunk, &mut b64_buf[..b64_len])
            .context("base64 encode pixels")?;

        let more = end < len;
        if first {
            if more {
                write!(
                    out,
                    "\x1b_Ga=T,f=32,s={},v={},t=d,i={},p={},c={},r={},C=1,q=2,z=-1,m=1;",
                    w, h, image_id, placement_id, cols, rows
                )?;
            } else {
                write!(
                    out,
                    "\x1b_Ga=T,f=32,s={},v={},t=d,i={},p={},c={},r={},C=1,q=2,z=-1;",
                    w, h, image_id, placement_id, cols, rows
                )?;
            }
            first = false;
        } else if more {
            out.write_all(b"\x1b_Gm=1;")?;
        } else {
            out.write_all(b"\x1b_Gm=0;")?;
        }

        out.write_all(&b64_buf[..written])?;
        out.write_all(b"\x1b\\")?;

        off = end;
    }

    Ok(())
}

fn clear_visual_text_layer(out: &mut dyn Write, visual_rows: usize) -> anyhow::Result<()> {
    for row in 1..=visual_rows {
        write!(out, "\x1b[{};1H\x1b[0m\x1b[2K", row)?;
    }
    Ok(())
}
