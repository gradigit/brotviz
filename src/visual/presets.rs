use crate::audio::AudioFeatures;
use crate::config::Quality;
use std::f32::consts::PI;
use std::time::Instant;

pub struct RenderCtx {
    pub now: Instant,
    pub t: f32,
    pub dt: f32,
    pub w: usize,
    pub h: usize,
    pub audio: AudioFeatures,
    pub beat_pulse: f32,
    pub fractal_zoom_mul: f32,
    pub safe: bool,
    pub quality: Quality,
    pub scale: usize,
}

pub trait Preset {
    fn name(&self) -> &'static str;
    fn render(&mut self, ctx: &RenderCtx, prev: &[u8], out: &mut [u8]);
    fn on_resize(&mut self, _w: usize, _h: usize) {}
}

pub fn make_presets() -> Vec<Box<dyn Preset>> {
    use Algo::*;
    use Palette::*;

    let mut v: Vec<Box<dyn Preset>> = Vec::new();

    v.push(Box::new(FieldPreset::new(
        "Mandelbrot: Bass Zoom",
        Mandelbrot { center: (-0.65, 0.0) },
        Prism,
        Feedback::tunnel(0.88, 0.018, 1.5),
    )));
    v.push(Box::new(FieldPreset::new(
        "Julia: Treble Shimmer",
        Julia { c_base: (-0.8, 0.156) },
        Neon,
        Feedback::tunnel(0.9, 0.014, 1.2),
    )));
    v.push(Box::new(FieldPreset::new(
        "Fractal Flame: Beat Burst",
        Julia { c_base: (0.285, 0.01) },
        Fire,
        Feedback::tunnel(0.92, 0.02, 1.8),
    )));
    v.push(Box::new(FieldPreset::new(
        "Feedback Tunnel: Tempo Spin",
        Plasma { freq: 2.4 },
        Aurora,
        Feedback::tunnel(0.94, 0.03, 2.2),
    )));
    v.push(Box::new(FieldPreset::new(
        "Plasma Kaleidoscope",
        Kaleido { freq: 2.7, symmetry: 7 },
        Acid,
        Feedback::tunnel(0.9, 0.02, 1.4),
    )));
    v.push(Box::new(FieldPreset::new(
        "Neon Grid Warp",
        Stripes { freq: 14.0 },
        Neon,
        Feedback::tunnel(0.93, 0.022, 1.7),
    )));
    v.push(Box::new(FieldPreset::new(
        "Voronoi Shatter: Beat Cuts",
        Voronoi { points: 6 },
        Prism,
        Feedback::tunnel(0.88, 0.03, 1.6),
    )));
    v.push(Box::new(FieldPreset::new(
        "Metaballs: Sub Bass Pump",
        Metaballs { blobs: 5 },
        Aurora,
        Feedback::tunnel(0.9, 0.02, 1.2),
    )));
    v.push(Box::new(FieldPreset::new(
        "Particle Fountain: Hi-hat Sparks",
        Sparks { density: 1.0 },
        Neon,
        Feedback::tunnel(0.86, 0.018, 1.0),
    )));
    v.push(Box::new(FieldPreset::new(
        "Starfield: Kick Accel",
        Starfield { depth: 1.0 },
        Prism,
        Feedback::none(),
    )));
    v.push(Box::new(FieldPreset::new(
        "Flow Field: Vocal Sway",
        Flow { freq: 1.6 },
        Aurora,
        Feedback::tunnel(0.95, 0.016, 0.9),
    )));
    v.push(Box::new(FieldPreset::new(
        "Chromatic Waves",
        Rings { freq: 6.5 },
        Prism,
        Feedback::tunnel(0.9, 0.02, 1.1),
    )));
    v.push(Box::new(FieldPreset::new(
        "Spectrum Vortex",
        Vortex { spin: 1.2 },
        Acid,
        Feedback::tunnel(0.93, 0.02, 1.4),
    )));
    v.push(Box::new(FieldPreset::new(
        "Reaction-Diffusion Lite",
        Smoke { blur: 0.55 },
        Aurora,
        Feedback::tunnel(0.97, 0.01, 0.6),
    )));
    v.push(Box::new(FieldPreset::new(
        "Cellular Automata: Beat Seeding",
        Cells { scale: 8.0 },
        Acid,
        Feedback::tunnel(0.9, 0.02, 0.8),
    )));
    v.push(Box::new(FieldPreset::new(
        "Glitch Mosaic: Transient Trigger",
        Glitch { block: 10.0 },
        Neon,
        Feedback::tunnel(0.86, 0.02, 1.0),
    )));
    v.push(Box::new(FieldPreset::new(
        "Concentric Rings: Snare Flash",
        Rings { freq: 10.0 },
        Fire,
        Feedback::tunnel(0.9, 0.02, 1.0),
    )));
    v.push(Box::new(FieldPreset::new(
        "Heatmap Smoke",
        Smoke { blur: 0.75 },
        Fire,
        Feedback::tunnel(0.98, 0.01, 0.5),
    )));
    v.push(Box::new(FieldPreset::new(
        "Phase-Shift Stripes",
        Stripes { freq: 22.0 },
        Prism,
        Feedback::tunnel(0.9, 0.02, 1.4),
    )));
    v.push(Box::new(FieldPreset::new(
        "Prism Noise: Treble Rain",
        Noise { freq: 3.0 },
        Prism,
        Feedback::tunnel(0.9, 0.02, 1.1),
    )));

    // v1.1: +20 presets (more fractals, warps, attractor fields, and geometric tiles).
    v.push(Box::new(FieldPreset::new(
        "Burning Ship: Bass Sink",
        BurningShip { center: (-0.45, -0.02) },
        Fire,
        Feedback::tunnel(0.90, 0.022, 1.55),
    )));
    v.push(Box::new(FieldPreset::new(
        "Orbit Trap: Neon Bloom",
        OrbitTrap {
            center: (-0.38, 0.58),
            trap: (0.15, 0.05),
        },
        Neon,
        Feedback::tunnel(0.93, 0.020, 1.25),
    )));
    v.push(Box::new(FieldPreset::new(
        "Clifford Field: Acid Lace",
        Clifford,
        Cosmic,
        Feedback::tunnel(0.92, 0.020, 1.15),
    )));
    v.push(Box::new(FieldPreset::new(
        "de Jong Field: Neon Knots",
        DeJong,
        Neon,
        Feedback::tunnel(0.90, 0.024, 1.10),
    )));
    v.push(Box::new(FieldPreset::new(
        "Domain Warp: Candy Melt",
        Warp { freq: 2.9 },
        Cosmic,
        Feedback::tunnel(0.95, 0.016, 0.95),
    )));
    v.push(Box::new(FieldPreset::new(
        "Polar Moire: Treble Sheen",
        PolarMoire { freq: 1.0 },
        Prism,
        Feedback::tunnel(0.92, 0.020, 1.10),
    )));
    v.push(Box::new(FieldPreset::new(
        "Truchet Tiles: Groove",
        Truchet { tiles: 8.0 },
        Aurora,
        Feedback::tunnel(0.90, 0.018, 1.05),
    )));
    v.push(Box::new(FieldPreset::new(
        "SDF Orbs: Beat Pop",
        Orbs { freq: 2.4 },
        Neon,
        Feedback::tunnel(0.88, 0.020, 1.05),
    )));
    v.push(Box::new(FieldPreset::new(
        "Chladni Plates: Bass Lines",
        Chladni { a: 2.0, b: 2.0 },
        Prism,
        Feedback::none(),
    )));
    v.push(Box::new(FieldPreset::new(
        "CRT Scanlines: VHS Glow",
        Crt { freq: 320.0 },
        Cosmic,
        Feedback::tunnel(0.96, 0.012, 0.85),
    )));
    v.push(Box::new(FieldPreset::new(
        "Kaleido Mandala",
        Kaleido { freq: 4.2, symmetry: 11 },
        Cosmic,
        Feedback::tunnel(0.92, 0.020, 1.35),
    )));
    v.push(Box::new(FieldPreset::new(
        "Moire Interference",
        Moire { freq: 18.0 },
        Prism,
        Feedback::tunnel(0.90, 0.020, 1.20),
    )));
    v.push(Box::new(FieldPreset::new(
        "Starfield: Hyperdrive",
        Starfield { depth: 1.6 },
        Prism,
        Feedback::tunnel(0.92, 0.016, 1.05),
    )));
    v.push(Box::new(FieldPreset::new(
        "Metaballs: Neon Lava",
        Metaballs { blobs: 8 },
        Fire,
        Feedback::tunnel(0.93, 0.020, 1.15),
    )));
    v.push(Box::new(FieldPreset::new(
        "Voronoi: Crystal Lattice",
        Voronoi { points: 10 },
        Aurora,
        Feedback::tunnel(0.90, 0.026, 1.35),
    )));
    v.push(Box::new(FieldPreset::new(
        "Plasma: Aurora Storm",
        Plasma { freq: 3.6 },
        Aurora,
        Feedback::tunnel(0.94, 0.020, 1.25),
    )));
    v.push(Box::new(FieldPreset::new(
        "Vortex: Warpdrive",
        Vortex { spin: 2.0 },
        Acid,
        Feedback::tunnel(0.93, 0.020, 1.30),
    )));
    v.push(Box::new(FieldPreset::new(
        "Stripes: Hypersync",
        Stripes { freq: 32.0 },
        Cosmic,
        Feedback::tunnel(0.90, 0.020, 1.20),
    )));
    v.push(Box::new(FieldPreset::new(
        "Glitch Blocks: DataMosh Drift",
        Glitch { block: 6.0 },
        Neon,
        Feedback::tunnel(0.86, 0.020, 1.05),
    )));
    v.push(Box::new(FieldPreset::new(
        "Noise Ribbons: Treble Drift",
        Noise { freq: 5.2 },
        Cosmic,
        Feedback::tunnel(0.92, 0.018, 1.18),
    )));

    // v1.2: deep fractal zoom pack (continuous dive + orbit drift).
    v.push(Box::new(FieldPreset::new(
        "Mandelbrot: Infinite Dive",
        MandelDeep {
            center: (-0.7436439, 0.13182591),
            orbit: (0.52, 0.38),
            speed: 0.92,
        },
        Prism,
        Feedback::tunnel(0.90, 0.018, 1.30),
    )));
    v.push(Box::new(FieldPreset::new(
        "Mandelbrot: Seahorse Zoom",
        MandelDeep {
            center: (-0.7453, 0.1127),
            orbit: (0.70, 0.46),
            speed: 0.86,
        },
        Cosmic,
        Feedback::tunnel(0.91, 0.018, 1.28),
    )));
    v.push(Box::new(FieldPreset::new(
        "Mandelbrot: Spiral Probe",
        MandelDeep {
            center: (-0.1011, 0.9563),
            orbit: (0.58, 0.62),
            speed: 0.84,
        },
        Neon,
        Feedback::tunnel(0.90, 0.020, 1.32),
    )));
    v.push(Box::new(FieldPreset::new(
        "Julia: Infinite Bloom",
        JuliaDeep {
            c_base: (-0.745, 0.186),
            speed: 0.88,
        },
        Acid,
        Feedback::tunnel(0.91, 0.020, 1.26),
    )));
    v.push(Box::new(FieldPreset::new(
        "Julia: Cathedral Zoom",
        JuliaDeep {
            c_base: (-0.391, -0.587),
            speed: 0.82,
        },
        Aurora,
        Feedback::tunnel(0.92, 0.018, 1.24),
    )));
    v.push(Box::new(FieldPreset::new(
        "Burning Ship: Abyss Dive",
        BurningShipDeep {
            center: (-1.7443, -0.0173),
            speed: 0.78,
        },
        Fire,
        Feedback::tunnel(0.90, 0.022, 1.30),
    )));

    // v1.3: research pack (reaction-diffusion / fluid / flame / sphere-trace inspired).
    v.push(Box::new(FieldPreset::new(
        "Reaction-Diffusion: Psychedelic Bloom",
        Smoke { blur: 0.62 },
        Acid,
        Feedback::tunnel(0.97, 0.012, 0.72),
    )));
    v.push(Box::new(FieldPreset::new(
        "Fluid Vorticity: Bass Storm",
        Flow { freq: 2.8 },
        Aurora,
        Feedback::tunnel(0.95, 0.015, 0.85),
    )));
    v.push(Box::new(FieldPreset::new(
        "Fractal Flame: IFS Cathedral",
        Warp { freq: 3.2 },
        Fire,
        Feedback::tunnel(0.92, 0.020, 1.18),
    )));
    v.push(Box::new(FieldPreset::new(
        "Mandelbulb Slice: Neon Relic",
        OrbitTrap {
            center: (-0.42, 0.57),
            trap: (0.18, 0.06),
        },
        Neon,
        Feedback::tunnel(0.91, 0.020, 1.22),
    )));
    v.push(Box::new(FieldPreset::new(
        "Sphere Trace: Gyroid Temple",
        Orbs { freq: 3.1 },
        Cosmic,
        Feedback::tunnel(0.92, 0.018, 1.08),
    )));
    v.push(Box::new(FieldPreset::new(
        "Curl Noise: Plasma Veins",
        Noise { freq: 6.0 },
        Prism,
        Feedback::tunnel(0.93, 0.018, 1.10),
    )));
    v.push(Box::new(FieldPreset::new(
        "Perlin Warp: Liquid Aurora",
        Plasma { freq: 4.6 },
        Aurora,
        Feedback::tunnel(0.94, 0.018, 1.16),
    )));
    v.push(Box::new(FieldPreset::new(
        "IFS Attractor: Ribbon Knot",
        Clifford,
        Acid,
        Feedback::tunnel(0.91, 0.020, 1.06),
    )));
    v.push(Box::new(FieldPreset::new(
        "Fractal Morph: Wormhole Garden",
        Kaleido { freq: 5.0, symmetry: 13 },
        Cosmic,
        Feedback::tunnel(0.92, 0.020, 1.30),
    )));
    v.push(Box::new(FieldPreset::new(
        "SDF Fractal: Cosmic Monolith",
        Orbs { freq: 4.0 },
        Neon,
        Feedback::tunnel(0.90, 0.022, 1.15),
    )));

    v
}

#[derive(Clone, Copy)]
enum Palette {
    Prism,
    Acid,
    Neon,
    Fire,
    Aurora,
    Cosmic,
}

#[derive(Clone, Copy)]
enum Algo {
    Mandelbrot { center: (f32, f32) },
    MandelDeep { center: (f32, f32), orbit: (f32, f32), speed: f32 },
    BurningShip { center: (f32, f32) },
    BurningShipDeep { center: (f32, f32), speed: f32 },
    OrbitTrap { center: (f32, f32), trap: (f32, f32) },
    Julia { c_base: (f32, f32) },
    JuliaDeep { c_base: (f32, f32), speed: f32 },
    Clifford,
    DeJong,
    Plasma { freq: f32 },
    Warp { freq: f32 },
    PolarMoire { freq: f32 },
    Kaleido { freq: f32, symmetry: u32 },
    Stripes { freq: f32 },
    Voronoi { points: u32 },
    Metaballs { blobs: u32 },
    Sparks { density: f32 },
    Starfield { depth: f32 },
    Flow { freq: f32 },
    Rings { freq: f32 },
    Vortex { spin: f32 },
    Smoke { blur: f32 },
    Cells { scale: f32 },
    Glitch { block: f32 },
    Noise { freq: f32 },
    Truchet { tiles: f32 },
    Orbs { freq: f32 },
    Chladni { a: f32, b: f32 },
    Crt { freq: f32 },
    Moire { freq: f32 },
}

#[derive(Clone, Copy)]
struct Feedback {
    fade: f32,
    warp_amp: f32,
    warp_freq: f32,
    zoom: f32,
    strength: f32,
}

impl Feedback {
    fn none() -> Self {
        Self {
            fade: 1.0,
            warp_amp: 0.0,
            warp_freq: 0.0,
            zoom: 1.0,
            strength: 0.0,
        }
    }

    fn tunnel(fade: f32, warp_amp: f32, zoom: f32) -> Self {
        Self {
            fade: fade.clamp(0.0, 1.0),
            warp_amp,
            warp_freq: 2.2,
            zoom,
            strength: 1.0,
        }
    }
}

pub struct FieldPreset {
    name: &'static str,
    algo: Algo,
    palette: Palette,
    fb: Feedback,
    seed: u32,
}

impl FieldPreset {
    fn new(name: &'static str, algo: Algo, palette: Palette, fb: Feedback) -> Self {
        Self {
            name,
            algo,
            palette,
            fb,
            seed: fastrand::u32(..),
        }
    }
}

impl Preset for FieldPreset {
    fn name(&self) -> &'static str {
        self.name
    }

    fn render(&mut self, ctx: &RenderCtx, prev: &[u8], out: &mut [u8]) {
        let w = ctx.w.max(1);
        let h = ctx.h.max(1);
        let scale = ctx.scale.max(1);

        let bass = ctx.audio.bands[1];
        let mid = ctx.audio.bands[3];
        let treb = ctx.audio.bands[6];
        let energy = ctx.audio.rms;

        let beat_pulse = if ctx.safe {
            (ctx.beat_pulse * 0.6).min(0.6)
        } else {
            ctx.beat_pulse
        };

        let zoom_mod = 1.0 - bass * 0.14 - beat_pulse * 0.08;
        let t = ctx.t;

        // Fill in blocks to allow adaptive downscale without a second buffer.
        for by in (0..h).step_by(scale) {
            for bx in (0..w).step_by(scale) {
                let x = bx as f32 / w as f32;
                let y = by as f32 / h as f32;
                let nx = x * 2.0 - 1.0;
                let ny = y * 2.0 - 1.0;

                let (sx, sy) = match self.algo {
                    Algo::Kaleido { symmetry, .. } => kaleido(nx, ny, symmetry),
                    _ => (nx, ny),
                };

                // Feedback base layer (warp previous frame into a tunnel).
                let mut base = [0u8, 0u8, 0u8];
                if self.fb.strength > 0.0 && !prev.is_empty() {
                    let ang = t * (0.5 + mid * 1.2);
                    let ca = ang.cos();
                    let sa = ang.sin();
                    let rx = sx * ca - sy * sa;
                    let ry = sx * sa + sy * ca;

                    let wamp = self.fb.warp_amp * (0.4 + treb * 1.8 + beat_pulse * 1.2);
                    let dx = (rx * self.fb.warp_freq + t * 1.7).sin()
                        + hash_noise(rx * 3.0, ry * 3.0, self.seed).sin() * 0.6;
                    let dy = (ry * self.fb.warp_freq - t * 1.3).cos()
                        + hash_noise(rx * 2.0, ry * 2.0, self.seed ^ 0x9E37_79B9).cos() * 0.6;

                    let z = (self.fb.zoom * zoom_mod).max(0.2);
                    let u = rx / z + dx * wamp;
                    let v = ry / z + dy * wamp;

                    base = sample_rgb(prev, w, h, u, v);
                    base[0] = (base[0] as f32 * self.fb.fade) as u8;
                    base[1] = (base[1] as f32 * self.fb.fade) as u8;
                    base[2] = (base[2] as f32 * self.fb.fade) as u8;
                }

                // Main field value (0..1)
                let mut val = match self.algo {
                    Algo::Mandelbrot { center } => {
                        let (fx, fy) =
                            fractal_motion_xy(sx, sy, t, ctx.fractal_zoom_mul, bass, mid, treb, beat_pulse);
                        let fz = fractal_zoom_motion(t, ctx.fractal_zoom_mul, bass, beat_pulse);
                        let zoom = 1.7 * zoom_mod * fz;
                        fractal_mandelbrot(
                            fx,
                            fy,
                            t,
                            center.0 + 0.04 * (t * 0.3).sin(),
                            center.1 + 0.03 * (t * 0.23).cos(),
                            zoom,
                            ctx.quality,
                        )
                    }
                    Algo::MandelDeep { center, orbit, speed } => {
                        let (fx, fy) =
                            fractal_motion_xy(sx, sy, t, ctx.fractal_zoom_mul, bass, mid, treb, beat_pulse);
                        fractal_mandelbrot_deep(
                            fx,
                            fy,
                            t,
                            center,
                            orbit,
                            speed,
                            ctx.fractal_zoom_mul,
                            bass,
                            mid,
                            treb,
                            beat_pulse,
                            ctx.quality,
                        )
                    }
                    Algo::BurningShip { center } => {
                        let (fx, fy) =
                            fractal_motion_xy(sx, sy, t, ctx.fractal_zoom_mul, bass, mid, treb, beat_pulse);
                        let fz = fractal_zoom_motion(t, ctx.fractal_zoom_mul, bass, beat_pulse);
                        let zoom = 1.55 * zoom_mod * fz;
                        fractal_burning_ship(
                            fx,
                            fy,
                            t,
                            center.0 + 0.03 * (t * 0.23).sin(),
                            center.1 + 0.02 * (t * 0.19).cos(),
                            zoom,
                            ctx.quality,
                        )
                    }
                    Algo::BurningShipDeep { center, speed } => {
                        let (fx, fy) =
                            fractal_motion_xy(sx, sy, t, ctx.fractal_zoom_mul, bass, mid, treb, beat_pulse);
                        fractal_burning_ship_deep(
                            fx,
                            fy,
                            t,
                            center,
                            speed,
                            ctx.fractal_zoom_mul,
                            bass,
                            mid,
                            treb,
                            beat_pulse,
                            ctx.quality,
                        )
                    }
                    Algo::OrbitTrap { center, trap } => {
                        let (fx, fy) =
                            fractal_motion_xy(sx, sy, t, ctx.fractal_zoom_mul, bass, mid, treb, beat_pulse);
                        let fz = fractal_zoom_motion(t, ctx.fractal_zoom_mul, bass, beat_pulse);
                        let zoom = 1.55 * zoom_mod * fz;
                        fractal_orbit_trap(
                            fx,
                            fy,
                            t,
                            center.0 + 0.02 * (t * 0.17).sin(),
                            center.1 + 0.02 * (t * 0.13).cos(),
                            zoom,
                            ctx.quality,
                            trap.0,
                            trap.1,
                        )
                    }
                    Algo::Julia { c_base } => {
                        let (fx, fy) =
                            fractal_motion_xy(sx, sy, t, ctx.fractal_zoom_mul, bass, mid, treb, beat_pulse);
                        let fz = fractal_zoom_motion(t, ctx.fractal_zoom_mul, bass, beat_pulse);
                        let zoom = 1.35 * zoom_mod * fz;
                        let cx = c_base.0 + 0.16 * (t * (0.17 + treb)).cos() + mid * 0.05;
                        let cy = c_base.1 + 0.14 * (t * (0.19 + bass)).sin() - treb * 0.04;
                        fractal_julia(fx, fy, t, cx, cy, zoom, ctx.quality)
                    }
                    Algo::JuliaDeep { c_base, speed } => {
                        let (fx, fy) =
                            fractal_motion_xy(sx, sy, t, ctx.fractal_zoom_mul, bass, mid, treb, beat_pulse);
                        fractal_julia_deep(
                            fx,
                            fy,
                            t,
                            c_base,
                            speed,
                            ctx.fractal_zoom_mul,
                            bass,
                            mid,
                            treb,
                            beat_pulse,
                            ctx.quality,
                        )
                    }
                    Algo::Clifford => clifford_field(sx, sy, t, bass, mid, treb, ctx.quality),
                    Algo::DeJong => dejong_field(sx, sy, t, bass, mid, treb, ctx.quality),
                    Algo::Plasma { freq } => plasma(sx, sy, t, freq, bass, treb),
                    Algo::Warp { freq } => warp_candy(sx, sy, t, freq, bass, mid, treb, self.seed),
                    Algo::PolarMoire { freq } => polar_moire(sx, sy, t, freq, bass, mid, treb, beat_pulse),
                    Algo::Kaleido { freq, .. } => plasma(sx, sy, t, freq, bass, treb),
                    Algo::Stripes { freq } => stripes(sx, sy, t, freq, beat_pulse),
                    Algo::Voronoi { points } => voronoiish(sx, sy, t, points, self.seed),
                    Algo::Metaballs { blobs } => metaballs(sx, sy, t, blobs, self.seed),
                    Algo::Sparks { density } => sparks(sx, sy, t, density, treb, beat_pulse, self.seed),
                    Algo::Starfield { depth } => starfield(sx, sy, t, bass, depth, self.seed),
                    Algo::Flow { freq } => flow(sx, sy, t, freq, mid, self.seed),
                    Algo::Rings { freq } => rings(sx, sy, t, freq, bass, beat_pulse),
                    Algo::Vortex { spin } => vortex(sx, sy, t, spin, energy, bass, treb),
                    Algo::Smoke { blur } => smoke(prev, w, h, bx, by, blur),
                    Algo::Cells { scale } => cells(sx, sy, t, scale, beat_pulse, self.seed),
                    Algo::Glitch { block } => glitch(bx, by, w, h, t, block, ctx.audio.onset, self.seed),
                    Algo::Noise { freq } => noise(sx, sy, t, freq, self.seed),
                    Algo::Truchet { tiles } => truchet(sx, sy, t, tiles, bass, treb, self.seed),
                    Algo::Orbs { freq } => orbs(sx, sy, t, freq, bass, beat_pulse),
                    Algo::Chladni { a, b } => chladni(sx, sy, t, a, b, bass, mid, treb),
                    Algo::Crt { freq } => crt_scan(sx, sy, t, freq, bass, mid, treb, beat_pulse, self.seed),
                    Algo::Moire { freq } => moire(sx, sy, t, freq, bass, treb, beat_pulse),
                };

                // Extra "psychedelic pop": beat injects energy into the field.
                val = (val + beat_pulse * 0.35 + treb * 0.18).fract();

                let ink = palette(self.palette, val, t, bass, mid, treb, beat_pulse);

                let ink_alpha = (0.55 + energy * 0.35 + beat_pulse * 0.35).clamp(0.2, 0.95);
                let r = (base[0] as f32 * (1.0 - ink_alpha) + ink[0] as f32 * ink_alpha) as u8;
                let g = (base[1] as f32 * (1.0 - ink_alpha) + ink[1] as f32 * ink_alpha) as u8;
                let b = (base[2] as f32 * (1.0 - ink_alpha) + ink[2] as f32 * ink_alpha) as u8;

                for dy in 0..scale {
                    for dx in 0..scale {
                        let x2 = bx + dx;
                        let y2 = by + dy;
                        if x2 >= w || y2 >= h {
                            continue;
                        }
                        let i = (y2 * w + x2) * 4;
                        out[i] = r;
                        out[i + 1] = g;
                        out[i + 2] = b;
                        out[i + 3] = 255;
                    }
                }
            }
        }
    }
}

fn fractal_mandelbrot(
    x: f32,
    y: f32,
    t: f32,
    cx: f32,
    cy: f32,
    zoom: f32,
    quality: Quality,
) -> f32 {
    let (max_iter, bail) = match quality {
        Quality::Ultra => (120u32, 4.0),
        Quality::High => (96u32, 4.0),
        Quality::Balanced => (72u32, 4.0),
        Quality::Fast => (48u32, 4.0),
    };

    let scale = 1.25 / zoom;
    let cr = cx + x * scale + 0.02 * (t * 0.15).sin();
    let ci = cy + y * scale + 0.02 * (t * 0.12).cos();

    let mut zr = 0.0f32;
    let mut zi = 0.0f32;
    let mut i = 0u32;
    while i < max_iter {
        let zr2 = zr * zr - zi * zi + cr;
        zi = 2.0 * zr * zi + ci;
        zr = zr2;
        if zr * zr + zi * zi > bail {
            break;
        }
        i += 1;
    }

    (i as f32 / max_iter as f32).sqrt()
}

fn fractal_zoom_motion(t: f32, zoom_mul: f32, bass: f32, beat: f32) -> f32 {
    if zoom_mul <= 0.0 {
        return 1.0;
    }
    let zm = zoom_mul.clamp(0.35, 2.5);
    let phase = (t * (0.11 + 0.09 * bass) * zm + beat * 0.05).fract();
    let e = phase * phase * (3.0 - 2.0 * phase); // smoothstep
    0.95 + (1.0 + 2.2 * zm) * e
}

fn fractal_motion_xy(
    x: f32,
    y: f32,
    t: f32,
    zoom_mul: f32,
    bass: f32,
    mid: f32,
    treb: f32,
    beat: f32,
) -> (f32, f32) {
    if zoom_mul <= 0.0 {
        return (x, y);
    }
    let zm = zoom_mul.clamp(0.35, 8.0);
    let phase = (t * (0.13 + 0.10 * bass) * zm + beat * 0.05).fract();
    let e = phase * phase * (3.0 - 2.0 * phase);
    let zcam = 2.0f32.powf(e * (1.0 + 1.2 * zm));
    let dx = 0.08 * (t * 0.43 + 0.7 * mid).sin();
    let dy = 0.08 * (t * 0.37 + 0.6 * treb).cos();
    ((x + dx) / zcam, (y + dy) / zcam)
}

fn fractal_julia(
    x: f32,
    y: f32,
    t: f32,
    cr: f32,
    ci: f32,
    zoom: f32,
    quality: Quality,
) -> f32 {
    let (max_iter, bail) = match quality {
        Quality::Ultra => (112u32, 4.0),
        Quality::High => (88u32, 4.0),
        Quality::Balanced => (64u32, 4.0),
        Quality::Fast => (44u32, 4.0),
    };

    let scale = 1.4 / zoom;
    let mut zr = x * scale;
    let mut zi = y * scale;
    let mut i = 0u32;
    while i < max_iter {
        let zr2 = zr * zr - zi * zi + cr;
        zi = 2.0 * zr * zi + ci + 0.01 * (t * 0.2).sin();
        zr = zr2;
        if zr * zr + zi * zi > bail {
            break;
        }
        i += 1;
    }
    (i as f32 / max_iter as f32).powf(0.65)
}

fn fractal_mandelbrot_deep(
    x: f32,
    y: f32,
    t: f32,
    center: (f32, f32),
    orbit: (f32, f32),
    speed: f32,
    zoom_mul: f32,
    bass: f32,
    mid: f32,
    treb: f32,
    beat: f32,
    quality: Quality,
) -> f32 {
    let max_iter = match quality {
        Quality::Ultra => 240u32,
        Quality::High => 192u32,
        Quality::Balanced => 144u32,
        Quality::Fast => 96u32,
    };

    let speed = if zoom_mul <= 0.0 {
        0.0
    } else {
        speed * zoom_mul.clamp(0.35, 2.5)
    };
    let phase = (t * speed * (0.22 + 0.20 * bass) + beat * 0.04).fract();
    let ease = phase * phase * (3.0 - 2.0 * phase);
    let power = 1.6 + 16.4 * ease;
    let zoom = 2.0f32.powf(power);
    let scale = 1.9 / zoom.max(1.0);

    let orbit_scale = scale * (40.0 + 40.0 * treb);
    let cx = center.0 + orbit.0 * (t * (0.17 + 0.12 * mid)).sin() * orbit_scale;
    let cy = center.1 + orbit.1 * (t * (0.13 + 0.10 * treb)).cos() * orbit_scale;
    let cr = cx + x * scale;
    let ci = cy + y * scale;

    let mut zr = 0.0f32;
    let mut zi = 0.0f32;
    let mut i = 0u32;
    let mut m2 = 0.0f32;
    while i < max_iter {
        let zr2 = zr * zr - zi * zi + cr;
        zi = 2.0 * zr * zi + ci;
        zr = zr2;
        m2 = zr * zr + zi * zi;
        if m2 > 256.0 {
            break;
        }
        i += 1;
    }
    if i >= max_iter {
        return 0.0;
    }

    let nu = i as f32 + 1.0 - (m2.max(1.0001).ln().ln() / std::f32::consts::LN_2);
    let n = (nu / max_iter as f32).clamp(0.0, 1.0);
    let stripe = ((nu * (0.10 + 0.06 * treb)) + t * (0.8 + 1.4 * beat)).sin() * 0.5 + 0.5;
    ((1.0 - n).powf(0.33) * 0.72 + stripe * 0.28).clamp(0.0, 1.0)
}

fn fractal_julia_deep(
    x: f32,
    y: f32,
    t: f32,
    c_base: (f32, f32),
    speed: f32,
    zoom_mul: f32,
    bass: f32,
    mid: f32,
    treb: f32,
    beat: f32,
    quality: Quality,
) -> f32 {
    let max_iter = match quality {
        Quality::Ultra => 220u32,
        Quality::High => 176u32,
        Quality::Balanced => 132u32,
        Quality::Fast => 88u32,
    };

    let speed = if zoom_mul <= 0.0 {
        0.0
    } else {
        speed * zoom_mul.clamp(0.35, 2.5)
    };
    let phase = (t * speed * (0.22 + 0.20 * mid) + beat * 0.04).fract();
    let ease = phase * phase * (3.0 - 2.0 * phase);
    let power = 1.2 + 15.8 * ease;
    let zoom = 2.0f32.powf(power);
    let scale = 1.8 / zoom.max(1.0);

    let cx = c_base.0 + 0.16 * (t * (0.21 + treb * 0.25)).cos() + bass * 0.05;
    let cy = c_base.1 + 0.15 * (t * (0.19 + bass * 0.22)).sin() - treb * 0.04;

    let mut zr = x * scale;
    let mut zi = y * scale;
    let mut i = 0u32;
    let mut m2 = 0.0f32;
    while i < max_iter {
        let zr2 = zr * zr - zi * zi + cx;
        zi = 2.0 * zr * zi + cy;
        zr = zr2;
        m2 = zr * zr + zi * zi;
        if m2 > 256.0 {
            break;
        }
        i += 1;
    }
    if i >= max_iter {
        return 0.0;
    }

    let nu = i as f32 + 1.0 - (m2.max(1.0001).ln().ln() / std::f32::consts::LN_2);
    let n = (nu / max_iter as f32).clamp(0.0, 1.0);
    let stripe = ((nu * (0.12 + 0.05 * treb)) - t * (0.9 + 1.2 * beat)).sin() * 0.5 + 0.5;
    ((1.0 - n).powf(0.30) * 0.68 + stripe * 0.32).clamp(0.0, 1.0)
}

fn fractal_burning_ship(
    x: f32,
    y: f32,
    t: f32,
    cx: f32,
    cy: f32,
    zoom: f32,
    quality: Quality,
) -> f32 {
    let (max_iter, bail) = match quality {
        Quality::Ultra => (120u32, 8.0),
        Quality::High => (96u32, 8.0),
        Quality::Balanced => (72u32, 8.0),
        Quality::Fast => (48u32, 8.0),
    };

    let scale = 1.15 / zoom;
    let cr = cx + x * scale + 0.02 * (t * 0.13).sin();
    let ci = cy + y * scale + 0.02 * (t * 0.11).cos();

    let mut zr = 0.0f32;
    let mut zi = 0.0f32;
    let mut i = 0u32;
    while i < max_iter {
        zr = zr.abs();
        zi = zi.abs();
        let zr2 = zr * zr - zi * zi + cr;
        zi = 2.0 * zr * zi + ci;
        zr = zr2;
        if zr * zr + zi * zi > bail {
            break;
        }
        i += 1;
    }

    (i as f32 / max_iter as f32).powf(0.55)
}

fn fractal_burning_ship_deep(
    x: f32,
    y: f32,
    t: f32,
    center: (f32, f32),
    speed: f32,
    zoom_mul: f32,
    bass: f32,
    mid: f32,
    treb: f32,
    beat: f32,
    quality: Quality,
) -> f32 {
    let max_iter = match quality {
        Quality::Ultra => 240u32,
        Quality::High => 192u32,
        Quality::Balanced => 144u32,
        Quality::Fast => 96u32,
    };

    let speed = if zoom_mul <= 0.0 {
        0.0
    } else {
        speed * zoom_mul.clamp(0.35, 2.5)
    };
    let phase = (t * speed * (0.22 + 0.20 * bass) + beat * 0.04).fract();
    let ease = phase * phase * (3.0 - 2.0 * phase);
    let power = 1.1 + 15.4 * ease;
    let zoom = 2.0f32.powf(power);
    let scale = 2.0 / zoom.max(1.0);

    let cx = center.0 + (scale * 46.0) * (t * (0.11 + 0.08 * mid)).sin();
    let cy = center.1 + (scale * 36.0) * (t * (0.09 + 0.07 * treb)).cos();
    let cr = cx + x * scale;
    let ci = cy + y * scale;

    let mut zr = 0.0f32;
    let mut zi = 0.0f32;
    let mut i = 0u32;
    let mut m2 = 0.0f32;
    while i < max_iter {
        zr = zr.abs();
        zi = zi.abs();
        let zr2 = zr * zr - zi * zi + cr;
        zi = 2.0 * zr * zi + ci;
        zr = zr2;
        m2 = zr * zr + zi * zi;
        if m2 > 512.0 {
            break;
        }
        i += 1;
    }
    if i >= max_iter {
        return 0.0;
    }

    let nu = i as f32 + 1.0 - (m2.max(1.0001).ln().ln() / std::f32::consts::LN_2);
    let n = (nu / max_iter as f32).clamp(0.0, 1.0);
    let grain = ((x * 120.0 + y * 90.0 + t * (1.2 + 1.8 * beat)).sin() * 0.5 + 0.5) * 0.18;
    ((1.0 - n).powf(0.36) * 0.82 + grain).clamp(0.0, 1.0)
}

fn fractal_orbit_trap(
    x: f32,
    y: f32,
    t: f32,
    cx: f32,
    cy: f32,
    zoom: f32,
    quality: Quality,
    trap_x: f32,
    trap_y: f32,
) -> f32 {
    let (max_iter, bail) = match quality {
        Quality::Ultra => (128u32, 4.0),
        Quality::High => (104u32, 4.0),
        Quality::Balanced => (78u32, 4.0),
        Quality::Fast => (56u32, 4.0),
    };

    let scale = 1.25 / zoom;
    let cr = cx + x * scale + 0.02 * (t * 0.15).sin();
    let ci = cy + y * scale + 0.02 * (t * 0.12).cos();

    let mut zr = 0.0f32;
    let mut zi = 0.0f32;
    let mut dmin = 1e9f32;
    let mut i = 0u32;
    while i < max_iter {
        let zr2 = zr * zr - zi * zi + cr;
        zi = 2.0 * zr * zi + ci;
        zr = zr2;

        let dx = zr - trap_x;
        let dy = zi - trap_y;
        let d = (dx * dx + dy * dy).sqrt();
        if d < dmin {
            dmin = d;
        }

        if zr * zr + zi * zi > bail {
            break;
        }
        i += 1;
    }

    let v = (-8.5 * dmin).exp();
    (v + (i as f32 / max_iter as f32) * 0.15).clamp(0.0, 1.0)
}

fn plasma(x: f32, y: f32, t: f32, freq: f32, bass: f32, treb: f32) -> f32 {
    let a = (x * freq + t * (0.9 + bass * 1.3)).sin();
    let b = (y * (freq * 1.13) - t * (1.1 + treb * 1.6)).sin();
    let c = ((x + y) * (freq * 0.77) + t * 0.7).sin();
    ((a + b + c) / 3.0 * 0.5 + 0.5).clamp(0.0, 1.0)
}

fn stripes(x: f32, y: f32, t: f32, freq: f32, beat: f32) -> f32 {
    let v = (x * freq + t * (1.2 + beat * 3.0)).sin() * 0.6
        + (y * (freq * 0.7) - t * 0.9).cos() * 0.4;
    (v * 0.5 + 0.5).fract()
}

fn rings(x: f32, y: f32, t: f32, freq: f32, bass: f32, beat: f32) -> f32 {
    let r = (x * x + y * y).sqrt();
    let w = freq * (1.0 + bass * 0.7);
    let v = (r * w - t * (1.8 + beat * 4.0)).sin();
    (v * 0.5 + 0.5).clamp(0.0, 1.0)
}

fn vortex(x: f32, y: f32, t: f32, spin: f32, energy: f32, bass: f32, treb: f32) -> f32 {
    let ang = y.atan2(x);
    let r = (x * x + y * y).sqrt();
    let v = (ang * (2.0 + spin) + r * (6.0 + energy * 6.0) - t * (2.0 + bass * 3.0)).sin()
        + (r * 10.0 + t * (1.8 + treb * 6.0)).cos() * 0.35;
    (v * 0.35 + 0.5).fract()
}

fn flow(x: f32, y: f32, t: f32, freq: f32, mid: f32, seed: u32) -> f32 {
    let n = hash_noise(x * 2.0, y * 2.0, seed);
    let a = (n * 2.0 * PI + t * (0.55 + mid * 2.2)).sin();
    let b = ((x + a) * freq + t * 0.6).sin() + ((y - a) * freq - t * 0.8).cos();
    (b * 0.25 + 0.5).fract()
}

fn voronoiish(x: f32, y: f32, t: f32, points: u32, seed: u32) -> f32 {
    let mut best = 1e9f32;
    let p = points.max(2).min(12);
    for i in 0..p {
        let fi = i as f32;
        let px = (hash_noise(fi, 1.0, seed) * 2.0 - 1.0) * 0.9 + (t * (0.2 + fi * 0.01)).sin() * 0.15;
        let py = (hash_noise(fi, 2.0, seed) * 2.0 - 1.0) * 0.9 + (t * (0.17 + fi * 0.013)).cos() * 0.15;
        let dx = x - px;
        let dy = y - py;
        let d = dx * dx + dy * dy;
        if d < best {
            best = d;
        }
    }
    (best.sqrt() * 2.2).fract()
}

fn metaballs(x: f32, y: f32, t: f32, blobs: u32, seed: u32) -> f32 {
    let b = blobs.max(2).min(10);
    let mut acc = 0.0f32;
    for i in 0..b {
        let fi = i as f32;
        let px = (hash_noise(fi, 10.0, seed) * 2.0 - 1.0) * 0.85 + (t * (0.3 + fi * 0.03)).sin() * 0.2;
        let py = (hash_noise(fi, 11.0, seed) * 2.0 - 1.0) * 0.85 + (t * (0.27 + fi * 0.02)).cos() * 0.2;
        let dx = x - px;
        let dy = y - py;
        let d2 = (dx * dx + dy * dy).max(1e-3);
        acc += 0.08 / d2;
    }
    (acc.tanh() * 0.9).fract()
}

fn sparks(x: f32, y: f32, t: f32, density: f32, treb: f32, beat: f32, seed: u32) -> f32 {
    let n = hash_noise(x * 40.0, y * 40.0 + t * 2.0, seed);
    let gate = (0.92 - treb * 0.25 - beat * 0.35).clamp(0.35, 0.95);
    if n > gate {
        (n * 3.0 + t * 0.7).fract() * density
    } else {
        (plasma(x, y, t, 2.0, 0.0, treb) * 0.4).fract()
    }
}

fn starfield(x: f32, y: f32, t: f32, bass: f32, depth: f32, seed: u32) -> f32 {
    // Stateless starfield: hash-based points that drift with time.
    let sx = x * 120.0 + t * (6.0 + bass * 30.0) * depth;
    let sy = y * 70.0 - t * (4.0 + bass * 20.0) * depth;
    let n = hash_noise(sx, sy, seed);
    if n > 0.985 {
        0.98
    } else if n > 0.97 {
        0.85
    } else {
        0.02 + n * 0.08
    }
}

fn smoke(prev: &[u8], w: usize, h: usize, x: usize, y: usize, blur: f32) -> f32 {
    if prev.len() < w * h * 4 {
        return 0.0;
    }
    let clamp_x = |xx: isize| -> usize { xx.clamp(0, (w as isize) - 1) as usize };
    let clamp_y = |yy: isize| -> usize { yy.clamp(0, (h as isize) - 1) as usize };
    let ix = x as isize;
    let iy = y as isize;

    let mut acc = 0.0f32;
    let mut wsum = 0.0f32;
    for (dx, dy, ww) in [
        (0, 0, 0.40),
        (-1, 0, 0.15),
        (1, 0, 0.15),
        (0, -1, 0.15),
        (0, 1, 0.15),
    ] {
        let xx = clamp_x(ix + dx);
        let yy = clamp_y(iy + dy);
        let i = (yy * w + xx) * 4;
        let lum = (prev[i] as f32 * 0.2126 + prev[i + 1] as f32 * 0.7152 + prev[i + 2] as f32 * 0.0722)
            / 255.0;
        acc += lum * ww;
        wsum += ww;
    }
    let v = (acc / wsum).powf(0.8);
    (v * blur + (1.0 - blur) * (hash_noise(ix as f32, iy as f32, 0) * 0.2)).fract()
}

fn cells(x: f32, y: f32, t: f32, scale: f32, beat: f32, seed: u32) -> f32 {
    let gx = ((x + 1.0) * 0.5 * scale).floor();
    let gy = ((y + 1.0) * 0.5 * scale).floor();
    let n = hash_noise(gx, gy + (t * 0.5).floor(), seed);
    let v = if beat > 0.05 { n } else { n * 0.6 };
    (v * 1.7).fract()
}

fn glitch(x: usize, y: usize, _w: usize, h: usize, t: f32, block: f32, onset: f32, seed: u32) -> f32 {
    let bx = ((x as f32) / block).floor();
    let by = ((y as f32) / block).floor();
    let n = hash_noise(bx, by + (t * (0.8 + onset * 2.0)).floor(), seed);
    let scan = ((y as f32 / h.max(1) as f32) * 30.0 + t * 6.0).sin() * 0.2 + 0.8;
    ((n * scan) * 2.2).fract()
}

fn noise(x: f32, y: f32, t: f32, freq: f32, seed: u32) -> f32 {
    let n = hash_noise(x * 6.0 * freq + t * 0.5, y * 6.0 * freq - t * 0.4, seed);
    let m = hash_noise(x * 12.0 * freq - t * 1.2, y * 12.0 * freq + t * 0.9, seed ^ 0xA53A_9B17);
    ((n * 0.65 + m * 0.35) * 1.3).fract()
}

fn clifford_field(x: f32, y: f32, t: f32, bass: f32, mid: f32, treb: f32, quality: Quality) -> f32 {
    let iters = match quality {
        Quality::Ultra => 22u32,
        Quality::High => 18u32,
        Quality::Balanced => 14u32,
        Quality::Fast => 10u32,
    };

    let a = 1.6 + 0.8 * bass + 0.1 * (t * 0.17).sin();
    let b = 1.7 + 0.9 * mid + 0.1 * (t * 0.13).cos();
    let c = 0.6 + 0.5 * treb;
    let d = 1.2 + 0.4 * bass;

    let mut zx = x * 0.6;
    let mut zy = y * 0.6;
    let mut acc = 0.0f32;
    for _ in 0..iters {
        let nx = (a * zy).sin() + c * (a * zx).cos();
        let ny = (b * zx).sin() + d * (b * zy).cos();
        zx = nx;
        zy = ny;
        let r2 = zx * zx + zy * zy;
        acc += (-2.2 * r2).exp();
    }
    (acc * 0.18).clamp(0.0, 1.0)
}

fn dejong_field(x: f32, y: f32, t: f32, bass: f32, mid: f32, treb: f32, quality: Quality) -> f32 {
    let iters = match quality {
        Quality::Ultra => 22u32,
        Quality::High => 18u32,
        Quality::Balanced => 14u32,
        Quality::Fast => 10u32,
    };

    let a = 1.4 + 1.0 * bass + 0.1 * (t * 0.11).cos();
    let b = 1.8 + 1.0 * treb + 0.1 * (t * 0.09).sin();
    let c = 1.6 + 0.7 * mid;
    let d = 1.9 + 0.6 * bass;

    let mut zx = x * 0.7;
    let mut zy = y * 0.7;
    let mut acc = 0.0f32;
    for _ in 0..iters {
        let nx = (a * zy).sin() - (b * zx).cos();
        let ny = (c * zx).sin() - (d * zy).cos();
        zx = nx;
        zy = ny;
        let r2 = zx * zx + zy * zy;
        acc += (-2.4 * r2).exp();
    }
    (acc * 0.20).clamp(0.0, 1.0)
}

fn warp_candy(x: f32, y: f32, t: f32, freq: f32, bass: f32, mid: f32, treb: f32, seed: u32) -> f32 {
    let n = hash_noise(x * 1.7 + t * 0.25, y * 1.7 - t * 0.21, seed);
    let m = hash_noise(x * 3.1 - t * 0.55, y * 3.1 + t * 0.43, seed ^ 0xA53A_9B17);
    let wx = x + (n * 2.0 - 1.0) * (0.35 + 0.30 * bass) + 0.12 * (t * (0.7 + mid)).sin();
    let wy = y + (m * 2.0 - 1.0) * (0.35 + 0.30 * treb) + 0.12 * (t * (0.6 + treb)).cos();
    plasma(wx, wy, t, freq, bass, treb)
}

fn polar_moire(x: f32, y: f32, t: f32, _freq: f32, bass: f32, mid: f32, treb: f32, beat: f32) -> f32 {
    let r = (x * x + y * y).sqrt();
    let a = y.atan2(x);
    let v = (r * (20.0 + 30.0 * bass) + t * (1.0 + beat * 2.0)).sin()
        + (a * (10.0 + 18.0 * treb) - t * 1.4).cos()
        + ((r + a) * (12.0 + 6.0 * mid) + t * 0.8).sin();
    (v * (1.0 / 3.0) * 0.5 + 0.5).clamp(0.0, 1.0)
}

fn truchet(x: f32, y: f32, t: f32, tiles: f32, bass: f32, treb: f32, seed: u32) -> f32 {
    let tiles = (tiles * (1.0 + bass * 1.2)).clamp(4.0, 32.0);
    let u = (x + 1.0) * 0.5 * tiles;
    let v = (y + 1.0) * 0.5 * tiles;
    let ix = u.floor();
    let iy = v.floor();
    let fx = (u - ix) - 0.5;
    let fy = (v - iy) - 0.5;

    let h = hash_noise(ix, iy + (t * (0.3 + 0.9 * treb)).floor(), seed);
    let flip = h > 0.5;
    let (c1x, c1y, c2x, c2y) = if flip {
        (-0.5, -0.5, 0.5, 0.5)
    } else {
        (-0.5, 0.5, 0.5, -0.5)
    };

    let d1 = ((fx - c1x).powi(2) + (fy - c1y).powi(2)).sqrt();
    let d2 = ((fx - c2x).powi(2) + (fy - c2y).powi(2)).sqrt();
    let d = (d1.min(d2) - 0.5).abs();
    let v = (-20.0 * d).exp();
    (v * (0.8 + 0.6 * treb + 0.4 * bass)).clamp(0.0, 1.0)
}

fn orbs(x: f32, y: f32, t: f32, freq: f32, bass: f32, beat: f32) -> f32 {
    let a = 0.2 * (t * 0.3).sin() + bass;
    let ca = a.cos();
    let sa = a.sin();
    let rx = x * ca - y * sa;
    let ry = x * sa + y * ca;

    let s = (freq * (1.0 + bass)).clamp(1.2, 5.5);
    let fx = (rx * s).fract() - 0.5;
    let fy = (ry * s).fract() - 0.5;
    let r = (fx * fx + fy * fy).sqrt();
    let rad = 0.18 + 0.12 * bass + 0.08 * beat;
    let d = (r - rad).abs();
    (-14.0 * d).exp().clamp(0.0, 1.0)
}

fn chladni(x: f32, y: f32, t: f32, a0: f32, b0: f32, bass: f32, mid: f32, treb: f32) -> f32 {
    let ax = a0 + 10.0 * bass;
    let ay = b0 + 10.0 * mid;
    let v = (ax * PI * x).sin() * (ay * PI * y).sin()
        + 0.35 * ((ax + ay) * 0.5 * PI * (x + y) + t * (0.8 + treb)).sin();
    v.abs().powf(0.35).clamp(0.0, 1.0)
}

fn crt_scan(x: f32, y: f32, t: f32, freq: f32, bass: f32, mid: f32, treb: f32, beat: f32, seed: u32) -> f32 {
    let base = warp_candy(x, y, t, 3.2, bass, mid, treb, seed);
    let scan = 0.7 + 0.3 * ((y + 1.0) * 0.5 * freq + t * (25.0 + 40.0 * beat)).sin();
    (base * scan).clamp(0.0, 1.0)
}

fn moire(x: f32, y: f32, t: f32, freq: f32, bass: f32, treb: f32, beat: f32) -> f32 {
    let a = 0.25 * (t * 0.2).sin() + 0.4 * bass;
    let ca = a.cos();
    let sa = a.sin();
    let rx = x * ca - y * sa;
    let ry = x * sa + y * ca;
    let v = (rx * freq + t * 0.6).sin()
        + (ry * (freq * 0.94) - t * 0.7).sin()
        + ((rx + ry) * (freq * 0.5) + t * (0.8 + beat * 1.2) + treb * 2.0).sin();
    (v * (1.0 / 3.0) * 0.5 + 0.5).clamp(0.0, 1.0)
}

fn palette(p: Palette, v: f32, t: f32, bass: f32, mid: f32, treb: f32, beat: f32) -> [u8; 3] {
    let v = v.clamp(0.0, 1.0);
    let pop = (0.4 * bass + 0.3 * mid + 0.6 * treb + 0.8 * beat).clamp(0.0, 1.0);

    match p {
        Palette::Prism => {
            let h = fract01(v * 0.92 + t * 0.04 + pop * 0.12);
            hsv_to_rgb(h, 0.92, (0.55 + v * 0.45 + pop * 0.2).min(1.0))
        }
        Palette::Acid => {
            let h = fract01(0.65 + v * 0.55 + t * 0.07 + beat * 0.18);
            hsv_to_rgb(h, 0.98, (0.5 + v * 0.55 + pop * 0.25).min(1.0))
        }
        Palette::Neon => {
            // Pink/cyan/blue cycling
            let h = fract01(0.86 + (v * 0.35) + t * 0.05 - treb * 0.12);
            hsv_to_rgb(h, 0.95, (0.55 + v * 0.5 + pop * 0.2).min(1.0))
        }
        Palette::Fire => {
            let h = fract01(0.02 + v * 0.12 + t * 0.02 + beat * 0.05);
            hsv_to_rgb(h, 0.96, (0.35 + v * 0.75 + pop * 0.35).min(1.0))
        }
        Palette::Aurora => {
            let h = fract01(0.35 + v * 0.22 + t * 0.03 + mid * 0.1);
            hsv_to_rgb(h, 0.9, (0.45 + v * 0.6 + pop * 0.15).min(1.0))
        }
        Palette::Cosmic => {
            // Cosine palette (IQ-style) for smoother, more "shader-y" cycling.
            let tt = fract01(v + t * 0.04 + pop * 0.08 - treb * 0.05);
            let a = [0.32, 0.22, 0.28];
            let b = [0.75, 0.65, 0.80];
            let c = [1.0, 1.0, 1.0];
            let d = [0.00 + 0.15 * bass, 0.33 + 0.10 * mid, 0.67 - 0.12 * treb];
            iq_palette(tt, a, b, c, d)
        }
    }
}

fn iq_palette(t: f32, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]) -> [u8; 3] {
    let tau = 2.0 * PI;
    let r = a[0] + b[0] * (tau * (c[0] * t + d[0])).cos();
    let g = a[1] + b[1] * (tau * (c[1] * t + d[1])).cos();
    let bb = a[2] + b[2] * (tau * (c[2] * t + d[2])).cos();
    [
        (r.clamp(0.0, 1.0) * 255.0) as u8,
        (g.clamp(0.0, 1.0) * 255.0) as u8,
        (bb.clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = fract01(h) * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
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
    [
        (r.clamp(0.0, 1.0) * 255.0) as u8,
        (g.clamp(0.0, 1.0) * 255.0) as u8,
        (b.clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

fn fract01(x: f32) -> f32 {
    let f = x - x.floor();
    if f < 0.0 { f + 1.0 } else { f }
}

fn kaleido(x: f32, y: f32, symmetry: u32) -> (f32, f32) {
    let s = symmetry.max(2) as f32;
    let ang = y.atan2(x);
    let r = (x * x + y * y).sqrt();
    let seg = 2.0 * PI / s;
    let mut a = ang.rem_euclid(2.0 * PI);
    a = a % seg;
    // Mirror every other segment.
    if a > seg * 0.5 {
        a = seg - a;
    }
    (a.cos() * r, a.sin() * r)
}

fn hash_noise(x: f32, y: f32, seed: u32) -> f32 {
    let xi = (x * 163.0).floor() as i32;
    let yi = (y * 163.0).floor() as i32;
    let mut n = (xi as u32).wrapping_mul(374_761_393)
        ^ (yi as u32).wrapping_mul(668_265_263)
        ^ seed.wrapping_mul(0x9E37_79B9);
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    n = n ^ (n >> 16);
    ((n & 0x00FF_FFFF) as f32) / 16_777_215.0
}

fn sample_rgb(prev: &[u8], w: usize, h: usize, nx: f32, ny: f32) -> [u8; 3] {
    if prev.len() < w * h * 4 {
        return [0, 0, 0];
    }
    let x = ((nx * 0.5 + 0.5) * (w as f32 - 1.0)).round() as isize;
    let y = ((ny * 0.5 + 0.5) * (h as f32 - 1.0)).round() as isize;
    let xx = x.clamp(0, (w as isize) - 1) as usize;
    let yy = y.clamp(0, (h as isize) - 1) as usize;
    let i = (yy * w + xx) * 4;
    [prev[i], prev[i + 1], prev[i + 2]]
}
