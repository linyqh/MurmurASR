//! Persistent settings and transcript history, stored as JSON in the app data dir.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::Segment;
use crate::models::DownloadSource;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub active_model: Option<String>,
    pub backend: String,
    pub threads: i32,
    pub language: String,
    pub itn: bool,
    pub download_source: DownloadSource,
    pub input_device: Option<String>,
    pub save_recordings: bool,
    pub models_dir: Option<String>,
    pub onboarded: bool,
    pub api_enabled: bool,
    pub api_lan: bool,
    pub api_port: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            active_model: None,
            backend: default_backend().into(),
            threads: default_threads(),
            language: "auto".into(),
            itn: true,
            download_source: DownloadSource::Auto,
            input_device: None,
            save_recordings: true,
            models_dir: None,
            onboarded: false,
            api_enabled: true,
            api_lan: true,
            api_port: crate::api::DEFAULT_PORT,
        }
    }
}

fn default_backend() -> &'static str {
    if cfg!(target_os = "macos") {
        "metal"
    } else {
        "best"
    }
}

fn default_threads() -> i32 {
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    (cores.saturating_sub(2)).clamp(2, 8) as i32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub id: String,
    pub title: String,
    /// "file" or "live".
    pub source: String,
    pub created_at: u64,
    pub duration: f64,
    pub language: String,
    pub model: String,
    pub audio_path: Option<String>,
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSummary {
    pub id: String,
    pub title: String,
    pub source: String,
    pub created_at: u64,
    pub duration: f64,
    pub preview: String,
}

impl Transcript {
    pub fn summary(&self) -> TranscriptSummary {
        let preview: String = self
            .segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(120)
            .collect();
        TranscriptSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            source: self.source.clone(),
            created_at: self.created_at,
            duration: self.duration,
            preview,
        }
    }
}

pub struct Store {
    root: PathBuf,
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

impl Store {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(root.join("transcripts"))?;
        std::fs::create_dir_all(root.join("recordings"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn default_models_dir(&self) -> PathBuf {
        self.root.join("models")
    }

    pub fn recordings_dir(&self) -> PathBuf {
        self.root.join("recordings")
    }

    pub fn load_settings(&self) -> Settings {
        std::fs::read(self.root.join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(settings).map_err(std::io::Error::other)?;
        write_atomic(&self.root.join("settings.json"), &bytes)
    }

    fn transcript_path(&self, id: &str) -> Option<PathBuf> {
        valid_id(id).then(|| self.root.join("transcripts").join(format!("{id}.json")))
    }

    pub fn save_transcript(&self, transcript: &Transcript) -> std::io::Result<()> {
        let path = self
            .transcript_path(&transcript.id)
            .ok_or_else(|| std::io::Error::other("invalid transcript id"))?;
        let bytes = serde_json::to_vec(transcript).map_err(std::io::Error::other)?;
        write_atomic(&path, &bytes)
    }

    pub fn load_transcript(&self, id: &str) -> Option<Transcript> {
        let bytes = std::fs::read(self.transcript_path(id)?).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn delete_transcript(&self, id: &str) -> std::io::Result<()> {
        let Some(path) = self.transcript_path(id) else {
            return Ok(());
        };
        if let Some(t) = self.load_transcript(id) {
            if let Some(audio) = t.audio_path {
                let audio = PathBuf::from(audio);
                // Only remove recordings we made ourselves, never user files.
                if audio.starts_with(self.recordings_dir()) {
                    let _ = std::fs::remove_file(audio);
                }
            }
        }
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    pub fn list_transcripts(&self) -> Vec<TranscriptSummary> {
        let Ok(entries) = std::fs::read_dir(self.root.join("transcripts")) else {
            return Vec::new();
        };
        let mut items: Vec<TranscriptSummary> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| std::fs::read(e.path()).ok())
            .filter_map(|b| serde_json::from_slice::<Transcript>(&b).ok())
            .map(|t| t.summary())
            .collect();
        items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        items
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Txt,
    Srt,
    Vtt,
    Json,
    Md,
}

fn timestamp(seconds: f64, separator: char) -> String {
    let total_ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let (h, m, s, ms) = (
        total_ms / 3_600_000,
        (total_ms / 60_000) % 60,
        (total_ms / 1000) % 60,
        total_ms % 1000,
    );
    format!("{h:02}:{m:02}:{s:02}{separator}{ms:03}")
}

pub fn export(transcript: &Transcript, format: ExportFormat) -> String {
    let segments = &transcript.segments;
    match format {
        ExportFormat::Txt => {
            let cjk = segments.iter().any(|s| {
                s.text
                    .chars()
                    .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
            });
            let sep = if cjk { "" } else { " " };
            let mut out = segments
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join(sep);
            out.push('\n');
            out
        }
        ExportFormat::Srt => {
            let mut out = String::new();
            for (i, s) in segments.iter().enumerate() {
                out.push_str(&format!(
                    "{}\n{} --> {}\n{}\n\n",
                    i + 1,
                    timestamp(s.start, ','),
                    timestamp(s.end, ','),
                    s.text
                ));
            }
            out
        }
        ExportFormat::Vtt => {
            let mut out = String::from("WEBVTT\n\n");
            for s in segments {
                out.push_str(&format!(
                    "{} --> {}\n{}\n\n",
                    timestamp(s.start, '.'),
                    timestamp(s.end, '.'),
                    s.text
                ));
            }
            out
        }
        ExportFormat::Json => serde_json::to_string_pretty(transcript).unwrap_or_default(),
        ExportFormat::Md => {
            let mut out = format!("# {}\n\n", transcript.title);
            for s in segments {
                out.push_str(&format!(
                    "**[{}]** {}\n\n",
                    &timestamp(s.start, '.')[..8],
                    s.text
                ));
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Transcript {
        Transcript {
            id: "abc".into(),
            title: "Demo".into(),
            source: "file".into(),
            created_at: 0,
            duration: 5.0,
            language: "en".into(),
            model: "m".into(),
            audio_path: None,
            elapsed_ms: 0,
            segments: vec![
                Segment {
                    index: 0,
                    start: 0.0,
                    end: 1.5,
                    text: "Hello".into(),
                    words: vec![],
                },
                Segment {
                    index: 1,
                    start: 3661.25,
                    end: 3662.0,
                    text: "world".into(),
                    words: vec![],
                },
            ],
        }
    }

    #[test]
    fn srt_format() {
        let srt = export(&sample(), ExportFormat::Srt);
        assert!(srt.starts_with("1\n00:00:00,000 --> 00:00:01,500\nHello\n\n2\n01:01:01,250"));
    }

    #[test]
    fn vtt_format() {
        let vtt = export(&sample(), ExportFormat::Vtt);
        assert!(vtt.starts_with("WEBVTT\n\n00:00:00.000 --> 00:00:01.500\nHello"));
    }

    #[test]
    fn txt_joins_latin_with_spaces_and_cjk_without() {
        assert_eq!(export(&sample(), ExportFormat::Txt), "Hello world\n");
        let mut t = sample();
        t.segments[0].text = "你好".into();
        t.segments[1].text = "世界".into();
        assert_eq!(export(&t, ExportFormat::Txt), "你好世界\n");
    }

    #[test]
    fn rejects_path_traversal_ids() {
        assert!(!valid_id("../etc"));
        assert!(!valid_id(""));
        assert!(valid_id("2f1c-ab"));
    }

    #[test]
    fn store_roundtrip() {
        let dir = std::env::temp_dir().join(format!("murmur-test-{}", uuid::Uuid::new_v4()));
        let store = Store::new(dir.clone()).unwrap();
        let t = sample();
        store.save_transcript(&t).unwrap();
        assert_eq!(store.list_transcripts().len(), 1);
        assert_eq!(store.load_transcript("abc").unwrap().segments.len(), 2);
        store.delete_transcript("abc").unwrap();
        assert!(store.list_transcripts().is_empty());
        let s = Settings {
            language: "zh".into(),
            ..Default::default()
        };
        store.save_settings(&s).unwrap();
        assert_eq!(store.load_settings().language, "zh");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
