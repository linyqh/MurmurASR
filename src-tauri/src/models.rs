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

/// One downloadable file of a model package.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub name: String,
    pub size: u64,
    pub sha256: String,
    pub sources: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    pub id: String,
    #[serde(default)]
    pub kind: ModelKind,
    pub name: String,
    pub family: String,
    /// Section of the model list the model is shown in.
    pub group: String,
    pub precision: String,
    /// Package contents. A single file is loaded directly; a package of
    /// several files is loaded as its directory.
    pub files: Vec<ModelFile>,
    pub license: String,
    pub languages: String,
    pub summary: String,
    pub description: String,
    pub speed: u8,
    pub accuracy: u8,
    pub tags: Vec<String>,
    pub word_timestamps: bool,
    pub speaker_diarization: bool,
    /// Companion aligner model that adds word timestamps once installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aligner: Option<String>,
}

impl CatalogModel {
    /// Total download size of the package.
    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    #[default]
    Asr,
    Aligner,
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

/// Candidate URLs for `file` in the order they should be tried for `source`.
pub fn source_urls(file: &ModelFile, source: DownloadSource) -> Vec<(String, String)> {
    let hf = file.sources.get("huggingface").cloned();
    let ms = file.sources.get("modelscope").cloned();
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

fn file_path(models_root: &Path, model: &CatalogModel, file: &ModelFile) -> PathBuf {
    model_dir(models_root, model).join(&file.name)
}

fn partial_path(models_root: &Path, model: &CatalogModel, file: &ModelFile) -> PathBuf {
    model_dir(models_root, model).join(format!("{}.part", file.name))
}

/// Path handed to the engine: the file itself for single-file models, the
/// package directory otherwise.
pub fn load_path(models_root: &Path, model: &CatalogModel) -> PathBuf {
    match model.files.as_slice() {
        [file] => file_path(models_root, model, file),
        _ => model_dir(models_root, model),
    }
}

fn file_complete(models_root: &Path, model: &CatalogModel, file: &ModelFile) -> bool {
    std::fs::metadata(file_path(models_root, model, file)).is_ok_and(|m| m.len() == file.size)
}

pub fn is_installed(models_root: &Path, model: &CatalogModel) -> bool {
    model
        .files
        .iter()
        .all(|f| file_complete(models_root, model, f))
}

/// Path of `model`'s companion aligner when it is fully downloaded.
pub fn installed_aligner(
    models_root: &Path,
    catalog: &Catalog,
    model: &CatalogModel,
) -> Option<PathBuf> {
    let aligner = catalog.get(model.aligner.as_deref()?)?;
    is_installed(models_root, aligner).then(|| load_path(models_root, aligner))
}

pub fn installed(models_root: &Path, catalog: &Catalog) -> Vec<InstalledModel> {
    catalog
        .models
        .iter()
        .filter(|model| is_installed(models_root, model))
        .map(|model| InstalledModel {
            id: model.id.clone(),
            path: load_path(models_root, model).to_string_lossy().into_owned(),
            size: model.size(),
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
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "state"
)]
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
pub(crate) enum FetchError {
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
    let client = match http_client() {
        Ok(c) => c,
        Err(e) => {
            return DownloadEvent::Failed {
                id,
                error: e.to_string(),
            }
        }
    };

    let total = model.size();
    let mut offset = 0;
    for file in &model.files {
        if !file_complete(models_root, model, file) {
            if let Err(event) =
                download_file(&client, models_root, model, file, source, offset, total, cancel, emit).await
            {
                return event;
            }
        }
        offset += file.size;
    }
    DownloadEvent::Done {
        id,
        path: load_path(models_root, model).to_string_lossy().into_owned(),
    }
}

/// Downloads and verifies one package file, trying each source in turn. A
/// source serving different bytes (failed checksum) is skipped like a failed one.
#[allow(clippy::too_many_arguments)]
async fn download_file(
    client: &reqwest::Client,
    models_root: &Path,
    model: &CatalogModel,
    file: &ModelFile,
    source: DownloadSource,
    offset: u64,
    total: u64,
    cancel: &AtomicBool,
    emit: &(impl Fn(DownloadEvent) + Send + Sync),
) -> Result<(), DownloadEvent> {
    let id = &model.id;
    let partial = partial_path(models_root, model, file);
    let mut errors = Vec::new();
    for (label, url) in source_urls(file, source) {
        let fetched = fetch(
            client, &url, &label, &partial, id, file.size, offset, total, cancel, emit,
        )
        .await;
        match fetched {
            Ok(()) => {}
            Err(FetchError::Cancelled) => return Err(DownloadEvent::Cancelled { id: id.clone() }),
            Err(FetchError::Other(e)) => {
                log::warn!("download {} from {label} failed: {e}", file.name);
                errors.push(format!("{label}：{e}"));
                continue;
            }
        }
        emit(DownloadEvent::Verifying { id: id.clone() });
        match sha256_file(&partial, cancel).await {
            Ok(digest) if digest.eq_ignore_ascii_case(&file.sha256) => {}
            Ok(_) => {
                let _ = tokio::fs::remove_file(&partial).await;
                log::warn!("download {} from {label} failed its checksum", file.name);
                errors.push(format!("{label}：文件校验失败（SHA-256 不匹配）"));
                continue;
            }
            Err(FetchError::Cancelled) => return Err(DownloadEvent::Cancelled { id: id.clone() }),
            Err(FetchError::Other(error)) => {
                return Err(DownloadEvent::Failed {
                    id: id.clone(),
                    error,
                })
            }
        }
        return tokio::fs::rename(&partial, file_path(models_root, model, file))
            .await
            .map_err(|e| DownloadEvent::Failed {
                id: id.clone(),
                error: e.to_string(),
            });
    }
    Err(DownloadEvent::Failed {
        id: id.clone(),
        error: errors.join("；"),
    })
}

pub(crate) fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("Murmur/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .build()
}

/// Downloads `url` into `partial`, resuming from its current length, and
/// reports progress for `id` through `emit`. `size` is this file's length;
/// progress is reported as `offset` bytes already done out of `total`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn fetch(
    client: &reqwest::Client,
    url: &str,
    label: &str,
    partial: &Path,
    id: &str,
    size: u64,
    offset: u64,
    total: u64,
    cancel: &AtomicBool,
    emit: &impl Fn(DownloadEvent),
) -> Result<(), FetchError> {
    let mut have = tokio::fs::metadata(partial)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if have > size {
        tokio::fs::remove_file(partial).await?;
        have = 0;
    }
    if have == size {
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
                id: id.to_string(),
                downloaded: offset + downloaded,
                total,
                bytes_per_sec: rate,
                source: label.to_string(),
            });
        }
    }
    file.flush().await?;
    drop(file);
    log::info!(
        "downloaded {id} from {label} in {:.1}s",
        started.elapsed().as_secs_f64()
    );
    if downloaded != size {
        return Err(FetchError::Other(format!(
            "下载不完整（{downloaded}/{size} 字节）"
        )));
    }
    Ok(())
}

pub(crate) async fn sha256_file(path: &Path, cancel: &AtomicBool) -> Result<String, FetchError> {
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
            assert!(!model.files.is_empty(), "{} has no files", model.id);
            let mut names = std::collections::HashSet::new();
            for file in &model.files {
                assert!(names.insert(&file.name), "{} lists {} twice", model.id, file.name);
                assert_eq!(file.sha256.len(), 64, "{}", file.name);
                assert!(file.size > 0);
                let hf = &file.sources["huggingface"];
                // A branch name lets the bytes change under a fixed checksum.
                assert!(!hf.contains("/resolve/main/"), "{} is not pinned", file.name);
            }
            if let Some(aligner) = &model.aligner {
                let aligner = catalog.get(aligner).expect("aligner is in the catalog");
                assert_eq!(aligner.kind, ModelKind::Aligner);
            }
        }
        let recommended = catalog.get(&catalog.recommended).unwrap();
        assert_eq!(recommended.kind, ModelKind::Asr);
    }

    #[test]
    fn every_catalog_family_is_built_into_the_engine() {
        let lock = include_str!("../../engine/audiocpp.lock");
        let built: Vec<&str> = lock
            .lines()
            .find_map(|l| l.strip_prefix("AUDIOCPP_MODELS="))
            .expect("lock lists models")
            .split(',')
            .collect();
        for model in &Catalog::builtin().models {
            // Build targets that bring in more than one loader family.
            let target = match model.family.as_str() {
                "vibevoice_asr_streaming" => "vibevoice_asr",
                "qwen3_forced_aligner" => "qwen3_asr",
                family => family,
            };
            assert!(built.contains(&target), "{} is not built", model.family);
        }
    }

    #[test]
    fn source_order_follows_preference() {
        let catalog = Catalog::builtin();
        let model = catalog.get("qwen3-asr-0.6b-q8").unwrap();
        let file = &model.files[0];
        let auto = source_urls(file, DownloadSource::Auto);
        assert_eq!(auto[0].0, "Hugging Face");
        assert_eq!(auto.len(), 3);
        let ms = source_urls(file, DownloadSource::Modelscope);
        assert_eq!(ms[0].0, "ModelScope");
        let mirror = source_urls(file, DownloadSource::Hfmirror);
        assert!(mirror[0].1.starts_with("https://hf-mirror.com/"));
    }

    #[test]
    fn models_without_modelscope_fall_back_to_mirror() {
        let catalog = Catalog::builtin();
        let model = catalog.get("sensevoice-small-q8").unwrap();
        let ms = source_urls(&model.files[0], DownloadSource::Modelscope);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].0, "HF Mirror");
    }
}
