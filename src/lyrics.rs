use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct LyricLine {
    pub time_s: f32,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct LyricsTrack {
    lines: Vec<LyricLine>,
    span_s: f32,
}

impl LyricsTrack {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw =
            fs::read_to_string(path).with_context(|| format!("failed reading {}", path.display()))?;
        Self::parse(&raw)
    }

    pub fn parse(input: &str) -> Result<Self> {
        let mut timed = Vec::<LyricLine>::new();
        let mut untimed = Vec::<String>::new();

        for raw_line in input.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            let (timestamps, text, had_tag) = extract_timestamps(line);
            let text = text.trim();

            if !timestamps.is_empty() {
                if text.is_empty() {
                    continue;
                }
                for ts in timestamps {
                    timed.push(LyricLine {
                        time_s: ts,
                        text: text.to_string(),
                    });
                }
                continue;
            }

            if !had_tag {
                untimed.push(text.to_string());
            }
        }

        if timed.is_empty() && untimed.is_empty() {
            return Err(anyhow!("no lyric lines found"));
        }

        if timed.is_empty() {
            let cadence_s = 2.4f32;
            for (i, text) in untimed.into_iter().enumerate() {
                timed.push(LyricLine {
                    time_s: i as f32 * cadence_s,
                    text,
                });
            }
        }

        timed.sort_by(|a, b| a.time_s.total_cmp(&b.time_s));
        let span_s = timed
            .last()
            .map(|x| (x.time_s + 2.4).max(1.0))
            .unwrap_or(1.0);

        Ok(Self { lines: timed, span_s })
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn current_line(&self, t_s: f32, looped: bool) -> Option<&str> {
        if self.lines.is_empty() {
            return None;
        }
        let mut t = t_s;
        if looped {
            t = rem_euclid_f32(t, self.span_s.max(1.0));
        }
        let idx = self.lines.partition_point(|line| line.time_s <= t);
        if idx == 0 {
            return None;
        }
        self.lines.get(idx - 1).map(|line| line.text.as_str())
    }
}

fn rem_euclid_f32(value: f32, modulus: f32) -> f32 {
    if modulus <= 0.0 {
        return value;
    }
    ((value % modulus) + modulus) % modulus
}

fn extract_timestamps(line: &str) -> (Vec<f32>, &str, bool) {
    let mut rest = line;
    let mut times = Vec::<f32>::new();
    let mut had_tag = false;

    while let Some(after_open) = rest.strip_prefix('[') {
        let Some(close_idx) = after_open.find(']') else {
            break;
        };
        had_tag = true;
        let token = &after_open[..close_idx];
        if let Some(ts) = parse_lrc_timestamp(token) {
            times.push(ts);
        }
        rest = &after_open[close_idx + 1..];
    }

    (times, rest, had_tag)
}

fn parse_lrc_timestamp(token: &str) -> Option<f32> {
    // Accept:
    // - mm:ss
    // - mm:ss.xx
    // - hh:mm:ss.xx
    // Reject metadata tags like ti:, ar:, by:
    if token.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }

    let parts: Vec<&str> = token.split(':').collect();
    match parts.as_slice() {
        [m, s] => {
            let min = m.parse::<u32>().ok()?;
            let sec = parse_secs(s)?;
            Some(min as f32 * 60.0 + sec)
        }
        [h, m, s] => {
            let hour = h.parse::<u32>().ok()?;
            let min = m.parse::<u32>().ok()?;
            let sec = parse_secs(s)?;
            Some(hour as f32 * 3600.0 + min as f32 * 60.0 + sec)
        }
        _ => None,
    }
}

fn parse_secs(value: &str) -> Option<f32> {
    let normalized = value.replace(',', ".");
    normalized.parse::<f32>().ok()
}
