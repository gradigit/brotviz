use std::f32::consts::PI;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};

struct Args {
    out: PathBuf,
    sample_rate: u32,
}

fn parse_args() -> Args {
    let mut out = PathBuf::from("assets/test/latency_pulse_120bpm.wav");
    let mut sample_rate = 48_000u32;

    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let v = it.next();
        match (k.as_str(), v) {
            ("--out", Some(p)) => out = PathBuf::from(p),
            ("--sample-rate", Some(v)) => {
                if let Ok(sr) = v.parse::<u32>() {
                    sample_rate = sr.clamp(8_000, 192_000);
                }
            }
            _ => {}
        }
    }

    Args { out, sample_rate }
}

fn main() -> Result<()> {
    let args = parse_args();
    if let Some(parent) = args.out.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create dir {}", parent.display()))?;
    }

    let samples = make_latency_fixture(args.sample_rate);
    write_wav_i16_mono(&args.out, args.sample_rate, &samples)
        .with_context(|| format!("write {}", args.out.display()))?;

    println!("generated: {}", args.out.display());
    println!("sample_rate={}Hz duration={:.2}s samples={}", args.sample_rate, samples.len() as f32 / args.sample_rate as f32, samples.len());
    Ok(())
}

fn make_latency_fixture(sr: u32) -> Vec<i16> {
    let mut out = Vec::<i16>::new();

    // 1) 2s silence for startup.
    push_silence(&mut out, sr, 2.0);

    // 2) 20 pulses at 120 BPM (0.5s interval), each pulse has low + high click.
    for i in 0..20 {
        let freq_low = 60.0 + (i as f32 * 1.5);
        push_pulse(&mut out, sr, 0.020, freq_low, 0.92, 2200.0, 0.35);
        push_silence(&mut out, sr, 0.480);
    }

    // 3) 10s smooth pad-like section for transition behavior tests.
    push_pad_section(&mut out, sr, 10.0);

    // 4) 12s dense transients for jump-cut transition tests.
    push_dense_transients(&mut out, sr, 12.0);

    // 5) 8s chirp sweep to exercise centroid/treble responsiveness.
    push_chirp(&mut out, sr, 8.0, 120.0, 8_000.0, 0.70);

    // 6) 2s tail silence.
    push_silence(&mut out, sr, 2.0);

    out
}

fn push_silence(out: &mut Vec<i16>, sr: u32, seconds: f32) {
    let n = (seconds.max(0.0) * sr as f32).round() as usize;
    out.resize(out.len() + n, 0);
}

fn push_pulse(
    out: &mut Vec<i16>,
    sr: u32,
    seconds: f32,
    freq_low: f32,
    amp_low: f32,
    freq_click: f32,
    amp_click: f32,
) {
    let n = (seconds.max(0.0) * sr as f32).round() as usize;
    for i in 0..n {
        let t = i as f32 / sr as f32;
        // Fast attack, short decay.
        let env = ((1.0 - t / seconds.max(1e-5)).max(0.0)).powf(2.4);
        let s_low = (2.0 * PI * freq_low * t).sin() * amp_low;
        let s_click = (2.0 * PI * freq_click * t).sin() * amp_click;
        let v = (s_low + s_click) * env;
        out.push(to_i16(v));
    }
}

fn push_pad_section(out: &mut Vec<i16>, sr: u32, seconds: f32) {
    let n = (seconds.max(0.0) * sr as f32).round() as usize;
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let env = 0.65;
        let a = (2.0 * PI * (110.0 + 8.0 * (t * 0.21).sin()) * t).sin() * 0.45;
        let b = (2.0 * PI * (220.0 + 16.0 * (t * 0.17).cos()) * t).sin() * 0.25;
        let c = (2.0 * PI * (440.0 + 24.0 * (t * 0.13).sin()) * t).sin() * 0.12;
        out.push(to_i16((a + b + c) * env));
    }
}

fn push_dense_transients(out: &mut Vec<i16>, sr: u32, seconds: f32) {
    let segment_samples = (seconds.max(0.0) * sr as f32).round() as usize;
    let mut i = 0usize;
    while i < segment_samples {
        let t = i as f32 / sr as f32;
        // 160 BPM-ish event grid with slight jitter pattern.
        let beat_period = 60.0 / 160.0;
        let phase = (t / beat_period).fract();
        let hit = if phase < 0.06 { 1.0 } else { 0.0 };

        let low = (2.0 * PI * 55.0 * t).sin() * (0.45 + 0.45 * hit);
        let mid = (2.0 * PI * 330.0 * t).sin() * 0.22;
        let hat = (2.0 * PI * 5_500.0 * t).sin() * (0.05 + 0.25 * hit);
        let noise = pseudo_noise(i as u32) * (0.03 + 0.15 * hit);

        out.push(to_i16(low + mid + hat + noise));
        i += 1;
    }
}

fn push_chirp(out: &mut Vec<i16>, sr: u32, seconds: f32, f0: f32, f1: f32, amp: f32) {
    let n = (seconds.max(0.0) * sr as f32).round() as usize;
    let dur = seconds.max(1e-4);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let x = (t / dur).clamp(0.0, 1.0);
        let f = f0 * (f1 / f0).powf(x);
        let env = 0.15 + 0.85 * (1.0 - (2.0 * x - 1.0).abs());
        out.push(to_i16((2.0 * PI * f * t).sin() * amp * env));
    }
}

fn pseudo_noise(x: u32) -> f32 {
    let mut n = x.wrapping_mul(374_761_393);
    n ^= n >> 13;
    n = n.wrapping_mul(1_274_126_177);
    n ^= n >> 16;
    let v = (n & 0x00FF_FFFF) as f32 / 16_777_215.0;
    v * 2.0 - 1.0
}

fn to_i16(x: f32) -> i16 {
    let y = x.clamp(-1.0, 1.0);
    (y * i16::MAX as f32) as i16
}

fn write_wav_i16_mono(path: &PathBuf, sr: u32, samples: &[i16]) -> Result<()> {
    let mut w = BufWriter::new(fs::File::create(path)?);

    let channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let byte_rate = sr * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_bytes = (samples.len() * std::mem::size_of::<i16>()) as u32;
    let riff_size = 4 + 8 + 16 + 8 + data_bytes;

    w.write_all(b"RIFF")?;
    w.write_all(&riff_size.to_le_bytes())?;
    w.write_all(b"WAVE")?;

    // fmt chunk
    w.write_all(b"fmt ")?;
    w.write_all(&16u32.to_le_bytes())?; // PCM fmt chunk size
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&channels.to_le_bytes())?;
    w.write_all(&sr.to_le_bytes())?;
    w.write_all(&byte_rate.to_le_bytes())?;
    w.write_all(&block_align.to_le_bytes())?;
    w.write_all(&bits_per_sample.to_le_bytes())?;

    // data chunk
    w.write_all(b"data")?;
    w.write_all(&data_bytes.to_le_bytes())?;
    for s in samples {
        w.write_all(&s.to_le_bytes())?;
    }

    w.flush()?;
    Ok(())
}
