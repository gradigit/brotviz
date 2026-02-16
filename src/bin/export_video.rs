use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use tui_visualizer::audio::AudioFeatures;
use tui_visualizer::config::{Quality, SwitchMode};
use tui_visualizer::visual::{make_presets, PresetEngine, RenderCtx, VisualEngine};

#[cfg(target_os = "macos")]
use tui_visualizer::visual::MetalEngine;

const ANALYZER_WINDOW: usize = 1024;
const DEFAULT_OUTPUT: &str = "export.mp4";
const DEFAULT_SEED: u64 = 0xF15D_2026;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum EngineArg {
    Cpu,
    Metal,
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "export_video",
    version,
    about = "Offline visualizer export (WAV input -> MP4 output via ffmpeg)"
)]
pub(crate) struct Cli {
    #[arg(long, value_name = "WAV")]
    pub(crate) audio: PathBuf,

    #[arg(long, value_name = "MP4", default_value = DEFAULT_OUTPUT)]
    pub(crate) out: PathBuf,

    #[arg(long, default_value_t = 1280)]
    pub(crate) width: usize,

    #[arg(long, default_value_t = 720)]
    pub(crate) height: usize,

    #[arg(long, default_value_t = 60)]
    pub(crate) fps: u32,

    #[arg(long, value_name = "SECONDS")]
    pub(crate) duration: Option<f32>,

    #[arg(long, value_name = "INDEX_OR_SUBSTRING")]
    pub(crate) preset: Option<String>,

    #[arg(long, value_enum, default_value_t = EngineArg::Metal)]
    pub(crate) engine: EngineArg,

    #[arg(long, default_value_t = false)]
    pub(crate) safe: bool,
}

pub(crate) fn compute_export_duration(audio_duration_s: f32, duration_cap_s: Option<f32>) -> f32 {
    let base = audio_duration_s.max(0.0);
    match duration_cap_s {
        Some(cap) => base.min(cap.max(0.0)),
        None => base,
    }
}

pub(crate) fn compute_frame_count(duration_s: f32, fps: u32) -> usize {
    ((duration_s.max(0.0) * fps as f32).floor() as usize).max(1)
}

pub(crate) fn validate_args(args: &Cli) -> Result<()> {
    if args.width == 0 {
        bail!("--width must be >= 1");
    }
    if args.height == 0 {
        bail!("--height must be >= 1");
    }
    if args.fps == 0 {
        bail!("--fps must be >= 1");
    }
    if let Some(cap) = args.duration {
        if cap <= 0.0 {
            bail!("--duration must be > 0 seconds");
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Cli::parse();
    run(args)
}

fn run(args: Cli) -> Result<()> {
    validate_args(&args)?;

    ensure_ffmpeg_available()?;

    let (sample_rate_hz, samples) = read_wav_mono_f32(&args.audio)
        .with_context(|| format!("read wav {}", args.audio.display()))?;
    if samples.is_empty() {
        bail!("wav had no samples");
    }

    let audio_duration_s = samples.len() as f32 / sample_rate_hz as f32;
    let export_duration_s = compute_export_duration(audio_duration_s, args.duration);
    if export_duration_s <= 0.0 {
        bail!("audio duration is zero after applying --duration");
    }
    let frame_count = compute_frame_count(export_duration_s, args.fps);

    let preset_names = make_presets().iter().map(|p| p.name()).collect::<Vec<_>>();
    let active = resolve_preset_index(args.preset.as_deref(), &preset_names)?;
    let mut engine = build_engine(args.engine, active, &preset_names)?;
    engine.resize(args.width, args.height);

    let mut parent = args.out.parent().unwrap_or_else(|| Path::new(""));
    if parent == Path::new("") {
        parent = Path::new(".");
    }
    fs::create_dir_all(parent)
        .with_context(|| format!("create output directory {}", parent.display()))?;

    let features = build_feature_track(&samples, sample_rate_hz, args.fps, frame_count)?;
    let encoded_duration_s = frame_count as f32 / args.fps as f32;

    let mut ffmpeg = spawn_ffmpeg(
        &args.audio,
        &args.out,
        args.width,
        args.height,
        args.fps,
        encoded_duration_s,
    )?;
    let mut ffmpeg_in = ffmpeg
        .stdin
        .take()
        .context("failed to open ffmpeg stdin for rawvideo input")?;

    render_frames(
        &mut *engine,
        &features,
        args.width,
        args.height,
        args.fps,
        args.safe,
        &mut ffmpeg_in,
    )?;
    drop(ffmpeg_in);

    let status = ffmpeg.wait().context("wait for ffmpeg")?;
    if !status.success() {
        bail!("ffmpeg exited with status {status}");
    }

    println!(
        "exported {} frames @ {} fps (duration {:.3}s) -> {}",
        frame_count,
        args.fps,
        encoded_duration_s,
        args.out.display()
    );
    Ok(())
}

fn ensure_ffmpeg_available() -> Result<()> {
    match Command::new("ffmpeg")
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            bail!("ffmpeg not found in PATH (install ffmpeg and retry)")
        }
        Err(err) => Err(anyhow!("failed to run ffmpeg: {err}")),
    }
}

fn build_engine(
    requested: EngineArg,
    active: usize,
    preset_names: &[&'static str],
) -> Result<Box<dyn VisualEngine>> {
    fastrand::seed(DEFAULT_SEED);

    match requested {
        EngineArg::Cpu => Ok(Box::new(PresetEngine::new(
            make_presets(),
            active,
            false,
            SwitchMode::Manual,
            16,
            20.0,
        ))),
        EngineArg::Metal => {
            #[cfg(target_os = "macos")]
            {
                match MetalEngine::new(
                    preset_names.to_vec(),
                    active,
                    false,
                    SwitchMode::Manual,
                    16,
                    20.0,
                ) {
                    Ok(engine) => Ok(Box::new(engine)),
                    Err(err) => {
                        eprintln!(
                            "warning: metal engine unavailable ({err}); falling back to cpu"
                        );
                        Ok(Box::new(PresetEngine::new(
                            make_presets(),
                            active,
                            false,
                            SwitchMode::Manual,
                            16,
                            20.0,
                        )))
                    }
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                eprintln!("warning: --engine metal is only supported on macOS; falling back to cpu");
                Ok(Box::new(PresetEngine::new(
                    make_presets(),
                    active,
                    false,
                    SwitchMode::Manual,
                    16,
                    20.0,
                )))
            }
        }
    }
}

fn resolve_preset_index(selection: Option<&str>, preset_names: &[&str]) -> Result<usize> {
    if preset_names.is_empty() {
        bail!("no presets available");
    }
    let Some(raw) = selection else {
        return Ok(0);
    };

    if let Ok(idx) = raw.parse::<usize>() {
        if idx < preset_names.len() {
            return Ok(idx);
        }
        bail!(
            "preset index {} out of range (0..{})",
            idx,
            preset_names.len().saturating_sub(1)
        );
    }

    let needle = raw.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(0);
    }

    if let Some((idx, _)) = preset_names
        .iter()
        .enumerate()
        .find(|(_, name)| name.to_ascii_lowercase() == needle)
    {
        return Ok(idx);
    }

    if let Some((idx, _)) = preset_names
        .iter()
        .enumerate()
        .find(|(_, name)| name.to_ascii_lowercase().contains(&needle))
    {
        return Ok(idx);
    }

    bail!("preset '{}' not found", raw)
}

fn spawn_ffmpeg(
    audio_path: &Path,
    out_path: &Path,
    width: usize,
    height: usize,
    fps: u32,
    duration_s: f32,
) -> Result<std::process::Child> {
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("-video_size")
        .arg(format!("{width}x{height}"))
        .arg("-framerate")
        .arg(fps.to_string())
        .arg("-i")
        .arg("-")
        .arg("-i")
        .arg(audio_path)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("1:a:0")
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-c:a")
        .arg("aac")
        .arg("-t")
        .arg(format!("{duration_s:.6}"))
        .arg("-shortest")
        .arg("-movflags")
        .arg("+faststart")
        .arg(out_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    cmd.spawn().with_context(|| {
        format!(
            "spawn ffmpeg for output {} (audio {})",
            out_path.display(),
            audio_path.display()
        )
    })
}

fn render_frames(
    engine: &mut dyn VisualEngine,
    features: &[AudioFeatures],
    width: usize,
    height: usize,
    fps: u32,
    safe: bool,
    sink: &mut dyn Write,
) -> Result<()> {
    let fps_f = fps as f32;
    let dt = 1.0 / fps_f;
    let start = Instant::now();
    let mut beat_pulse = 0.0f32;

    for (frame_idx, audio) in features.iter().copied().enumerate() {
        if audio.beat {
            beat_pulse = (beat_pulse + 0.65 + audio.beat_strength * 0.7).min(1.6);
        }
        beat_pulse *= (0.1f32).powf(dt);

        let t = frame_idx as f32 / fps_f;
        let ctx = RenderCtx {
            now: start + Duration::from_secs_f32(t),
            t,
            dt,
            w: width,
            h: height,
            audio,
            beat_pulse,
            fractal_zoom_mul: 1.0,
            safe,
            quality: Quality::Balanced,
            scale: 1,
        };
        let pixels = engine.render(ctx, Quality::Balanced, 1);
        sink.write_all(pixels).context("write frame to ffmpeg stdin")?;
    }

    Ok(())
}

fn build_feature_track(
    samples: &[f32],
    sample_rate_hz: u32,
    fps: u32,
    frame_count: usize,
) -> Result<Vec<AudioFeatures>> {
    let n = ANALYZER_WINDOW;
    if n == 0 {
        bail!("invalid analyzer window size");
    }

    let hann = (0..n)
        .map(|i| 0.5 - 0.5 * ((2.0 * std::f32::consts::PI * i as f32) / (n as f32)).cos())
        .collect::<Vec<_>>();

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let mut fft_buf = vec![Complex { re: 0.0, im: 0.0 }; n];
    let mut mags = vec![0.0f32; n / 2];
    let mut prev_mags = vec![0.0f32; n / 2];
    let mut window = vec![0.0f32; n];

    let mut flux_avg = 0.0f32;
    let mut flux_hist = [0.0f32; 3];

    let mut rms_s = 0.0f32;
    let mut bands_s = [0.0f32; 8];
    let mut centroid_s = 0.0f32;
    let mut flatness_s = 0.0f32;

    let mut out = Vec::<AudioFeatures>::with_capacity(frame_count);
    let sr = sample_rate_hz as f32;
    let fps_f = fps as f32;

    for frame in 0..frame_count {
        let t = frame as f32 / fps_f;
        let sample_end = ((t * sr).floor() as usize).min(samples.len());
        fill_window(samples, sample_end, &mut window);

        let (rms, bands, flux, centroid, flatness) = analyze_window(
            &window,
            &hann,
            &fft,
            &mut fft_buf,
            &mut mags,
            &mut prev_mags,
            sample_rate_hz,
        );

        flux_hist[0] = flux_hist[1];
        flux_hist[1] = flux_hist[2];
        flux_hist[2] = flux;
        flux_avg = flux_avg * 0.95 + flux * 0.05;

        let peak = flux_hist[1] > flux_hist[0] && flux_hist[1] > flux_hist[2];
        let thr = (flux_avg * 1.45).max(1e-6);
        let beat = peak && flux_hist[1] > thr;
        let beat_strength = ((flux_hist[1] - thr) / (thr + 1e-6)).clamp(0.0, 1.0);

        rms_s = rms_s * 0.85 + rms * 0.15;
        for i in 0..bands_s.len() {
            bands_s[i] = bands_s[i] * 0.85 + bands[i] * 0.15;
        }
        centroid_s = centroid_s * 0.9 + centroid * 0.1;
        flatness_s = flatness_s * 0.9 + flatness * 0.1;

        out.push(AudioFeatures {
            rms: rms_s,
            bands: bands_s,
            onset: flux,
            beat,
            beat_strength,
            centroid: centroid_s,
            flatness: flatness_s,
        });
    }

    Ok(out)
}

fn fill_window(samples: &[f32], sample_end: usize, out: &mut [f32]) {
    out.fill(0.0);
    let len = out.len();
    let end = sample_end.min(samples.len());
    let start = end.saturating_sub(len);
    let src = &samples[start..end];
    let dst_off = len.saturating_sub(src.len());
    out[dst_off..].copy_from_slice(src);
}

fn analyze_window(
    window: &[f32],
    hann: &[f32],
    fft: &std::sync::Arc<dyn rustfft::Fft<f32>>,
    fft_buf: &mut [Complex<f32>],
    mags: &mut [f32],
    prev_mags: &mut [f32],
    sample_rate_hz: u32,
) -> (f32, [f32; 8], f32, f32, f32) {
    let n = fft_buf.len();
    let half = mags.len();

    let mut rms_acc = 0.0f32;
    for i in 0..n {
        let s = window[i];
        rms_acc += s * s;
        fft_buf[i].re = s * hann[i];
        fft_buf[i].im = 0.0;
    }
    let rms = (rms_acc / n as f32).sqrt().clamp(0.0, 1.0);

    fft.process(fft_buf);
    for (i, c) in fft_buf.iter().take(half).enumerate() {
        mags[i] = (c.re * c.re + c.im * c.im).sqrt();
    }

    let mut flux = 0.0f32;
    for i in 0..half {
        let d = mags[i] - prev_mags[i];
        if d > 0.0 {
            flux += d;
        }
        prev_mags[i] = mags[i];
    }
    let flux_scale = 0.002 * (1024.0 / (half as f32).max(1.0));
    flux = (flux * flux_scale).tanh();

    let edges_hz = [20.0, 60.0, 150.0, 400.0, 1000.0, 2500.0, 6000.0, 12000.0, 20000.0];
    let mut bands = [0.0f32; 8];
    let mut counts = [0u32; 8];
    let sr = sample_rate_hz as f32;
    for i in 1..half {
        let f = i as f32 * sr / n as f32;
        if f < edges_hz[0] {
            continue;
        }
        if f >= edges_hz[8] {
            break;
        }
        let mut band = 0usize;
        while band + 1 < edges_hz.len() - 1 && f >= edges_hz[band + 1] {
            band += 1;
        }
        bands[band] += mags[i];
        counts[band] += 1;
    }
    for i in 0..bands.len() {
        let denom = counts[i].max(1) as f32;
        bands[i] = ((bands[i] / denom) * 0.01).tanh();
    }

    let mut num = 0.0f32;
    let mut den = 0.0f32;
    for i in 1..half {
        let f = i as f32 * sr / n as f32;
        let m = mags[i];
        num += f * m;
        den += m;
    }
    let centroid = if den > 1e-6 {
        (num / den / 8000.0).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let start_bin = (400.0 * n as f32 / sr) as usize;
    let end_bin = (6000.0 * n as f32 / sr) as usize;
    let mut log_gm = 0.0f32;
    let mut am = 0.0f32;
    let mut k = 0u32;
    for i in start_bin.clamp(1, half.saturating_sub(1))..end_bin.clamp(1, half) {
        let m = mags[i].max(1e-6);
        log_gm += m.ln();
        am += m;
        k += 1;
    }
    let flatness = if k > 0 && am > 1e-6 {
        let gm = (log_gm / k as f32).exp();
        (gm / (am / k as f32)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    (rms, bands, flux, centroid, flatness)
}

fn read_wav_mono_f32(path: &Path) -> Result<(u32, Vec<f32>)> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 44 {
        bail!("wav too small");
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        bail!("not a RIFF/WAVE file");
    }

    let mut fmt_audio_format = 0u16;
    let mut fmt_channels = 0u16;
    let mut fmt_sample_rate = 0u32;
    let mut fmt_bits = 0u16;
    let mut data: Option<&[u8]> = None;

    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size =
            u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]])
                as usize;
        let start = pos + 8;
        let end = start.saturating_add(size);
        if end > bytes.len() {
            break;
        }

        if id == b"fmt " {
            if size < 16 {
                bail!("invalid fmt chunk");
            }
            fmt_audio_format = u16::from_le_bytes([bytes[start], bytes[start + 1]]);
            fmt_channels = u16::from_le_bytes([bytes[start + 2], bytes[start + 3]]);
            fmt_sample_rate = u32::from_le_bytes([
                bytes[start + 4],
                bytes[start + 5],
                bytes[start + 6],
                bytes[start + 7],
            ]);
            fmt_bits = u16::from_le_bytes([bytes[start + 14], bytes[start + 15]]);
        } else if id == b"data" {
            data = Some(&bytes[start..end]);
        }

        pos = end + (size % 2);
    }

    let data = data.context("missing data chunk")?;
    if fmt_channels == 0 {
        bail!("invalid channel count");
    }

    match (fmt_audio_format, fmt_bits) {
        (1, 16) => {
            let ch = fmt_channels as usize;
            let total = data.len() / 2;
            let frames = total / ch;
            let mut out = Vec::<f32>::with_capacity(frames);
            for i in 0..frames {
                let mut acc = 0.0f32;
                for c in 0..ch {
                    let o = (i * ch + c) * 2;
                    let s = i16::from_le_bytes([data[o], data[o + 1]]) as f32 / 32768.0;
                    acc += s;
                }
                out.push((acc / ch as f32).clamp(-1.0, 1.0));
            }
            Ok((fmt_sample_rate, out))
        }
        (3, 32) => {
            let ch = fmt_channels as usize;
            let total = data.len() / 4;
            let frames = total / ch;
            let mut out = Vec::<f32>::with_capacity(frames);
            for i in 0..frames {
                let mut acc = 0.0f32;
                for c in 0..ch {
                    let o = (i * ch + c) * 4;
                    let s = f32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
                    acc += s;
                }
                out.push((acc / ch as f32).clamp(-1.0, 1.0));
            }
            Ok((fmt_sample_rate, out))
        }
        _ => bail!(
            "unsupported wav format: audio_format={} bits={} (supported: PCM16, Float32)",
            fmt_audio_format,
            fmt_bits
        ),
    }
}
