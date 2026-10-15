//! Model catalog, local inventory and resumable, verified downloads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CATALOG_JSON: &str = include_str!("../catalog.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    pub family: String,
    pub precision: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub license: String,
    pub languages: String,
    pub summary: String,
    pub speed: u8,
    pub accuracy: u8,
    pub tags: Vec<String>,
    pub sources: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub version: u32,
    pub recommended: String,
    pub models: Vec<CatalogModel>,
}

impl Catalog {
    pub fn builtin() -> Self {
        serde_json::from_str(CATALOG_JSON).expect("bundled catalog.json is valid")
    }

    pub fn get(&self, id: &str) -> Option<&CatalogModel> {
        self.models.iter().find(|m| m.id == id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DownloadSource {
    #[default]
    Auto,
    Huggingface,
    Hfmirror,
    Modelscope,
}

/// Candidate URLs in the order they should be tried for `source`.
pub fn source_urls(model: &CatalogModel, source: DownloadSource) -> Vec<(String, String)> {
    let hf = model.sources.get("huggingface").cloned();
    let ms = model.sources.get("modelscope").cloned();
    let mirror = hf
        .as_ref()
        .map(|u| u.replacen("https://huggingface.co/", "https://hf-mirror.com/", 1));
    let order: Vec<(&str, Option<String>)> = match source {
        DownloadSource::Huggingface => vec![("Hugging Face", hf)],
        DownloadSource::Hfmirror => vec![("HF Mirror", mirror)],
        DownloadSource::Modelscope => vec![("ModelScope", ms), ("HF Mirror", mirror)],
        DownloadSource::Auto => vec![
            ("Hugging Face", hf),
            ("ModelScope", ms),
            ("HF Mirror", mirror),
        ],
    };
    order
        .into_iter()
        .filter_map(|(label, url)| url.map(|u| (label.to_string(), u)))
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModel {
    pub id: String,
    pub path: String,
    pub size: u64,
}

pub fn model_dir(models_root: &Path, model: &CatalogModel) -> PathBuf {
    models_root.join(&model.id)
}

pub fn model_path(models_root: &Path, model: &CatalogModel) -> PathBuf {
    model_dir(models_root, model).join(&model.file)
}

fn partial_path(models_root: &Path, model: &CatalogModel) -> PathBuf {
    model_dir(models_root, model).join(format!("{}.part", model.file))
}

pub fn installed(models_root: &Path, catalog: &Catalog) -> Vec<InstalledModel> {
    catalog
        .models
        .iter()
        .filter_map(|model| {
            let path = model_path(models_root, model);
            let meta = std::fs::metadata(&path).ok()?;
            (meta.len() == model.size).then(|| InstalledModel {
                id: model.id.clone(),
                path: path.to_string_lossy().into_owned(),
                size: meta.len(),
            })
        })
        .collect()
}

pub fn remove(models_root: &Path, model: &CatalogModel) -> std::io::Result<()> {
    let dir = model_dir(models_root, model);
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum DownloadEvent {
    Progress {
        id: String,
        downloaded: u64,
        total: u64,
        bytes_per_sec: f64,
        source: String,
    },
    Verifying {
        id: String,
    },
    Done {
        id: String,
        path: String,
    },
    Failed {
        id: String,
        error: String,
    },
    Cancelled {
        id: String,
    },
}

#[derive(Default)]
pub struct Downloads {
    active: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl Downloads {
    pub fn is_active(&self, id: &str) -> bool {
        self.active.lock().contains_key(id)
    }

    pub fn cancel(&self, id: &str) -> bool {
        match self.active.lock().get(id) {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    fn begin(&self, id: &str) -> Option<Arc<AtomicBool>> {
        let mut active = self.active.lock();
        if active.contains_key(id) {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        active.insert(id.to_string(), flag.clone());
        Some(flag)
    }

    fn end(&self, id: &str) {
        self.active.lock().remove(id);
    }
}

#[derive(Debug, thiserror::Error)]
enum FetchError {
    #[error("已取消")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

impl From<std::io::Error> for FetchError {
    fn from(e: std::io::Error) -> Self {
        FetchError::Other(e.to_string())
    }
}

/// Downloads `model` into `models_root`, trying each source in turn and
/// resuming any partial file. Emits progress through `emit`.
pub async fn download(
    downloads: Arc<Downloads>,
    models_root: PathBuf,
    model: CatalogModel,
    source: DownloadSource,
    emit: impl Fn(DownloadEvent) + Send + Sync + 'static,
) {
    let Some(cancel) = downloads.begin(&model.id) else {
        return;
    };
    let event = run_download(&models_root, &model, source, &cancel, &emit).await;
    downloads.end(&model.id);
    emit(event);
}

async fn run_download(
    models_root: &Path,
    model: &CatalogModel,
    source: DownloadSource,
    cancel: &AtomicBool,
    emit: &(impl Fn(DownloadEvent) + Send + Sync),
) -> DownloadEvent {
    let id = model.id.clone();
    let dir = model_dir(models_root, model);
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return DownloadEvent::Failed {
            id,
            error: format!("无法创建模型目录：{e}"),
        };
    }
    let client = match reqwest::Client::builder()
        .user_agent(concat!("Murmur/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return DownloadEvent::Failed {
                id,
                error: e.to_string(),
            }
        }
    };

    let partial = partial_path(models_root, model);
    let mut errors = Vec::new();
    for (label, url) in source_urls(model, source) {
        match fetch(&client, &url, &label, &partial, model, cancel, emit).await {
            Ok(()) => {
                errors.clear();
                break;
            }
            Err(FetchError::Cancelled) => return DownloadEvent::Cancelled { id },
            Err(FetchError::Other(e)) => {
                log::warn!("download {} from {label} failed: {e}", model.id);
                errors.push(format!("{label}：{e}"));
            }
        }
    }
    if !errors.is_empty() {
        return DownloadEvent::Failed {
            id,
            error: errors.join("；"),
        };
    }

    emit(DownloadEvent::Verifying { id: id.clone() });
    match sha256_file(&partial, cancel).await {
        Ok(digest) if digest.eq_ignore_ascii_case(&model.sha256) => {}
        Ok(_) => {
            let _ = tokio::fs::remove_file(&partial).await;
            return DownloadEvent::Failed {
                id,
                error: "文件校验失败（SHA-256 不匹配），已删除损坏的文件，请重试".into(),
            };
        }
        Err(FetchError::Cancelled) => return DownloadEvent::Cancelled { id },
        Err(FetchError::Other(e)) => return DownloadEvent::Failed { id, error: e },
    }
    let final_path = model_path(models_root, model);
    if let Err(e) = tokio::fs::rename(&partial, &final_path).await {
        return DownloadEvent::Failed {
            id,
            error: e.to_string(),
        };
    }
    DownloadEvent::Done {
        id,
        path: final_path.to_string_lossy().into_owned(),
    }
}

async fn fetch(
    client: &reqwest::Client,
    url: &str,
    label: &str,
    partial: &Path,
    model: &CatalogModel,
    cancel: &AtomicBool,
    emit: &(impl Fn(DownloadEvent) + Send + Sync),
) -> Result<(), FetchError> {
    let mut have = tokio::fs::metadata(partial)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if have > model.size {
        tokio::fs::remove_file(partial).await?;
        have = 0;
    }
    if have == model.size {
        return Ok(());
    }
    let mut request = client.get(url);
    if have > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let response = request
        .send()
        .await
        .map_err(|e| FetchError::Other(e.without_url().to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(FetchError::Other(format!("HTTP {}", status.as_u16())));
    }
    let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
    if !resumed {
        have = 0;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(partial)
        .await?;

    let mut stream = response.bytes_stream();
    let mut downloaded = have;
    let started = Instant::now();
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let mut window_start = Instant::now();
    let mut window_bytes = 0u64;
    let mut rate = 0f64;
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            file.flush().await?;
            return Err(FetchError::Cancelled);
        }
        let chunk = chunk.map_err(|e| FetchError::Other(e.without_url().to_string()))?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        window_bytes += chunk.len() as u64;
        let elapsed = window_start.elapsed();
        if elapsed >= Duration::from_millis(800) {
            let instant = window_bytes as f64 / elapsed.as_secs_f64();
            rate = if rate == 0.0 {
                instant
            } else {
                rate * 0.6 + instant * 0.4
            };
            window_start = Instant::now();
            window_bytes = 0;
        }
        if last_emit.elapsed() >= Duration::from_millis(200) {
            last_emit = Instant::now();
            emit(DownloadEvent::Progress {
                id: model.id.clone(),
                downloaded,
                total: model.size,
                bytes_per_sec: rate,
                source: label.to_string(),
            });
        }
    }
    file.flush().await?;
    drop(file);
    log::info!(
        "downloaded {} from {label} in {:.1}s",
        model.id,
        started.elapsed().as_secs_f64()
    );
    if downloaded != model.size {
        return Err(FetchError::Other(format!(
            "下载不完整（{downloaded}/{} 字节）",
            model.size
        )));
    }
    Ok(())
}

async fn sha256_file(path: &Path, cancel: &AtomicBool) -> Result<String, FetchError> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 4 << 20];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(FetchError::Cancelled);
        }
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        let catalog = Catalog::builtin();
        assert!(catalog.get(&catalog.recommended).is_some());
        let mut ids = std::collections::HashSet::new();
        for model in &catalog.models {
            assert!(ids.insert(&model.id), "duplicate id {}", model.id);
            assert_eq!(model.sha256.len(), 64);
            assert!(model.size > 0);
            assert!(model.sources.contains_key("huggingface"));
            assert!(model.file.ends_with(".gguf"));
        }
    }

    #[test]
    fn source_order_follows_preference() {
        let catalog = Catalog::builtin();
        let model = catalog.get("qwen3-asr-0.6b-q8").unwrap();
        let auto = source_urls(model, DownloadSource::Auto);
        assert_eq!(auto[0].0, "Hugging Face");
        assert_eq!(auto.len(), 3);
        let ms = source_urls(model, DownloadSource::Modelscope);
        assert_eq!(ms[0].0, "ModelScope");
        let mirror = source_urls(model, DownloadSource::Hfmirror);
        assert!(mirror[0].1.starts_with("https://hf-mirror.com/"));
    }

    #[test]
    fn models_without_modelscope_fall_back_to_mirror() {
        let catalog = Catalog::builtin();
        let model = catalog.get("sensevoice-small-q8").unwrap();
        let ms = source_urls(model, DownloadSource::Modelscope);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].0, "HF Mirror");
    }
}
