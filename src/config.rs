use clap::{Parser, ValueEnum};

#[derive(Parser, Debug, Clone)]
#[command(name = "tui-visualizer", version, about = "Psychedelic, audio-reactive terminal visualizer (Ghostty/macOS v1)")]
pub struct Config {
    #[arg(long, value_enum, default_value_t = AudioSource::Mic)]
    pub source: AudioSource,

    #[arg(long, value_enum, default_value_t = EngineMode::Metal)]
    pub engine: EngineMode,

    #[arg(long, value_enum, default_value_t = RendererMode::HalfBlock)]
    pub renderer: RendererMode,

    #[arg(long, default_value_t = 60)]
    pub fps: u32,

    #[arg(long, value_enum, default_value_t = Quality::Balanced)]
    pub quality: Quality,

    #[arg(long, default_value_t = true)]
    pub adaptive_quality: bool,

    #[arg(long, value_enum, default_value_t = SwitchMode::Manual)]
    pub switch: SwitchMode,

    #[arg(long, default_value_t = false)]
    pub shuffle: bool,

    #[arg(long, default_value_t = 16)]
    pub beats_per_switch: u32,

    #[arg(long, default_value_t = 20.0)]
    pub seconds_per_switch: f32,

    #[arg(long)]
    pub preset: Option<String>,

    #[arg(long, default_value_t = false)]
    pub list_devices: bool,

    #[arg(long)]
    pub device: Option<String>,

    #[arg(long, default_value_t = false)]
    pub safe: bool,

    #[arg(long, default_value_t = false)]
    pub stage_mode: bool,

    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub auto_probe: bool,

    #[arg(long, default_value_t = false)]
    pub latency_calibration: bool,

    #[arg(long, default_value_t = 0.0)]
    pub latency_offset_ms: f32,

    #[arg(long)]
    pub theme_pack: Option<String>,

    #[arg(long)]
    pub control_matrix: Option<String>,

    #[arg(long)]
    pub preset_graph: Option<String>,

    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub sync_updates: bool,

    #[arg(long)]
    pub lyrics_file: Option<String>,

    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub lyrics_loop: bool,

    #[arg(long, default_value_t = 0.0)]
    pub lyrics_offset_ms: f32,

    #[arg(long, value_enum, default_value_t = SystemDataMode::Off)]
    pub system_data: SystemDataMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AudioSource {
    Mic,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RendererMode {
    #[value(alias = "ansi", alias = "text")]
    Ascii,
    #[value(name = "half-block", alias = "halfblock", alias = "half_block", alias = "hb")]
    HalfBlock,
    #[value(alias = "hires", alias = "dots")]
    Braille,
    #[value(alias = "sext", alias = "mosaic", alias = "symbols")]
    Sextant,
    Kitty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EngineMode {
    Cpu,
    #[value(alias = "gpu")]
    Metal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SwitchMode {
    Manual,
    Beat,
    Energy,
    Time,
    Adaptive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Quality {
    Ultra,
    High,
    Balanced,
    Fast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SystemDataMode {
    Off,
    Subtle,
    Creep,
}

impl Quality {
    pub fn lower(self) -> Self {
        match self {
            Self::Ultra => Self::High,
            Self::High => Self::Balanced,
            Self::Balanced => Self::Fast,
            Self::Fast => Self::Fast,
        }
    }

    pub fn higher(self) -> Self {
        match self {
            Self::Fast => Self::Balanced,
            Self::Balanced => Self::High,
            Self::High => Self::Ultra,
            Self::Ultra => Self::Ultra,
        }
    }
}
