use std::collections::HashSet;
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct ThemePackManifest {
    pub name: String,
    pub tags: Vec<String>,
    pub preset_indices: Vec<usize>,
    pub transition: TransitionPrefs,
    pub intensity_default: f32,
    pub zoom_default: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionPrefs {
    pub min_beats: u32,
    pub max_beats: u32,
    pub crossfade_ms: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ThemePackError {
    Io(String),
    Parse { line: usize, message: String },
    MissingField(&'static str),
    DuplicatePresetIndex(usize),
    InvalidValue { field: &'static str, message: String },
}

impl fmt::Display for ThemePackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "I/O error: {msg}"),
            Self::Parse { line, message } => write!(f, "parse error at line {line}: {message}"),
            Self::MissingField(field) => write!(f, "missing required field: {field}"),
            Self::DuplicatePresetIndex(idx) => write!(f, "duplicate preset index: {idx}"),
            Self::InvalidValue { field, message } => {
                write!(f, "invalid value for {field}: {message}")
            }
        }
    }
}

impl std::error::Error for ThemePackError {}

impl ThemePackManifest {
    pub fn parse(text: &str) -> Result<Self, ThemePackError> {
        let mut name: Option<String> = None;
        let mut tags: Option<Vec<String>> = None;
        let mut preset_indices: Option<Vec<usize>> = None;
        let mut min_beats: Option<u32> = None;
        let mut max_beats: Option<u32> = None;
        let mut crossfade_ms: Option<u32> = None;
        let mut intensity_default: Option<f32> = None;
        let mut zoom_default: Option<f32> = None;

        for (line_idx, raw) in text.lines().enumerate() {
            let line_no = line_idx + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let (key, value) = trimmed.split_once('=').ok_or(ThemePackError::Parse {
                line: line_no,
                message: "expected <key>=<value>".to_string(),
            })?;
            let key = key.trim();
            let value = value.trim();

            match key {
                "name" => {
                    assign_once(
                        &mut name,
                        value.to_string(),
                        line_no,
                        "duplicate 'name' field",
                    )?;
                }
                "tags" => {
                    let parsed = parse_csv_strings(value);
                    assign_once(&mut tags, parsed, line_no, "duplicate 'tags' field")?;
                }
                "presets" => {
                    let parsed = parse_csv_usize(value, line_no, "presets")?;
                    assign_once(
                        &mut preset_indices,
                        parsed,
                        line_no,
                        "duplicate 'presets' field",
                    )?;
                }
                "transition.min_beats" => {
                    let parsed = parse_u32(value, line_no, "transition.min_beats")?;
                    assign_once(
                        &mut min_beats,
                        parsed,
                        line_no,
                        "duplicate 'transition.min_beats' field",
                    )?;
                }
                "transition.max_beats" => {
                    let parsed = parse_u32(value, line_no, "transition.max_beats")?;
                    assign_once(
                        &mut max_beats,
                        parsed,
                        line_no,
                        "duplicate 'transition.max_beats' field",
                    )?;
                }
                "transition.crossfade_ms" => {
                    let parsed = parse_u32(value, line_no, "transition.crossfade_ms")?;
                    assign_once(
                        &mut crossfade_ms,
                        parsed,
                        line_no,
                        "duplicate 'transition.crossfade_ms' field",
                    )?;
                }
                "defaults.intensity" => {
                    let parsed = parse_f32(value, line_no, "defaults.intensity")?;
                    assign_once(
                        &mut intensity_default,
                        parsed,
                        line_no,
                        "duplicate 'defaults.intensity' field",
                    )?;
                }
                "defaults.zoom" => {
                    let parsed = parse_f32(value, line_no, "defaults.zoom")?;
                    assign_once(
                        &mut zoom_default,
                        parsed,
                        line_no,
                        "duplicate 'defaults.zoom' field",
                    )?;
                }
                _ => {
                    return Err(ThemePackError::Parse {
                        line: line_no,
                        message: format!("unknown key '{key}'"),
                    });
                }
            }
        }

        let manifest = Self {
            name: name.ok_or(ThemePackError::MissingField("name"))?,
            tags: tags.unwrap_or_default(),
            preset_indices: preset_indices.ok_or(ThemePackError::MissingField("presets"))?,
            transition: TransitionPrefs {
                min_beats: min_beats.ok_or(ThemePackError::MissingField("transition.min_beats"))?,
                max_beats: max_beats.ok_or(ThemePackError::MissingField("transition.max_beats"))?,
                crossfade_ms: crossfade_ms
                    .ok_or(ThemePackError::MissingField("transition.crossfade_ms"))?,
            },
            intensity_default: intensity_default
                .ok_or(ThemePackError::MissingField("defaults.intensity"))?,
            zoom_default: zoom_default.ok_or(ThemePackError::MissingField("defaults.zoom"))?,
        };

        manifest.validate()?;
        Ok(manifest)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, ThemePackError> {
        let text =
            std::fs::read_to_string(path.as_ref()).map_err(|e| ThemePackError::Io(e.to_string()))?;
        Self::parse(&text)
    }

    pub fn to_text(&self) -> String {
        let tags = self.tags.join(",");
        let presets = self
            .preset_indices
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(",");
        [
            format!("name={}", self.name),
            format!("tags={tags}"),
            format!("presets={presets}"),
            format!("transition.min_beats={}", self.transition.min_beats),
            format!("transition.max_beats={}", self.transition.max_beats),
            format!("transition.crossfade_ms={}", self.transition.crossfade_ms),
            format!("defaults.intensity={}", self.intensity_default),
            format!("defaults.zoom={}", self.zoom_default),
        ]
        .join("\n")
    }

    pub fn validate(&self) -> Result<(), ThemePackError> {
        if self.name.trim().is_empty() {
            return Err(ThemePackError::InvalidValue {
                field: "name",
                message: "name must not be empty".to_string(),
            });
        }
        if self.preset_indices.is_empty() {
            return Err(ThemePackError::InvalidValue {
                field: "presets",
                message: "at least one preset index is required".to_string(),
            });
        }

        let mut seen = HashSet::new();
        for preset in &self.preset_indices {
            if !seen.insert(*preset) {
                return Err(ThemePackError::DuplicatePresetIndex(*preset));
            }
        }

        if self.transition.min_beats == 0 {
            return Err(ThemePackError::InvalidValue {
                field: "transition.min_beats",
                message: "must be greater than 0".to_string(),
            });
        }
        if self.transition.max_beats < self.transition.min_beats {
            return Err(ThemePackError::InvalidValue {
                field: "transition.max_beats",
                message: "must be >= transition.min_beats".to_string(),
            });
        }
        if self.transition.crossfade_ms == 0 {
            return Err(ThemePackError::InvalidValue {
                field: "transition.crossfade_ms",
                message: "must be greater than 0".to_string(),
            });
        }
        if !self.intensity_default.is_finite() || !(0.0..=2.0).contains(&self.intensity_default) {
            return Err(ThemePackError::InvalidValue {
                field: "defaults.intensity",
                message: "must be finite and in [0,2]".to_string(),
            });
        }
        if !self.zoom_default.is_finite() || self.zoom_default <= 0.0 {
            return Err(ThemePackError::InvalidValue {
                field: "defaults.zoom",
                message: "must be finite and > 0".to_string(),
            });
        }
        Ok(())
    }
}

fn assign_once<T>(
    slot: &mut Option<T>,
    value: T,
    line: usize,
    duplicate_message: &str,
) -> Result<(), ThemePackError> {
    if slot.is_some() {
        return Err(ThemePackError::Parse {
            line,
            message: duplicate_message.to_string(),
        });
    }
    *slot = Some(value);
    Ok(())
}

fn parse_u32(s: &str, line: usize, field: &'static str) -> Result<u32, ThemePackError> {
    s.parse::<u32>().map_err(|_| ThemePackError::Parse {
        line,
        message: format!("invalid integer for {field}"),
    })
}

fn parse_f32(s: &str, line: usize, field: &'static str) -> Result<f32, ThemePackError> {
    let v = s.parse::<f32>().map_err(|_| ThemePackError::Parse {
        line,
        message: format!("invalid float for {field}"),
    })?;
    if !v.is_finite() {
        return Err(ThemePackError::Parse {
            line,
            message: format!("invalid float for {field}"),
        });
    }
    Ok(v)
}

fn parse_csv_strings(s: &str) -> Vec<String> {
    s.split(',')
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_csv_usize(s: &str, line: usize, field: &'static str) -> Result<Vec<usize>, ThemePackError> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        let value = token.parse::<usize>().map_err(|_| ThemePackError::Parse {
            line,
            message: format!("invalid list entry for {field}"),
        })?;
        out.push(value);
    }
    Ok(out)
}
