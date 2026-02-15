use crate::config::AudioSource;
use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer as _, Producer as _, Split as _};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use std::f32::consts::PI;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "macos")]
use screencapturekit::prelude::*;
#[cfg(target_os = "macos")]
use screencapturekit::cm::AudioBufferList;

#[derive(Debug, Clone, Copy)]
pub struct AudioFeatures {
    pub rms: f32,
    pub bands: [f32; 8],
    pub onset: f32,
    pub beat: bool,
    pub beat_strength: f32,
    pub centroid: f32,
    pub flatness: f32,
}

impl Default for AudioFeatures {
    fn default() -> Self {
        Self {
            rms: 0.0,
            bands: [0.0; 8],
            onset: 0.0,
            beat: false,
            beat_strength: 0.0,
            centroid: 0.0,
            flatness: 0.0,
        }
    }
}

pub struct AtomicAudioFeatures {
    seq: AtomicU64,
    rms: AtomicU32,
    bands: [AtomicU32; 8],
    onset: AtomicU32,
    beat: AtomicU32,
    beat_strength: AtomicU32,
    centroid: AtomicU32,
    flatness: AtomicU32,
    updated_ms: AtomicU64,
}

impl AtomicAudioFeatures {
    pub fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
            rms: AtomicU32::new(0),
            bands: std::array::from_fn(|_| AtomicU32::new(0)),
            onset: AtomicU32::new(0),
            beat: AtomicU32::new(0),
            beat_strength: AtomicU32::new(0),
            centroid: AtomicU32::new(0),
            flatness: AtomicU32::new(0),
            updated_ms: AtomicU64::new(0),
        }
    }

    pub fn store(&self, f: AudioFeatures) {
        self.seq.fetch_add(1, Ordering::Release); // odd => write in progress
        self.rms.store(f.rms.to_bits(), Ordering::Relaxed);
        for (dst, src) in self.bands.iter().zip(f.bands) {
            dst.store(src.to_bits(), Ordering::Relaxed);
        }
        self.onset.store(f.onset.to_bits(), Ordering::Relaxed);
        self.beat
            .store(if f.beat { 1 } else { 0 }, Ordering::Relaxed);
        self.beat_strength
            .store(f.beat_strength.to_bits(), Ordering::Relaxed);
        self.centroid.store(f.centroid.to_bits(), Ordering::Relaxed);
        self.flatness.store(f.flatness.to_bits(), Ordering::Relaxed);
        self.updated_ms.store(now_ms(), Ordering::Relaxed);
        self.seq.fetch_add(1, Ordering::Release); // even => stable
    }

    pub fn load(&self) -> AudioFeatures {
        loop {
            let v1 = self.seq.load(Ordering::Acquire);
            if v1 & 1 == 1 {
                continue;
            }

            let rms = f32::from_bits(self.rms.load(Ordering::Relaxed));
            let mut bands = [0.0f32; 8];
            for (i, src) in self.bands.iter().enumerate() {
                bands[i] = f32::from_bits(src.load(Ordering::Relaxed));
            }
            let onset = f32::from_bits(self.onset.load(Ordering::Relaxed));
            let beat = self.beat.load(Ordering::Relaxed) != 0;
            let beat_strength = f32::from_bits(self.beat_strength.load(Ordering::Relaxed));
            let centroid = f32::from_bits(self.centroid.load(Ordering::Relaxed));
            let flatness = f32::from_bits(self.flatness.load(Ordering::Relaxed));

            let v2 = self.seq.load(Ordering::Acquire);
            if v1 == v2 {
                return AudioFeatures {
                    rms,
                    bands,
                    onset,
                    beat,
                    beat_strength,
                    centroid,
                    flatness,
                };
            }
        }
    }

    pub fn age_ms(&self) -> f32 {
        let t = self.updated_ms.load(Ordering::Relaxed);
        if t == 0 {
            return 0.0;
        }
        now_ms().saturating_sub(t) as f32
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_millis() as u64
}

pub fn list_input_devices() -> anyhow::Result<()> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .context("enumerate input devices")?;

    let mut out = io::stdout();
    writeln!(out, "Input devices:")?;
    for dev in devices {
        let name = dev.name().unwrap_or_else(|_| "<unknown>".to_string());
        writeln!(out, "  - {}", name)?;
    }
    Ok(())
}

enum AudioBackend {
    Cpal(cpal::Stream),
    #[cfg(target_os = "macos")]
    ScreenCaptureKit(SystemAudioStream),
}

#[cfg(target_os = "macos")]
struct SystemAudioStream {
    stream: SCStream,
}

pub struct AudioSystem {
    backend: AudioBackend,
    stop: Arc<AtomicBool>,
    analyzer_handle: Option<thread::JoinHandle<()>>,
    features: Arc<AtomicAudioFeatures>,
    pub sample_rate_hz: u32,
}

impl AudioSystem {
    pub fn new(source: AudioSource, device_query: Option<&str>) -> anyhow::Result<Self> {
        match source {
            AudioSource::Mic => Self::new_mic(device_query),
            AudioSource::System => Self::new_system(),
        }
    }

    fn new_mic(device_query: Option<&str>) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = select_mic_input_device(&host, device_query)?;
        let supported = device
            .default_input_config()
            .context("get default input config")?;
        let sample_rate_hz = supported.sample_rate().0;
        let channels = supported.channels() as usize;
        let config: cpal::StreamConfig = supported.clone().into();

        let rb_capacity = (sample_rate_hz as usize).saturating_mul(4);
        let rb = HeapRb::<f32>::new(rb_capacity);
        let (mut prod, mut cons) = rb.split();

        let stop = Arc::new(AtomicBool::new(false));
        let features = Arc::new(AtomicAudioFeatures::new());
        let features_for_thread = Arc::clone(&features);
        let stop_for_thread = Arc::clone(&stop);

        let err_fn = |err| eprintln!("audio stream error: {err}");

        let stream = match supported.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &config,
                move |data: &[f32], _| push_interleaved(data, channels, &mut prod),
                err_fn,
                None,
            )?,
            SampleFormat::I16 => device.build_input_stream(
                &config,
                move |data: &[i16], _| push_interleaved(data, channels, &mut prod),
                err_fn,
                None,
            )?,
            SampleFormat::U16 => device.build_input_stream(
                &config,
                move |data: &[u16], _| push_interleaved(data, channels, &mut prod),
                err_fn,
                None,
            )?,
            fmt => return Err(anyhow!("unsupported sample format: {fmt:?}")),
        };

        stream.play().context("start input stream")?;

        let analyzer_handle = thread::spawn(move || {
            analyze_loop(
                &mut cons,
                sample_rate_hz,
                &stop_for_thread,
                &features_for_thread,
            )
        });

        Ok(Self {
            backend: AudioBackend::Cpal(stream),
            stop,
            analyzer_handle: Some(analyzer_handle),
            features,
            sample_rate_hz,
        })
    }

    fn new_system() -> anyhow::Result<Self> {
        #[cfg(not(target_os = "macos"))]
        {
            Err(anyhow!("--source system is only supported on macOS in v1"))
        }

        #[cfg(target_os = "macos")]
        {
            let sample_rate_hz = 48_000u32;

            let rb_capacity = (sample_rate_hz as usize).saturating_mul(4);
            let rb = HeapRb::<f32>::new(rb_capacity);
            let (prod, mut cons) = rb.split();

            let stop = Arc::new(AtomicBool::new(false));
            let features = Arc::new(AtomicAudioFeatures::new());
            let features_for_thread = Arc::clone(&features);
            let stop_for_thread = Arc::clone(&stop);

            let handler = SystemAudioHandler {
                prod: std::sync::Mutex::new(prod),
            };

            let stream = start_system_audio_stream(handler)
                .context("start system audio capture (ScreenCaptureKit)")?;

            let analyzer_handle = thread::spawn(move || {
                analyze_loop(
                    &mut cons,
                    sample_rate_hz,
                    &stop_for_thread,
                    &features_for_thread,
                )
            });

            Ok(Self {
                backend: AudioBackend::ScreenCaptureKit(SystemAudioStream { stream }),
                stop,
                analyzer_handle: Some(analyzer_handle),
                features,
                sample_rate_hz,
            })
        }
    }

    pub fn features(&self) -> Arc<AtomicAudioFeatures> {
        Arc::clone(&self.features)
    }
}

impl Drop for AudioSystem {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.analyzer_handle.take() {
            let _ = h.join();
        }

        match &mut self.backend {
            // Keep the mic stream alive for the full AudioSystem lifetime.
            // Explicitly matching here makes that intent visible and avoids dead-code warnings.
            AudioBackend::Cpal(_stream) => {}
            #[cfg(target_os = "macos")]
            AudioBackend::ScreenCaptureKit(s) => {
                let _ = s.stream.stop_capture();
            }
        }
    }
}

fn select_mic_input_device(
    host: &cpal::Host,
    device_query: Option<&str>,
) -> anyhow::Result<cpal::Device> {
    let devices = host
        .input_devices()
        .context("enumerate input devices")?
        .collect::<Vec<_>>();

    let want = device_query.map(|s| s.to_lowercase());
    if let Some(want) = want.as_deref() {
        if let Some(dev) = devices.iter().find(|d| {
            d.name()
                .map(|n| n.to_lowercase().contains(want))
                .unwrap_or(false)
        }) {
            return Ok(dev.clone());
        }
        return Err(anyhow!("no input device matching: {want}"));
    }

    host.default_input_device()
        .ok_or_else(|| anyhow!("no default input device found"))
}

fn push_interleaved<T: Sample<Float = f32> + Copy>(
    data: &[T],
    channels: usize,
    prod: &mut ringbuf::HeapProd<f32>,
) {
    for frame in data.chunks(channels) {
        let mut acc = 0.0f32;
        for s in frame {
            acc += (*s).to_float_sample();
        }
        let mono = acc / channels as f32;
        let _ = prod.try_push(mono);
    }
}

#[cfg(target_os = "macos")]
struct SystemAudioHandler {
    prod: std::sync::Mutex<ringbuf::HeapProd<f32>>,
}

#[cfg(target_os = "macos")]
impl SCStreamOutputTrait for SystemAudioHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        if !matches!(of_type, SCStreamOutputType::Audio) {
            return;
        }

        let _ = sample.make_data_ready();
        let Some(fmt) = sample.format_description() else {
            return;
        };
        if fmt.audio_is_big_endian() {
            // Extremely rare on modern macOS; skip rather than mis-decode.
            return;
        }

        let is_float = fmt.audio_is_float();
        let bits = fmt.audio_bits_per_channel().unwrap_or(32);
        let channels = fmt
            .audio_channel_count()
            .unwrap_or(2)
            .max(1) as usize;

        let Some(abl) = sample.audio_buffer_list() else {
            return;
        };

        let Ok(mut prod) = self.prod.lock() else {
            return;
        };

        if is_float && bits == 32 {
            push_audio_f32(&abl, channels, &mut *prod);
        } else if !is_float && bits == 16 {
            push_audio_i16(&abl, channels, &mut *prod);
        }
    }
}

#[cfg(target_os = "macos")]
fn start_system_audio_stream(handler: impl SCStreamOutputTrait + 'static) -> anyhow::Result<SCStream> {
    // ScreenCaptureKit is the only supported "no virtual device" path for system audio on macOS.
    // It requires Screen Recording permission for the *terminal app* (Ghostty) in:
    // System Settings -> Privacy & Security -> Screen Recording
    let content = SCShareableContent::get().context(
        "SCShareableContent::get() (macOS Screen Recording permission required for Ghostty)",
    )?;
    let displays = content.displays();
    let display = displays
        .get(0)
        .ok_or_else(|| anyhow!("no displays found (ScreenCaptureKit)"))?;

    let filter = SCContentFilter::create()
        .with_display(display)
        .with_excluding_windows(&[])
        .build();

    // We only care about audio; keep video work minimal but *don't* throttle the stream too hard,
    // or audio arrives in large buffered chunks (visible latency).
    let config = SCStreamConfiguration::new()
        .with_width(2)
        .with_height(2)
        .with_queue_depth(1)
        .with_fps(60)
        .with_captures_audio(true)
        .with_sample_rate(48_000)
        .with_channel_count(2);

    let mut stream = SCStream::new(&filter, &config);
    stream.add_output_handler(handler, SCStreamOutputType::Audio);
    stream
        .start_capture()
        .context("SCStream::start_capture()")?;
    Ok(stream)
}

#[cfg(target_os = "macos")]
fn push_audio_f32(list: &AudioBufferList, channels: usize, prod: &mut ringbuf::HeapProd<f32>) {
    if list.num_buffers() == 1 {
        if let Some(buf) = list.get(0) {
            let data = buf.data();
            if data.is_empty() {
                return;
            }
            let n = data.len() / 4;
            let samples: &[f32] = unsafe { std::slice::from_raw_parts(data.as_ptr().cast(), n) };

            let ch = (buf.number_channels as usize).max(1);
            let ch = ch.max(channels);
            for frame in samples.chunks_exact(ch) {
                let mut acc = 0.0f32;
                for s in frame.iter().take(channels) {
                    acc += *s;
                }
                let mono = (acc / channels as f32).clamp(-1.0, 1.0);
                let _ = prod.try_push(mono);
            }
        }
        return;
    }

    // Planar: one buffer per channel (common), or multiple buffers we average.
    let mut chans: Vec<&[f32]> = Vec::new();
    for buf in list.iter() {
        let data = buf.data();
        if data.is_empty() {
            continue;
        }
        let n = data.len() / 4;
        let samples: &[f32] = unsafe { std::slice::from_raw_parts(data.as_ptr().cast(), n) };
        chans.push(samples);
        if chans.len() >= channels {
            break;
        }
    }
    if chans.is_empty() {
        return;
    }
    let frames = chans.iter().map(|c| c.len()).min().unwrap_or(0);
    for i in 0..frames {
        let mut acc = 0.0f32;
        let mut k = 0u32;
        for c in chans.iter().take(channels) {
            acc += c[i];
            k += 1;
        }
        if k == 0 {
            continue;
        }
        let mono = (acc / k as f32).clamp(-1.0, 1.0);
        let _ = prod.try_push(mono);
    }
}

#[cfg(target_os = "macos")]
fn push_audio_i16(list: &AudioBufferList, channels: usize, prod: &mut ringbuf::HeapProd<f32>) {
    if list.num_buffers() == 1 {
        if let Some(buf) = list.get(0) {
            let data = buf.data();
            if data.is_empty() {
                return;
            }
            let n = data.len() / 2;
            let samples: &[i16] = unsafe { std::slice::from_raw_parts(data.as_ptr().cast(), n) };

            let ch = (buf.number_channels as usize).max(1);
            let ch = ch.max(channels);
            for frame in samples.chunks_exact(ch) {
                let mut acc = 0.0f32;
                for s in frame.iter().take(channels) {
                    acc += (*s as f32) / 32768.0;
                }
                let mono = (acc / channels as f32).clamp(-1.0, 1.0);
                let _ = prod.try_push(mono);
            }
        }
        return;
    }

    let mut chans: Vec<&[i16]> = Vec::new();
    for buf in list.iter() {
        let data = buf.data();
        if data.is_empty() {
            continue;
        }
        let n = data.len() / 2;
        let samples: &[i16] = unsafe { std::slice::from_raw_parts(data.as_ptr().cast(), n) };
        chans.push(samples);
        if chans.len() >= channels {
            break;
        }
    }
    if chans.is_empty() {
        return;
    }
    let frames = chans.iter().map(|c| c.len()).min().unwrap_or(0);
    for i in 0..frames {
        let mut acc = 0.0f32;
        let mut k = 0u32;
        for c in chans.iter().take(channels) {
            acc += (c[i] as f32) / 32768.0;
            k += 1;
        }
        if k == 0 {
            continue;
        }
        let mono = (acc / k as f32).clamp(-1.0, 1.0);
        let _ = prod.try_push(mono);
    }
}

fn analyze_loop(
    cons: &mut ringbuf::HeapCons<f32>,
    sample_rate_hz: u32,
    stop: &AtomicBool,
    features: &AtomicAudioFeatures,
) {
    // Smaller windows reduce analysis latency (especially noticeable on system audio capture).
    let n = 1024usize;
    let hop = 256usize;

    let mut scratch = vec![0.0f32; n];
    let mut write_pos = 0usize;
    let mut filled = 0usize;
    let mut since_last = 0usize;

    let hann = (0..n)
        .map(|i| 0.5 - 0.5 * ((2.0 * PI * i as f32) / (n as f32)).cos())
        .collect::<Vec<_>>();

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let mut fft_buf = vec![Complex { re: 0.0, im: 0.0 }; n];
    let mut mags = vec![0.0f32; n / 2];
    let mut prev_mags = vec![0.0f32; n / 2];

    let mut flux_avg = 0.0f32;
    let mut flux_hist = [0.0f32; 3];

    let mut rms_s = 0.0f32;
    let mut bands_s = [0.0f32; 8];
    let mut centroid_s = 0.0f32;
    let mut flatness_s = 0.0f32;

    while !stop.load(Ordering::Relaxed) {
        let mut got_any = false;
        while let Some(s) = cons.try_pop() {
            got_any = true;
            scratch[write_pos] = s;
            write_pos = (write_pos + 1) % n;
            if filled < n {
                filled += 1;
            }
            since_last += 1;
            if filled == n && since_last >= hop {
                since_last = 0;
                let (rms, bands, flux, centroid, flatness) = analyze_window(
                    &scratch,
                    write_pos,
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

                // Peak detection with 1-step latency.
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

                features.store(AudioFeatures {
                    rms: rms_s,
                    bands: bands_s,
                    onset: flux,
                    beat,
                    beat_strength,
                    centroid: centroid_s,
                    flatness: flatness_s,
                });
            }
        }

        if !got_any {
            thread::sleep(Duration::from_millis(1));
        }
    }
}

fn analyze_window(
    scratch: &[f32],
    write_pos: usize,
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
        let s = scratch[(write_pos + i) % n];
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
    // Keep onset magnitude roughly stable when FFT size changes.
    let flux_scale = 0.002 * (1024.0 / (half as f32).max(1.0));
    flux = (flux * flux_scale).tanh();

    // Bands: sub, bass, lowmid, mid, highmid, treb, air, presence
    let edges_hz = [20.0, 60.0, 150.0, 400.0, 1000.0, 2500.0, 6000.0, 12000.0, 20000.0];
    let mut bands = [0.0f32; 8];
    let mut counts = [0u32; 8];
    let sr = sample_rate_hz as f32;
    for i in 1..half {
        let f = (i as f32) * sr / (n as f32);
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
        let m = mags[i];
        bands[band] += m;
        counts[band] += 1;
    }
    for i in 0..bands.len() {
        let denom = counts[i].max(1) as f32;
        // Log-ish compression -> 0..1
        let e = (bands[i] / denom) * 0.01;
        bands[i] = e.tanh();
    }

    // Spectral centroid (0..1)
    let mut num = 0.0f32;
    let mut den = 0.0f32;
    for i in 1..half {
        let f = (i as f32) * sr / (n as f32);
        let m = mags[i];
        num += f * m;
        den += m;
    }
    let centroid = if den > 1e-6 {
        (num / den / 8000.0).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Spectral flatness (0..1) over mid-high bins
    let start_bin = (400.0 * (n as f32) / sr) as usize;
    let end_bin = (6000.0 * (n as f32) / sr) as usize;
    let mut log_gm = 0.0f32;
    let mut am = 0.0f32;
    let mut k = 0u32;
    for i in start_bin.clamp(1, half - 1)..end_bin.clamp(1, half) {
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
