use crate::audio::AudioFeatures;
use crate::config::SystemDataMode;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct SystemDataFeed {
    mode: SystemDataMode,
    tokens: Vec<String>,
}

impl SystemDataFeed {
    pub fn capture(mode: SystemDataMode) -> Self {
        let mut tokens = Vec::<String>::new();
        if mode == SystemDataMode::Off {
            return Self { mode, tokens };
        }

        let user = env::var("USER")
            .ok()
            .or_else(|| env::var("LOGNAME").ok())
            .unwrap_or_else(|| "unknown".to_string());
        let host = env::var("HOSTNAME")
            .ok()
            .or_else(|| env::var("COMPUTERNAME").ok())
            .unwrap_or_else(|| "localhost".to_string());
        let shell = env::var("SHELL").unwrap_or_else(|_| "shell".to_string());
        let cwd = env::current_dir().unwrap_or_else(|_| Path::new("/").to_path_buf());

        push_token(&mut tokens, format!("USER {}", mask_value(&user)));
        push_token(&mut tokens, format!("HOST {}", mask_value(&host)));
        push_token(
            &mut tokens,
            format!(
                "SHELL {}",
                shell
                    .rsplit('/')
                    .next()
                    .map_or_else(|| "shell".to_string(), |s| s.to_string())
            ),
        );
        push_token(
            &mut tokens,
            format!("OS {} {}", env::consts::OS, env::consts::ARCH),
        );
        push_token(&mut tokens, format!("PID {:05}", std::process::id() % 100000));
        push_token(
            &mut tokens,
            format!("CWD {}", tail_path(&cwd, if mode == SystemDataMode::Creep { 3 } else { 2 })),
        );

        if mode == SystemDataMode::Creep {
            if let Some(home) = home_dir() {
                push_token(
                    &mut tokens,
                    format!("HOME {}", tail_path(&home, 2)),
                );
                if let Ok(entries) = fs::read_dir(&home) {
                    let mut names = entries
                        .flatten()
                        .filter_map(|e| e.file_name().into_string().ok())
                        .filter(|n| !n.starts_with('.'))
                        .take(16)
                        .collect::<Vec<_>>();
                    names.sort_unstable();
                    for name in names {
                        push_token(&mut tokens, format!("FILE {}", summarize_name(&name)));
                    }
                }
            }
        }

        if tokens.is_empty() {
            tokens.push("LOCAL SIGNALS".to_string());
        }

        Self { mode, tokens }
    }

    pub fn label(&self) -> &'static str {
        match self.mode {
            SystemDataMode::Off => "off",
            SystemDataMode::Subtle => "subtle",
            SystemDataMode::Creep => "creep",
        }
    }

    pub fn token_at(&self, t: f32, audio: &AudioFeatures, beat_pulse: f32) -> Option<&str> {
        if self.tokens.is_empty() || self.mode == SystemDataMode::Off {
            return None;
        }
        let n = self.tokens.len();
        let speed = match self.mode {
            SystemDataMode::Off => 0.0,
            SystemDataMode::Subtle => 0.40,
            SystemDataMode::Creep => 1.20,
        };
        let mut phase = t * speed;
        phase += audio.onset * 4.1;
        phase += audio.beat_strength * 3.3;
        phase += beat_pulse * 2.6;
        phase += audio.centroid * 5.1;
        let idx = (phase.abs() * 2.0) as usize % n;
        self.tokens.get(idx).map(|s| s.as_str())
    }
}

fn push_token(out: &mut Vec<String>, token: String) {
    let cleaned = sanitize_token(&token);
    if cleaned.is_empty() {
        return;
    }
    if out.iter().any(|x| x == &cleaned) {
        return;
    }
    out.push(cleaned);
}

fn sanitize_token(input: &str) -> String {
    let upper = input.to_ascii_uppercase();
    let mut out = String::with_capacity(upper.len());
    for ch in upper.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '_' | '-' | ':' | '.' | '/') {
            out.push(ch);
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn summarize_name(name: &str) -> String {
    let mut trimmed = name.trim().replace('/', "_");
    if trimmed.chars().count() > 20 {
        trimmed = trimmed.chars().take(20).collect();
    }
    trimmed
}

fn mask_value(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    match chars.len() {
        0 => "???".to_string(),
        1 => "*".to_string(),
        2 => format!("{}*", chars[0]),
        _ => format!("{}***{}", chars[0], chars[chars.len() - 1]),
    }
}

fn tail_path(path: &Path, keep: usize) -> String {
    let mut parts = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .filter(|x| !x.is_empty() && *x != "/")
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return "/".to_string();
    }
    if parts.len() > keep {
        parts = parts.split_off(parts.len() - keep);
    }
    parts.join("/")
}

fn home_dir() -> Option<std::path::PathBuf> {
    env::var("HOME").ok().map(std::path::PathBuf::from)
}
