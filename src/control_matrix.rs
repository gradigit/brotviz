use crate::audio::AudioFeatures;
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::path::Path;

pub const FEATURE_KEY_COUNT: usize = 17;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FeatureKey {
    Rms,
    Onset,
    BeatGate,
    BeatStrength,
    Centroid,
    Flatness,
    Band0,
    Band1,
    Band2,
    Band3,
    Band4,
    Band5,
    Band6,
    Band7,
    Bass,
    Mid,
    Treble,
}

impl FeatureKey {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rms" => Some(Self::Rms),
            "onset" => Some(Self::Onset),
            "beat" | "beat_gate" => Some(Self::BeatGate),
            "beat_strength" => Some(Self::BeatStrength),
            "centroid" => Some(Self::Centroid),
            "flatness" => Some(Self::Flatness),
            "band0" => Some(Self::Band0),
            "band1" => Some(Self::Band1),
            "band2" => Some(Self::Band2),
            "band3" => Some(Self::Band3),
            "band4" => Some(Self::Band4),
            "band5" => Some(Self::Band5),
            "band6" => Some(Self::Band6),
            "band7" => Some(Self::Band7),
            "bass" => Some(Self::Bass),
            "mid" => Some(Self::Mid),
            "treble" => Some(Self::Treble),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rms => "rms",
            Self::Onset => "onset",
            Self::BeatGate => "beat",
            Self::BeatStrength => "beat_strength",
            Self::Centroid => "centroid",
            Self::Flatness => "flatness",
            Self::Band0 => "band0",
            Self::Band1 => "band1",
            Self::Band2 => "band2",
            Self::Band3 => "band3",
            Self::Band4 => "band4",
            Self::Band5 => "band5",
            Self::Band6 => "band6",
            Self::Band7 => "band7",
            Self::Bass => "bass",
            Self::Mid => "mid",
            Self::Treble => "treble",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Rms => 0,
            Self::Onset => 1,
            Self::BeatGate => 2,
            Self::BeatStrength => 3,
            Self::Centroid => 4,
            Self::Flatness => 5,
            Self::Band0 => 6,
            Self::Band1 => 7,
            Self::Band2 => 8,
            Self::Band3 => 9,
            Self::Band4 => 10,
            Self::Band5 => 11,
            Self::Band6 => 12,
            Self::Band7 => 13,
            Self::Bass => 14,
            Self::Mid => 15,
            Self::Treble => 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpandedFeatureVector {
    values: [f32; FEATURE_KEY_COUNT],
}

impl ExpandedFeatureVector {
    pub fn from_audio(audio: &AudioFeatures) -> Self {
        let mut values = [0.0f32; FEATURE_KEY_COUNT];
        values[FeatureKey::Rms.index()] = clamp01(audio.rms);
        values[FeatureKey::Onset.index()] = clamp01(audio.onset);
        values[FeatureKey::BeatGate.index()] = if audio.beat { 1.0 } else { 0.0 };
        values[FeatureKey::BeatStrength.index()] = clamp01(audio.beat_strength);
        values[FeatureKey::Centroid.index()] = clamp01(audio.centroid);
        values[FeatureKey::Flatness.index()] = clamp01(audio.flatness);
        values[FeatureKey::Band0.index()] = clamp01(audio.bands[0]);
        values[FeatureKey::Band1.index()] = clamp01(audio.bands[1]);
        values[FeatureKey::Band2.index()] = clamp01(audio.bands[2]);
        values[FeatureKey::Band3.index()] = clamp01(audio.bands[3]);
        values[FeatureKey::Band4.index()] = clamp01(audio.bands[4]);
        values[FeatureKey::Band5.index()] = clamp01(audio.bands[5]);
        values[FeatureKey::Band6.index()] = clamp01(audio.bands[6]);
        values[FeatureKey::Band7.index()] = clamp01(audio.bands[7]);

        let bass = (audio.bands[0] + audio.bands[1] + audio.bands[2]) / 3.0;
        let mid = (audio.bands[2] + audio.bands[3] + audio.bands[4]) / 3.0;
        let treble = (audio.bands[5] + audio.bands[6] + audio.bands[7]) / 3.0;
        values[FeatureKey::Bass.index()] = clamp01(bass);
        values[FeatureKey::Mid.index()] = clamp01(mid);
        values[FeatureKey::Treble.index()] = clamp01(treble);

        Self { values }
    }

    pub fn get(&self, key: FeatureKey) -> f32 {
        self.values[key.index()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlCurve {
    Linear,
    EaseIn,
    EaseOut,
    SmoothStep,
}

impl ControlCurve {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "linear" => Some(Self::Linear),
            "ease_in" => Some(Self::EaseIn),
            "ease_out" => Some(Self::EaseOut),
            "smoothstep" => Some(Self::SmoothStep),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::EaseIn => "ease_in",
            Self::EaseOut => "ease_out",
            Self::SmoothStep => "smoothstep",
        }
    }

    pub fn apply(self, x: f32) -> f32 {
        let x = clamp01(x);
        match self {
            Self::Linear => x,
            Self::EaseIn => x * x,
            Self::EaseOut => 1.0 - (1.0 - x) * (1.0 - x),
            Self::SmoothStep => x * x * (3.0 - 2.0 * x),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlRoute {
    pub control: String,
    pub source: FeatureKey,
    pub curve: ControlCurve,
    pub smoothing: f32,
    pub gain: f32,
    pub bias: f32,
    pub min: f32,
    pub max: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlMatrix {
    routes: Vec<ControlRoute>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControlMatrixError {
    Io(String),
    Parse { line: usize, message: String },
    EmptyRoutes,
    DuplicateControl(String),
    InvalidSmoothing { control: String, value: f32 },
    InvalidBounds { control: String, min: f32, max: f32 },
}

impl fmt::Display for ControlMatrixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "I/O error: {msg}"),
            Self::Parse { line, message } => write!(f, "parse error at line {line}: {message}"),
            Self::EmptyRoutes => write!(f, "control matrix must contain at least one route"),
            Self::DuplicateControl(name) => write!(f, "duplicate control route: {name}"),
            Self::InvalidSmoothing { control, value } => {
                write!(f, "invalid smoothing for control '{control}': {value}")
            }
            Self::InvalidBounds { control, min, max } => {
                write!(f, "invalid bounds for control '{control}': min={min} max={max}")
            }
        }
    }
}

impl std::error::Error for ControlMatrixError {}

impl ControlMatrix {
    pub fn parse(text: &str) -> Result<Self, ControlMatrixError> {
        let mut routes = Vec::new();

        for (line_idx, raw) in text.lines().enumerate() {
            let line_no = line_idx + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let tokens: Vec<&str> = trimmed.split_whitespace().collect();
            if tokens.first().copied() != Some("route") {
                return Err(ControlMatrixError::Parse {
                    line: line_no,
                    message: "expected 'route'".to_string(),
                });
            }
            if tokens.len() != 9 {
                return Err(ControlMatrixError::Parse {
                    line: line_no,
                    message:
                        "route expects: route <control> <feature> <curve> <smoothing> <gain> <bias> <min> <max>"
                            .to_string(),
                });
            }

            let control = tokens[1].to_string();
            let source =
                FeatureKey::parse(tokens[2]).ok_or_else(|| ControlMatrixError::Parse {
                    line: line_no,
                    message: format!("unknown feature '{}'", tokens[2]),
                })?;
            let curve = ControlCurve::parse(tokens[3]).ok_or_else(|| ControlMatrixError::Parse {
                line: line_no,
                message: format!("unknown curve '{}'", tokens[3]),
            })?;
            let smoothing = parse_f32(tokens[4], line_no, "invalid smoothing")?;
            let gain = parse_f32(tokens[5], line_no, "invalid gain")?;
            let bias = parse_f32(tokens[6], line_no, "invalid bias")?;
            let min = parse_f32(tokens[7], line_no, "invalid min")?;
            let max = parse_f32(tokens[8], line_no, "invalid max")?;

            routes.push(ControlRoute {
                control,
                source,
                curve,
                smoothing,
                gain,
                bias,
                min,
                max,
            });
        }

        let matrix = Self { routes };
        matrix.validate()?;
        Ok(matrix)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, ControlMatrixError> {
        let text = std::fs::read_to_string(path.as_ref())
            .map_err(|e| ControlMatrixError::Io(e.to_string()))?;
        Self::parse(&text)
    }

    pub fn to_text(&self) -> String {
        self.routes
            .iter()
            .map(|route| {
                format!(
                    "route {} {} {} {:.6} {:.6} {:.6} {:.6} {:.6}",
                    route.control,
                    route.source.as_str(),
                    route.curve.as_str(),
                    route.smoothing,
                    route.gain,
                    route.bias,
                    route.min,
                    route.max
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn routes(&self) -> &[ControlRoute] {
        &self.routes
    }

    pub fn validate(&self) -> Result<(), ControlMatrixError> {
        if self.routes.is_empty() {
            return Err(ControlMatrixError::EmptyRoutes);
        }

        let mut seen = HashSet::new();
        for route in &self.routes {
            if !seen.insert(route.control.clone()) {
                return Err(ControlMatrixError::DuplicateControl(route.control.clone()));
            }
            if !route.smoothing.is_finite() || !(0.0..=1.0).contains(&route.smoothing) {
                return Err(ControlMatrixError::InvalidSmoothing {
                    control: route.control.clone(),
                    value: route.smoothing,
                });
            }
            if !route.min.is_finite() || !route.max.is_finite() || route.min > route.max {
                return Err(ControlMatrixError::InvalidBounds {
                    control: route.control.clone(),
                    min: route.min,
                    max: route.max,
                });
            }
        }
        Ok(())
    }

    pub fn evaluate(
        &self,
        audio: &AudioFeatures,
        state: &mut ControlState,
    ) -> BTreeMap<String, f32> {
        let features = ExpandedFeatureVector::from_audio(audio);
        let mut out = BTreeMap::new();

        for route in &self.routes {
            let source = features.get(route.source);
            let shaped = route.curve.apply(source);
            let target = (route.bias + route.gain * shaped).clamp(route.min, route.max);
            let prev = state.values.get(&route.control).copied().unwrap_or(target);
            let next = (prev + (target - prev) * route.smoothing).clamp(route.min, route.max);
            state.values.insert(route.control.clone(), next);
            out.insert(route.control.clone(), next);
        }

        out
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ControlState {
    values: BTreeMap<String, f32>,
}

impl ControlState {
    pub fn get(&self, control: &str) -> Option<f32> {
        self.values.get(control).copied()
    }

    pub fn values(&self) -> &BTreeMap<String, f32> {
        &self.values
    }
}

fn parse_f32(s: &str, line: usize, msg: &str) -> Result<f32, ControlMatrixError> {
    let v = s.parse::<f32>().map_err(|_| ControlMatrixError::Parse {
        line,
        message: msg.to_string(),
    })?;
    if !v.is_finite() {
        return Err(ControlMatrixError::Parse {
            line,
            message: msg.to_string(),
        });
    }
    Ok(v)
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}
