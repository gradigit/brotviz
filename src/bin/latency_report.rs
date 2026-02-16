use std::cmp::Ordering;
use std::f32::consts::PI;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

#[derive(Debug, Clone)]
struct Args {
    wav: PathBuf,
    pulse_start_s: f32,
    pulse_interval_s: f32,
    pulse_count: usize,
    early_ms: f32,
    late_ms: f32,
    fail_over_ms: Option<f32>,
}

#[derive(Debug, Clone)]
struct MatchReport {
    deltas_ms: Vec<f32>,
    matched: usize,
    misses: usize,
    false_positives: usize,
    mean_ms: f32,
    p50_ms: f32,
    p95_ms: f32,
    min_ms: f32,
    max_ms: f32,
}

fn parse_args() -> Args {
    parse_args_from(std::env::args().skip(1).collect())
}

fn parse_args_from(argv: Vec<String>) -> Args {
    let mut args = Args {
        wav: PathBuf::from("assets/test/latency_pulse_120bpm.wav"),
        pulse_start_s: 2.0,
        pulse_interval_s: 0.5,
        pulse_count: 20,
        early_ms: 80.0,
        late_ms: 350.0,
        fail_over_ms: None,
    };

    let mut i = 0usize;
    while i < argv.len() {
        let k = argv[i].as_str();
        let v = argv.get(i + 1).map(|s| s.as_str());
        match (k, v) {
            ("--wav", Some(x)) => {
                args.wav = PathBuf::from(x);
                i += 2;
            }
            ("--pulse-start-s", Some(x)) => {
                if let Ok(v) = x.parse::<f32>() {
                    args.pulse_start_s = v.max(0.0);
                }
                i += 2;
            }
            ("--pulse-interval-s", Some(x)) => {
                if let Ok(v) = x.parse::<f32>() {
                    args.pulse_interval_s = v.max(0.01);
                }
                i += 2;
            }
            ("--pulse-count", Some(x)) => {
                if let Ok(v) = x.parse::<usize>() {
                    args.pulse_count = v.max(1);
                }
                i += 2;
            }
            ("--early-ms", Some(x)) => {
                if let Ok(v) = x.parse::<f32>() {
                    args.early_ms = v.max(0.0);
                }
                i += 2;
            }
            ("--late-ms", Some(x)) => {
                if let Ok(v) = x.parse::<f32>() {
                    args.late_ms = v.max(1.0);
                }
                i += 2;
            }
            ("--fail-over-ms", Some(x)) => {
                if let Ok(v) = x.parse::<f32>() {
                    args.fail_over_ms = Some(v.max(0.1));
                }
                i += 2;
            }
            _ => i += 1,
        }
    }

    args
}

fn main() -> Result<()> {
    let args = parse_args();
    let (sample_rate_hz, samples) = read_wav_mono_f32(&args.wav)
        .with_context(|| format!("read wav {}", args.wav.display()))?;
    if samples.is_empty() {
        return Err(anyhow!("wav had no samples"));
    }

    let detected = detect_beats_like_runtime(&samples, sample_rate_hz);
    let expected = (0..args.pulse_count)
        .map(|i| args.pulse_start_s + i as f32 * args.pulse_interval_s)
        .collect::<Vec<_>>();

    let early_s = args.early_ms / 1000.0;
    let late_s = args.late_ms / 1000.0;
    let pulse_end_s =
        args.pulse_start_s + args.pulse_interval_s * (args.pulse_count.saturating_sub(1) as f32);
    let eval_start_s = (args.pulse_start_s - early_s).max(0.0);
    let eval_end_s = pulse_end_s + late_s;
    let detected_in_window = detected
        .iter()
        .copied()
        .filter(|t| *t >= eval_start_s && *t <= eval_end_s)
        .collect::<Vec<_>>();

    let report = match_pulses(&expected, &detected_in_window, early_s, late_s);

    println!("Latency report");
    println!("  wav: {}", args.wav.display());
    println!(
        "  sample_rate: {} Hz  duration: {:.2} s",
        sample_rate_hz,
        samples.len() as f32 / sample_rate_hz as f32
    );
    println!(
        "  expected pulses: {}  detected beats: {} (pulse_window: {})  matched: {}  misses: {}  false_positives: {}",
        expected.len(),
        detected.len(),
        detected_in_window.len(),
        report.matched,
        report.misses,
        report.false_positives
    );
    println!(
        "  delta(ms): mean={:.1} p50={:.1} p95={:.1} min={:.1} max={:.1}",
        report.mean_ms, report.p50_ms, report.p95_ms, report.min_ms, report.max_ms
    );
    if !report.deltas_ms.is_empty() {
        let preview = report
            .deltas_ms
            .iter()
            .take(8)
            .map(|v| format!("{:.1}", v))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  first_deltas_ms: [{}]", preview);
    }
    println!("  analyzer window/hop: 1024/256");

    if let Some(limit) = args.fail_over_ms {
        if report.p95_ms > limit {
            anyhow::bail!("p95 {:.1}ms > fail-over {:.1}ms", report.p95_ms, limit);
        }
    }

    Ok(())
}

fn match_pulses(
    expected: &[f32],
    detected: &[f32],
    early_s: f32,
    late_s: f32,
) -> MatchReport {
    let mut deltas_ms = Vec::<f32>::new();
    let mut misses = 0usize;
    let mut di = 0usize;

    for &e in expected {
        while di < detected.len() && detected[di] < e - early_s {
            di += 1;
        }
        if di < detected.len() && detected[di] <= e + late_s {
            deltas_ms.push((detected[di] - e) * 1000.0);
            di += 1;
        } else {
            misses += 1;
        }
    }

    let matched = deltas_ms.len();
    let false_positives = detected.len().saturating_sub(matched);

    if deltas_ms.is_empty() {
        return MatchReport {
            deltas_ms,
            matched,
            misses,
            false_positives,
            mean_ms: 0.0,
            p50_ms: 0.0,
            p95_ms: 0.0,
            min_ms: 0.0,
            max_ms: 0.0,
        };
    }

    let mut sorted = deltas_ms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let mean_ms = deltas_ms.iter().sum::<f32>() / deltas_ms.len() as f32;
    let p50_ms = percentile(&sorted, 0.50);
    let p95_ms = percentile(&sorted, 0.95);
    let min_ms = *sorted.first().unwrap_or(&0.0);
    let max_ms = *sorted.last().unwrap_or(&0.0);

    MatchReport {
        deltas_ms,
        matched,
        misses,
        false_positives,
        mean_ms,
        p50_ms,
        p95_ms,
        min_ms,
        max_ms,
    }
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f32 * p.clamp(0.0, 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn detect_beats_like_runtime(samples: &[f32], sample_rate_hz: u32) -> Vec<f32> {
    let n = 1024usize;
    let hop = 256usize;
    let sr = sample_rate_hz as f32;

    let hann = (0..n)
        .map(|i| 0.5 - 0.5 * ((2.0 * PI * i as f32) / (n as f32)).cos())
        .collect::<Vec<_>>();

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let mut fft_buf = vec![Complex { re: 0.0, im: 0.0 }; n];
    let mut mags = vec![0.0f32; n / 2];
    let mut prev_mags = vec![0.0f32; n / 2];

    let mut scratch = vec![0.0f32; n];
    let mut write_pos = 0usize;
    let mut filled = 0usize;
    let mut since_last = 0usize;

    let mut flux_avg = 0.0f32;
    let mut flux_hist = [0.0f32; 3];
    let mut beats = Vec::<f32>::new();

    for (idx, &s) in samples.iter().enumerate() {
        scratch[write_pos] = s;
        write_pos = (write_pos + 1) % n;
        if filled < n {
            filled += 1;
        }
        since_last += 1;

        if filled == n && since_last >= hop {
            since_last = 0;
            let flux = analyze_flux_window(
                &scratch,
                write_pos,
                &hann,
                &fft,
                &mut fft_buf,
                &mut mags,
                &mut prev_mags,
            );

            flux_hist[0] = flux_hist[1];
            flux_hist[1] = flux_hist[2];
            flux_hist[2] = flux;
            flux_avg = flux_avg * 0.95 + flux * 0.05;

            let peak = flux_hist[1] > flux_hist[0] && flux_hist[1] > flux_hist[2];
            let thr = (flux_avg * 1.45).max(1e-6);
            let beat = peak && flux_hist[1] > thr;
            if beat {
                let t = idx as f32 / sr;
                if beats.last().map(|last| t - *last > 0.09).unwrap_or(true) {
                    beats.push(t);
                }
            }
        }
    }

    beats
}

fn analyze_flux_window(
    scratch: &[f32],
    write_pos: usize,
    hann: &[f32],
    fft: &std::sync::Arc<dyn rustfft::Fft<f32>>,
    fft_buf: &mut [Complex<f32>],
    mags: &mut [f32],
    prev_mags: &mut [f32],
) -> f32 {
    let n = fft_buf.len();
    let half = mags.len();

    for i in 0..n {
        let s = scratch[(write_pos + i) % n];
        fft_buf[i].re = s * hann[i];
        fft_buf[i].im = 0.0;
    }

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
    (flux * flux_scale).tanh()
}

fn read_wav_mono_f32(path: &PathBuf) -> Result<(u32, Vec<f32>)> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 44 {
        return Err(anyhow!("wav too small"));
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(anyhow!("not a RIFF/WAVE file"));
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
                return Err(anyhow!("invalid fmt chunk"));
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
        return Err(anyhow!("invalid channel count"));
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
        _ => Err(anyhow!(
            "unsupported wav format: audio_format={} bits={} (supported: PCM16, Float32)",
            fmt_audio_format,
            fmt_bits
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_defaults_are_stable() {
        let args = parse_args_from(Vec::new());
        assert_eq!(
            args.wav,
            PathBuf::from("assets/test/latency_pulse_120bpm.wav")
        );
        assert!((args.pulse_start_s - 2.0).abs() < 1e-6);
        assert!((args.pulse_interval_s - 0.5).abs() < 1e-6);
        assert_eq!(args.pulse_count, 20);
        assert!((args.early_ms - 80.0).abs() < 1e-6);
        assert!((args.late_ms - 350.0).abs() < 1e-6);
        assert_eq!(args.fail_over_ms, None);
    }

    #[test]
    fn parse_args_clamps_ranges() {
        let args = parse_args_from(
            [
                "--pulse-start-s",
                "-5",
                "--pulse-interval-s",
                "0",
                "--pulse-count",
                "0",
                "--early-ms",
                "-1",
                "--late-ms",
                "0",
                "--fail-over-ms",
                "0",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );

        assert!((args.pulse_start_s - 0.0).abs() < 1e-6);
        assert!((args.pulse_interval_s - 0.01).abs() < 1e-6);
        assert_eq!(args.pulse_count, 1);
        assert!((args.early_ms - 0.0).abs() < 1e-6);
        assert!((args.late_ms - 1.0).abs() < 1e-6);
        assert_eq!(args.fail_over_ms, Some(0.1));
    }

    #[test]
    fn percentile_clamps_inputs_and_uses_stable_rounding() {
        let sorted = [-10.0f32, 0.0, 5.0, 20.0];
        assert!((percentile(&sorted, -1.0) - (-10.0)).abs() < 1e-6);
        assert!((percentile(&sorted, 0.5) - 5.0).abs() < 1e-6);
        assert!((percentile(&sorted, 2.0) - 20.0).abs() < 1e-6);
        assert!((percentile(&[], 0.95) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn match_pulses_counts_matches_misses_and_false_positives() {
        let expected = [1.0f32, 2.0, 3.0];
        let detected = [0.6f32, 1.04, 2.25, 4.0];
        let report = match_pulses(&expected, &detected, 0.05, 0.3);

        assert_eq!(report.matched, 2);
        assert_eq!(report.misses, 1);
        assert_eq!(report.false_positives, 2);
        assert_eq!(report.deltas_ms.len(), 2);
        assert!((report.deltas_ms[0] - 40.0).abs() < 1e-3);
        assert!((report.deltas_ms[1] - 250.0).abs() < 1e-3);
        assert!((report.mean_ms - 145.0).abs() < 1e-3);
        assert!((report.p50_ms - 250.0).abs() < 1e-3);
        assert!((report.p95_ms - 250.0).abs() < 1e-3);
        assert!((report.min_ms - 40.0).abs() < 1e-3);
        assert!((report.max_ms - 250.0).abs() < 1e-3);
    }
}
