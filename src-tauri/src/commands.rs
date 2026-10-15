//! Tauri commands and shared application state.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::{self, Api, ApiStatus};
use crate::audio::{self, InputDevice};
use crate::engine::{EngineEvent, EngineHandle, LoadedModelInfo, Segment, TranscribeOptions};
use crate::models::{
    self, Catalog, CatalogModel, DownloadEvent, Downloads, InstalledModel, ModelKind,
};
use crate::store::{self, ExportFormat, Settings, Store, Transcript, TranscriptSummary};

pub const EVENT_ENGINE: &str = "engine://event";
pub const EVENT_DOWNLOAD: &str = "models://download";
pub const EVENT_HISTORY: &str = "history://changed";
pub const EVENT_API_JOB: &str = "api://job";

/// A transcript being assembled from engine events.
struct Pending {
    transcript: Transcript,
    cancel: Arc<AtomicBool>,
}

pub struct AppState {
    pub engine: EngineHandle,
    pub store: Store,
    pub catalog: Catalog,
    pub downloads: Arc<Downloads>,
    pub settings: Mutex<Settings>,
    pub loaded: Mutex<Option<LoadedModelInfo>>,
    pub loading: Mutex<Option<String>>,
    pub api: Api,
    pending: Mutex<HashMap<String, Pending>>,
}

impl AppState {
    pub fn new(engine: EngineHandle, store: Store) -> Self {
        let settings = store.load_settings();
        Self {
            engine,
            store,
            catalog: Catalog::builtin(),
            downloads: Arc::new(Downloads::default()),
            settings: Mutex::new(settings),
            loaded: Mutex::new(None),
            loading: Mutex::new(None),
            api: Api::default(),
            pending: Mutex::new(HashMap::new()),
        }
    }

    pub fn models_dir(&self) -> PathBuf {
        self.settings
            .lock()
            .models_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.store.default_models_dir())
    }

    /// Folds engine events into pending transcripts and persists finished ones.
    /// Returns true when the history list changed.
    pub fn observe(&self, event: &EngineEvent) -> bool {
        match event {
            EngineEvent::ModelLoading { id } => {
                *self.loading.lock() = Some(id.clone());
            }
            EngineEvent::ModelLoaded { info } => {
                *self.loading.lock() = None;
                *self.loaded.lock() = Some(info.clone());
            }
            EngineEvent::ModelError { .. } => {
                *self.loading.lock() = None;
                *self.loaded.lock() = None;
            }
            EngineEvent::ModelUnloaded => {
                *self.loaded.lock() = None;
            }
            EngineEvent::JobStarted { job, duration } => {
                if let Some(p) = self.pending.lock().get_mut(job) {
                    p.transcript.duration = *duration;
                }
            }
            EngineEvent::JobSegment { job, segment, .. }
            | EngineEvent::LiveSegment {
                session: job,
                segment,
            } => {
                if let Some(p) = self.pending.lock().get_mut(job) {
                    p.transcript.segments.push(segment.clone());
                }
            }
            EngineEvent::JobDone {
                job,
                language,
                elapsed_ms,
                audio_seconds,
            } => {
                if let Some(mut p) = self.pending.lock().remove(job) {
                    p.transcript.language = language.clone();
                    p.transcript.elapsed_ms = *elapsed_ms;
                    p.transcript.duration = *audio_seconds;
                    return self.persist(p.transcript);
                }
            }
            EngineEvent::LiveStopped {
                session,
                duration,
                audio_path,
            } => {
                if let Some(mut p) = self.pending.lock().remove(session) {
                    p.transcript.duration = *duration;
                    p.transcript.audio_path = audio_path.clone();
                    if p.transcript.segments.is_empty() {
                        if let Some(path) = audio_path {
                            let _ = std::fs::remove_file(path);
                        }
                        return false;
                    }
                    return self.persist(p.transcript);
                }
            }
            EngineEvent::JobFailed { job, .. } | EngineEvent::JobCancelled { job } => {
                self.pending.lock().remove(job);
            }
            EngineEvent::LiveError { session, .. } => {
                self.pending.lock().remove(session);
            }
            _ => {}
        }
        false
    }

    fn persist(&self, transcript: Transcript) -> bool {
        match self.store.save_transcript(&transcript) {
            Ok(()) => true,
            Err(e) => {
                log::error!("failed to save transcript {}: {e}", transcript.id);
                false
            }
        }
    }

    fn new_pending(
        &self,
        source: &str,
        title: String,
        audio_path: Option<String>,
    ) -> (String, Arc<AtomicBool>) {
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        let model = self
            .loaded
            .lock()
            .as_ref()
            .and_then(|l| self.catalog.get(&l.id).map(|m| m.name.clone()))
            .unwrap_or_default();
        let transcript = Transcript {
            id: id.clone(),
            title,
            source: source.into(),
            created_at: now_ms(),
            duration: 0.0,
            language: String::new(),
            model,
            audio_path,
            segments: Vec::new(),
            elapsed_ms: 0,
        };
        self.pending.lock().insert(
            id.clone(),
            Pending {
                transcript,
                cancel: cancel.clone(),
            },
        );
        (id, cancel)
    }

    pub fn cancel(&self, id: &str) -> bool {
        match self.pending.lock().get(id) {
            Some(p) => {
                p.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                true
            }
            None => false,
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

type CmdResult<T> = Result<T, String>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    engine_version: Option<String>,
    engine_error: Option<String>,
    platform: &'static str,
    arch: &'static str,
    data_dir: String,
    models_dir: String,
    backends: Vec<&'static str>,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    let (engine_version, engine_error) = match state.engine.version() {
        Ok(v) => (Some(v.to_string()), None),
        Err(e) => (None, Some(e.to_string())),
    };
    let backends = if cfg!(target_os = "macos") {
        vec!["metal", "cpu"]
    } else {
        vec!["best", "vulkan", "cuda", "cpu"]
    };
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        engine_version,
        engine_error,
        platform: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        data_dir: state.store.root().to_string_lossy().into_owned(),
        models_dir: state.models_dir().to_string_lossy().into_owned(),
        backends,
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().clone()
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<Settings> {
    state
        .store
        .save_settings(&settings)
        .map_err(|e| e.to_string())?;
    *state.settings.lock() = settings.clone();
    state.api.configure(
        &app,
        settings.api_enabled,
        settings.api_lan,
        settings.api_port,
    );
    Ok(settings)
}

#[tauri::command]
pub fn api_status(app: AppHandle, state: State<'_, AppState>) -> ApiStatus {
    state.api.status(&api::skill_dir(&app))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsSnapshot {
    catalog: Catalog,
    installed: Vec<InstalledModel>,
    downloading: Vec<String>,
    loaded: Option<LoadedModelInfo>,
    loading: Option<String>,
}

#[tauri::command]
pub fn list_models(state: State<'_, AppState>) -> ModelsSnapshot {
    let root = state.models_dir();
    ModelsSnapshot {
        installed: models::installed(&root, &state.catalog),
        downloading: state
            .catalog
            .models
            .iter()
            .filter(|m| state.downloads.is_active(&m.id))
            .map(|m| m.id.clone())
            .collect(),
        catalog: state.catalog.clone(),
        loaded: state.loaded.lock().clone(),
        loading: state.loading.lock().clone(),
    }
}

fn catalog_model(state: &AppState, id: &str) -> CmdResult<CatalogModel> {
    state
        .catalog
        .get(id)
        .cloned()
        .ok_or_else(|| format!("未知模型：{id}"))
}

#[tauri::command]
pub fn download_model(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let model = catalog_model(&state, &id)?;
    let root = state.models_dir();
    let source = state.settings.lock().download_source;
    let downloads = state.downloads.clone();
    tauri::async_runtime::spawn(async move {
        let emitter = app.clone();
        models::download(
            downloads,
            root,
            model,
            source,
            move |event: DownloadEvent| {
                if let DownloadEvent::Done { id, .. } = &event {
                    log::info!("model {id} installed");
                }
                let _ = emitter.emit(EVENT_DOWNLOAD, &event);
            },
        )
        .await;
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>, id: String) -> bool {
    state.downloads.cancel(&id)
}

#[tauri::command]
pub fn delete_model(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let model = catalog_model(&state, &id)?;
    // The loaded model may hold this file open as its aligner.
    let in_use = state.loaded.lock().as_ref().is_some_and(|l| {
        l.id == id
            || state.catalog.get(&l.id).and_then(|m| m.aligner.as_deref()) == Some(id.as_str())
    });
    if in_use {
        state.engine.unload().map_err(|e| e.to_string())?;
    }
    {
        let mut settings = state.settings.lock();
        if settings.active_model.as_deref() == Some(id.as_str()) {
            settings.active_model = None;
            let _ = state.store.save_settings(&settings);
        }
    }
    models::remove(&state.models_dir(), &model).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_model(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    start_model_load(&state, id)
}

pub fn start_model_load(state: &AppState, id: String) -> CmdResult<()> {
    let model = catalog_model(state, &id)?;
    if model.kind != ModelKind::Asr {
        return Err(format!("{} 是辅助模型，不能单独用于识别", model.name));
    }
    let root = state.models_dir();
    let path = models::model_path(&root, &model);
    if !path.exists() {
        return Err("模型尚未下载".into());
    }
    let aligner = models::installed_aligner(&root, &state.catalog, &model);
    let (backend, threads) = {
        let mut settings = state.settings.lock();
        settings.active_model = Some(id.clone());
        let _ = state.store.save_settings(&settings);
        (settings.backend.clone(), settings.threads)
    };
    *state.loading.lock() = Some(id.clone());
    state
        .engine
        .load_model(id, path, model.family, aligner, backend, threads)
        .map_err(|e| e.to_string())
}

pub fn transcribe_options(settings: &Settings) -> TranscribeOptions {
    TranscribeOptions {
        language: (settings.language != "auto").then(|| settings.language.clone()),
        itn: settings.itn,
    }
}

#[tauri::command]
pub fn transcribe_file(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    let file = PathBuf::from(&path);
    if !file.is_file() {
        return Err("文件不存在".into());
    }
    if state.loaded.lock().is_none() {
        return Err("请先加载一个模型".into());
    }
    let title = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "未命名".into());
    let options = transcribe_options(&state.settings.lock());
    queue_file(&state, file, title, options, None)
}

/// Queues a file job. `upload` is a copy we own and remove if the job fails.
pub fn queue_file(
    state: &AppState,
    file: PathBuf,
    title: String,
    options: TranscribeOptions,
    upload: Option<PathBuf>,
) -> CmdResult<String> {
    let audio_path = file.to_string_lossy().into_owned();
    let (id, cancel) = state.new_pending("file", title, Some(audio_path));
    state.api.track(&id, upload);
    if let Err(e) = state
        .engine
        .transcribe_file(id.clone(), file, options, cancel)
    {
        state.pending.lock().remove(&id);
        state.api.forget(&id);
        return Err(e.to_string());
    }
    Ok(id)
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, id: String) -> bool {
    state.cancel(&id)
}

#[tauri::command]
pub fn start_live(state: State<'_, AppState>, title: String) -> CmdResult<String> {
    if state.loaded.lock().is_none() {
        return Err("请先加载一个模型".into());
    }
    let settings = state.settings.lock().clone();
    let (id, stop) = state.new_pending("live", title, None);
    let save_to = settings
        .save_recordings
        .then(|| state.store.recordings_dir().join(format!("{id}.wav")));
    state
        .engine
        .start_live(
            id.clone(),
            settings.input_device.clone(),
            transcribe_options(&settings),
            save_to,
            stop,
        )
        .map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
pub fn stop_live(state: State<'_, AppState>, id: String) -> bool {
    state.cancel(&id)
}

#[tauri::command]
pub fn list_input_devices() -> Vec<InputDevice> {
    audio::list_input_devices()
}

#[tauri::command]
pub fn list_history(state: State<'_, AppState>) -> Vec<TranscriptSummary> {
    state.store.list_transcripts()
}

#[tauri::command]
pub fn get_transcript(state: State<'_, AppState>, id: String) -> CmdResult<Transcript> {
    state
        .store
        .load_transcript(&id)
        .ok_or_else(|| "记录不存在".into())
}

#[tauri::command]
pub fn update_transcript(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    title: Option<String>,
    segments: Option<Vec<Segment>>,
) -> CmdResult<Transcript> {
    let mut t = state.store.load_transcript(&id).ok_or("记录不存在")?;
    if let Some(title) = title.filter(|t| !t.trim().is_empty()) {
        t.title = title.trim().to_string();
    }
    if let Some(segments) = segments {
        t.segments = segments;
    }
    state.store.save_transcript(&t).map_err(|e| e.to_string())?;
    let _ = app.emit(EVENT_HISTORY, ());
    Ok(t)
}

#[tauri::command]
pub fn delete_transcript(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state
        .store
        .delete_transcript(&id)
        .map_err(|e| e.to_string())?;
    let _ = app.emit(EVENT_HISTORY, ());
    Ok(())
}

#[tauri::command]
pub fn export_transcript(
    state: State<'_, AppState>,
    id: String,
    format: ExportFormat,
    path: Option<String>,
) -> CmdResult<String> {
    let t = state.store.load_transcript(&id).ok_or("记录不存在")?;
    let content = store::export(&t, format);
    if let Some(path) = path {
        std::fs::write(&path, &content).map_err(|e| e.to_string())?;
    }
    Ok(content)
}

pub fn forward_engine_event(app: &AppHandle, event: EngineEvent) {
    let state = app.state::<AppState>();
    let history_changed = state.observe(&event);
    // After `observe`, so API waiters see the transcript already saved.
    state.api.observe(&event);
    #[cfg(debug_assertions)]
    smoke_hook(app, &state, &event);
    let _ = app.emit(EVENT_ENGINE, &event);
    if history_changed {
        let _ = app.emit(EVENT_HISTORY, ());
    }
}

/// Debug-only end-to-end check: `MURMUR_SMOKE_FILE=/path/audio murmur` transcribes
/// the file once the restored model is loaded and logs what was persisted.
#[cfg(debug_assertions)]
fn smoke_hook(app: &AppHandle, state: &AppState, event: &EngineEvent) {
    let Ok(file) = std::env::var("MURMUR_SMOKE_FILE") else {
        return;
    };
    match event {
        EngineEvent::ModelLoaded { info } => {
            log::info!(
                "SMOKE model loaded: {} on {} in {} ms",
                info.family,
                info.backend,
                info.load_ms
            );
            match transcribe_file(app.state::<AppState>(), file) {
                Ok(id) => log::info!("SMOKE job queued {id}"),
                Err(e) => log::error!("SMOKE queue failed: {e}"),
            }
        }
        EngineEvent::JobDone {
            job,
            elapsed_ms,
            audio_seconds,
            ..
        } => {
            let saved = state.store.load_transcript(job);
            log::info!(
                "SMOKE done {job}: {audio_seconds:.1}s audio in {elapsed_ms} ms, saved={} segments={}",
                saved.is_some(),
                saved.as_ref().map_or(0, |t| t.segments.len())
            );
            if let Some(t) = saved {
                log::info!(
                    "SMOKE text: {}",
                    store::export(&t, ExportFormat::Txt).trim()
                );
            }
        }
        EngineEvent::JobFailed { error, .. } | EngineEvent::ModelError { error, .. } => {
            log::error!("SMOKE failed: {error}");
        }
        _ => {}
    }
}
