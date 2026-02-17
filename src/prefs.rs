use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppPrefs {
    pub stage_mode: bool,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self { stage_mode: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefsError {
    Io(String),
    Parse { line: usize, message: String },
}

impl fmt::Display for PrefsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "I/O error: {msg}"),
            Self::Parse { line, message } => write!(f, "parse error at line {line}: {message}"),
        }
    }
}

impl std::error::Error for PrefsError {}

impl AppPrefs {
    pub fn load(path: Option<&Path>) -> Result<Self, PrefsError> {
        let Some(path) = path else {
            return Ok(Self::default());
        };

        let text = match std::fs::read_to_string(path) {
            Ok(v) => v,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(PrefsError::Io(err.to_string())),
        };

        let mut prefs = Self::default();
        for (line_idx, raw) in text.lines().enumerate() {
            let line_no = line_idx + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key_raw, value_raw)) = line.split_once('=') else {
                return Err(PrefsError::Parse {
                    line: line_no,
                    message: "expected <key>=<value>".to_string(),
                });
            };
            let key = key_raw.trim();
            let value = value_raw.trim();
            match key {
                "stage_mode" => {
                    prefs.stage_mode = parse_bool(value).ok_or_else(|| PrefsError::Parse {
                        line: line_no,
                        message: "stage_mode must be true/false".to_string(),
                    })?;
                }
                _ => {}
            }
        }
        Ok(prefs)
    }

    pub fn save(&self, path: Option<&Path>) -> Result<(), PrefsError> {
        let Some(path) = path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PrefsError::Io(e.to_string()))?;
        }
        let body = format!(
            "# tui_visualizer runtime prefs v1\nstage_mode={}\n",
            if self.stage_mode { "true" } else { "false" }
        );
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &body).map_err(|e| PrefsError::Io(e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| PrefsError::Io(e.to_string()))
    }
}

pub fn prefs_storage_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.trim().is_empty() {
            return Some(PathBuf::from(xdg).join("tui_visualizer").join("prefs.txt"));
        }
    }
    let home = std::env::var("HOME").ok()?;
    if home.trim().is_empty() {
        return None;
    }
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("tui_visualizer")
            .join("prefs.txt"),
    )
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}
