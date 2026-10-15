//! Local HTTP API so other programs (and agents) can transcribe through the
//! running app. Jobs go through the same engine queue and history as the UI.
//! Binds to 127.0.0.1 only and refuses browser requests.

use std::collections::HashMap;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;

use crate::commands::{self, AppState};
use crate::engine::EngineEvent;
use crate::models::{self, ModelKind};
use crate::store::{self, ExportFormat, Transcript};

pub const DEFAULT_PORT: u16 = 7895;
const KEEP_FINISHED_JOBS: usize = 256;
const MODEL_LOAD_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Decoding,
    Detecting,
    Transcribing,
    Done,
    Failed,
    Cancelled,
}

impl JobStatus {
    fn finished(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobState {
    id: String,
    status: JobStatus,
    progress: f32,
    duration: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

struct JobEntry {
    tx: watch::Sender<JobState>,
    /// Uploaded copy owned by the API; removed if the job does not finish.
    upload: Option<PathBuf>,
    seq: u64,
}

#[derive(Default)]
struct Jobs {
    map: HashMap<String, JobEntry>,
    seq: u64,
}

struct Server {
    port: u16,
    task: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Default)]
pub struct Api {
    server: Mutex<Option<Server>>,
    error: Mutex<Option<String>>,
    jobs: Mutex<Jobs>,
    model_error: Mutex<Option<(String, String)>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiStatus {
    running: bool,
    url: Option<String>,
    error: Option<String>,
    skill_dir: String,
}

/// Bundled agent skill that documents this API.
pub fn skill_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .resource_dir()
        .map(|dir| dir.join("skills/murmur-asr"))
        .ok()
        .filter(|dir| dir.exists())
        .unwrap_or_else(|| {
            let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../skills/murmur-asr");
            dev.canonicalize().unwrap_or(dev)
        })
}

fn discovery_file(app: &AppHandle) -> PathBuf {
    app.state::<AppState>().store.root().join("api.json")
}

fn base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

impl Api {
    /// Starts, restarts or stops the server to match the settings.
    pub fn configure(&self, app: &AppHandle, enabled: bool, port: u16) {
        let mut server = self.server.lock();
        if enabled && server.as_ref().is_some_and(|s| s.port == port) {
            return;
        }
        let restarted = server.take().map(|old| old.task.abort()).is_some();
        *self.error.lock() = None;
        let discovery = discovery_file(app);
        let _ = std::fs::remove_file(&discovery);
        if !enabled {
            return;
        }
        // An aborted server releases its socket asynchronously.
        let attempts = if restarted { 20 } else { 1 };
        let mut bound = None;
        for attempt in 0..attempts {
            match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
                Ok(listener) => {
                    bound = Some(Ok(listener));
                    break;
                }
                Err(e) => {
                    bound = Some(Err(e));
                    if attempt + 1 < attempts {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            }
        }
        let listener = match bound.expect("at least one attempt") {
            Ok(listener) => listener,
            Err(e) => {
                log::warn!("API server could not bind port {port}: {e}");
                *self.error.lock() = Some(format!("端口 {port} 不可用：{e}"));
                return;
            }
        };
        if let Err(e) = listener.set_nonblocking(true) {
            *self.error.lock() = Some(e.to_string());
            return;
        }
        let router = router(app.clone());
        let task = tauri::async_runtime::spawn(async move {
            match tokio::net::TcpListener::from_std(listener) {
                Ok(listener) => {
                    if let Err(e) = axum::serve(listener, router).await {
                        log::error!("API server stopped: {e}");
                    }
                }
                Err(e) => log::error!("API server could not start: {e}"),
            }
        });
        let info = json!({
            "baseUrl": base_url(port),
            "port": port,
            "pid": std::process::id(),
            "version": env!("CARGO_PKG_VERSION"),
        });
        let _ = std::fs::write(&discovery, info.to_string());
        log::info!("API server listening on {}", base_url(port));
        *server = Some(Server { port, task });
    }

    pub fn status(&self, skill_dir: &Path) -> ApiStatus {
        let port = self.server.lock().as_ref().map(|s| s.port);
        ApiStatus {
            running: port.is_some(),
            url: port.map(base_url),
            error: self.error.lock().clone(),
            skill_dir: skill_dir.to_string_lossy().into_owned(),
        }
    }

    pub fn track(&self, id: &str, upload: Option<PathBuf>) {
        let mut jobs = self.jobs.lock();
        jobs.seq += 1;
        let seq = jobs.seq;
        let (tx, _) = watch::channel(JobState {
            id: id.to_string(),
            status: JobStatus::Queued,
            progress: 0.0,
            duration: 0.0,
            error: None,
        });
        jobs.map
            .insert(id.to_string(), JobEntry { tx, upload, seq });
        while jobs.map.len() > KEEP_FINISHED_JOBS {
            let oldest = jobs
                .map
                .iter()
                .filter(|(_, e)| e.tx.borrow().status.finished())
                .min_by_key(|(_, e)| e.seq)
                .map(|(id, _)| id.clone());
            match oldest {
                Some(id) => jobs.map.remove(&id),
                None => break,
            };
        }
    }

    pub fn forget(&self, id: &str) {
        if let Some(entry) = self.jobs.lock().map.remove(id) {
            if let Some(path) = entry.upload {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    pub fn observe(&self, event: &EngineEvent) {
        let job = match event {
            EngineEvent::ModelError { id, error } => {
                *self.model_error.lock() = Some((id.clone(), error.clone()));
                return;
            }
            EngineEvent::JobPhase { job, .. }
            | EngineEvent::JobStarted { job, .. }
            | EngineEvent::JobSegment { job, .. }
            | EngineEvent::JobDone { job, .. }
            | EngineEvent::JobFailed { job, .. }
            | EngineEvent::JobCancelled { job } => job,
            _ => return,
        };
        let mut jobs = self.jobs.lock();
        let Some(entry) = jobs.map.get_mut(job) else {
            return;
        };
        entry.tx.send_modify(|s| match event {
            EngineEvent::JobPhase {
                phase, progress, ..
            } => {
                s.status = if phase == "detecting" {
                    JobStatus::Detecting
                } else {
                    JobStatus::Decoding
                };
                s.progress = *progress;
            }
            EngineEvent::JobStarted { duration, .. } => s.duration = *duration,
            EngineEvent::JobSegment { progress, .. } => {
                s.status = JobStatus::Transcribing;
                s.progress = *progress;
            }
            EngineEvent::JobDone { .. } => {
                s.status = JobStatus::Done;
                s.progress = 1.0;
            }
            EngineEvent::JobFailed { error, .. } => {
                s.status = JobStatus::Failed;
                s.error = Some(error.clone());
            }
            EngineEvent::JobCancelled { .. } => s.status = JobStatus::Cancelled,
            _ => {}
        });
        let status = entry.tx.borrow().status;
        if matches!(status, JobStatus::Failed | JobStatus::Cancelled) {
            if let Some(path) = entry.upload.take() {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    fn job(&self, id: &str) -> Option<JobState> {
        self.jobs.lock().map.get(id).map(|e| e.tx.borrow().clone())
    }

    async fn wait(&self, id: &str) -> Option<JobState> {
        let mut rx = self.jobs.lock().map.get(id)?.tx.subscribe();
        loop {
            let state = rx.borrow_and_update().clone();
            if state.status.finished() {
                return Some(state);
            }
            if rx.changed().await.is_err() {
                return Some(rx.borrow().clone());
            }
        }
    }
}

fn router(app: AppHandle) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/models", get(list_models))
        .route("/models/load", post(load_model))
        .route("/transcribe", post(transcribe))
        .route("/jobs/{id}", get(job_status))
        .route("/jobs/{id}/cancel", post(cancel_job))
        .route("/transcripts", get(list_transcripts))
        .route("/transcripts/{id}", get(get_transcript))
        .layer(DefaultBodyLimit::disable())
        .layer(middleware::from_fn(local_only))
        .with_state(app)
}

#[derive(Debug)]
struct ApiError(StatusCode, String);

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self(status, message.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

fn is_local_host(host: &str) -> bool {
    let name = match host.rsplit_once(':') {
        Some((name, port)) if port.chars().all(|c| c.is_ascii_digit()) => name,
        _ => host,
    };
    matches!(name, "127.0.0.1" | "localhost" | "[::1]")
}

/// Blocks DNS rebinding and cross-site requests from web pages: only local,
/// non-browser clients may drive the engine or read local files.
async fn local_only(req: Request, next: Next) -> Response {
    let headers = req.headers();
    let local = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(is_local_host);
    if !local || headers.contains_key(header::ORIGIN) {
        return ApiError::new(
            StatusCode::FORBIDDEN,
            "only local non-browser clients may call this API",
        )
        .into_response();
    }
    next.run(req).await
}

async fn health(State(app): State<AppHandle>) -> Json<Value> {
    let state = app.state::<AppState>();
    let (engine_version, engine_error) = match state.engine.version() {
        Ok(v) => (Some(v.to_string()), None),
        Err(e) => (None, Some(e.to_string())),
    };
    let loaded = state.loaded.lock().clone();
    let loading = state.loading.lock().clone();
    let installed: Vec<String> = models::installed(&state.models_dir(), &state.catalog)
        .into_iter()
        .filter(|m| {
            state
                .catalog
                .get(&m.id)
                .is_some_and(|c| c.kind == ModelKind::Asr)
        })
        .map(|m| m.id)
        .collect();
    let settings = state.settings.lock().clone();
    Json(json!({
        "ok": engine_error.is_none() && loaded.is_some(),
        "app": "murmur",
        "version": env!("CARGO_PKG_VERSION"),
        "engine": { "version": engine_version, "error": engine_error },
        "model": loaded,
        "loading": loading,
        "installedModels": installed,
        "defaults": { "language": settings.language, "itn": settings.itn },
        "formats": ["json", "txt", "srt", "vtt", "md"],
    }))
}

async fn list_models(State(app): State<AppHandle>) -> Json<Value> {
    let state = app.state::<AppState>();
    let root = state.models_dir();
    let installed: Vec<String> = models::installed(&root, &state.catalog)
        .into_iter()
        .map(|m| m.id)
        .collect();
    let loaded = state.loaded.lock().as_ref().map(|l| l.id.clone());
    let list: Vec<Value> = state
        .catalog
        .models
        .iter()
        .filter(|m| m.kind == ModelKind::Asr)
        .map(|m| {
            json!({
                "id": m.id,
                "name": m.name,
                "languages": m.languages,
                "summary": m.summary,
                "size": m.size,
                "wordTimestamps": m.word_timestamps,
                "installed": installed.contains(&m.id),
                "loaded": loaded.as_deref() == Some(m.id.as_str()),
            })
        })
        .collect();
    Json(json!({ "recommended": state.catalog.recommended, "models": list }))
}

#[derive(Deserialize)]
struct LoadRequest {
    id: String,
}

async fn load_model(
    State(app): State<AppHandle>,
    Json(req): Json<LoadRequest>,
) -> ApiResult<Json<Value>> {
    let state = app.state::<AppState>();
    if state.loaded.lock().as_ref().is_some_and(|l| l.id == req.id) {
        return Ok(Json(json!({ "model": *state.loaded.lock() })));
    }
    *state.api.model_error.lock() = None;
    commands::start_model_load(&state, req.id.clone())
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    let deadline = tokio::time::Instant::now() + MODEL_LOAD_TIMEOUT;
    while state.loading.lock().as_deref() == Some(req.id.as_str()) {
        if tokio::time::Instant::now() > deadline {
            return Err(ApiError::new(
                StatusCode::GATEWAY_TIMEOUT,
                "model is still loading; check GET /health later",
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let loaded = state.loaded.lock().clone();
    match loaded {
        Some(info) if info.id == req.id => Ok(Json(json!({ "model": info }))),
        _ => {
            let error = state
                .api
                .model_error
                .lock()
                .clone()
                .filter(|(id, _)| *id == req.id)
                .map_or_else(|| "model failed to load".to_string(), |(_, e)| e);
            Err(ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error))
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct TranscribeRequest {
    /// Local file path; alternative to a multipart `file` upload.
    path: Option<String>,
    title: Option<String>,
    language: Option<String>,
    itn: Option<bool>,
    model: Option<String>,
    wait: Option<bool>,
    format: Option<ExportFormat>,
}

/// An uploaded file removed on drop unless handed over to a job.
struct Upload(Option<PathBuf>);

impl Drop for Upload {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn parse_bool(value: &str) -> ApiResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        other => Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("expected true or false, got {other:?}"),
        )),
    }
}

fn parse_format(value: &str) -> ApiResult<ExportFormat> {
    serde_json::from_value(Value::String(value.trim().to_ascii_lowercase())).map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("unknown format {value:?}; use json, txt, srt, vtt or md"),
        )
    })
}

fn file_stem(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "未命名".into())
}

async fn read_multipart(
    state: &AppState,
    mut form: Multipart,
) -> ApiResult<(TranscribeRequest, Upload, Option<String>)> {
    let bad = |e: axum::extract::multipart::MultipartError| {
        ApiError::new(StatusCode::BAD_REQUEST, e.body_text())
    };
    let mut req = TranscribeRequest::default();
    let mut upload = Upload(None);
    let mut upload_name = None;
    while let Some(mut field) = form.next_field().await.map_err(bad)? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            if upload.0.is_some() {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "send one file per request",
                ));
            }
            let original = field.file_name().unwrap_or("upload").to_string();
            let ext = Path::new(&original)
                .extension()
                .and_then(|e| e.to_str())
                .filter(|e| e.chars().all(|c| c.is_ascii_alphanumeric()))
                .unwrap_or("bin");
            // Uploads live with recordings so deleting the transcript removes them.
            let path = state
                .store
                .recordings_dir()
                .join(format!("api-{}.{ext}", uuid::Uuid::new_v4()));
            upload.0 = Some(path.clone());
            let io =
                |e: std::io::Error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
            let mut out = tokio::fs::File::create(&path).await.map_err(io)?;
            while let Some(chunk) = field.chunk().await.map_err(bad)? {
                out.write_all(&chunk).await.map_err(io)?;
            }
            out.flush().await.map_err(io)?;
            upload_name = Some(original);
            continue;
        }
        let value = field.text().await.map_err(bad)?;
        match name.as_str() {
            "path" => req.path = Some(value),
            "title" => req.title = Some(value),
            "language" => req.language = Some(value),
            "itn" => req.itn = Some(parse_bool(&value)?),
            "model" => req.model = Some(value),
            "wait" => req.wait = Some(parse_bool(&value)?),
            "format" => req.format = Some(parse_format(&value)?),
            _ => {}
        }
    }
    Ok((req, upload, upload_name))
}

fn ensure_model(state: &AppState, requested: Option<&str>) -> ApiResult<()> {
    let current = state
        .loading
        .lock()
        .clone()
        .or_else(|| state.loaded.lock().as_ref().map(|l| l.id.clone()));
    match requested.filter(|m| !m.is_empty()) {
        // The engine runs commands in order, so the job waits for this load.
        Some(id) if current.as_deref() != Some(id) => commands::start_model_load(state, id.into())
            .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e)),
        Some(_) => Ok(()),
        None if current.is_some() => Ok(()),
        None => Err(ApiError::new(
            StatusCode::CONFLICT,
            "no model is loaded; pass `model` or call POST /models/load (see GET /models)",
        )),
    }
}

async fn transcribe(State(app): State<AppHandle>, request: Request) -> ApiResult<Response> {
    let state = app.state::<AppState>();
    let multipart = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("multipart/form-data"));
    let (req, mut upload, upload_name) = if multipart {
        let form = Multipart::from_request(request, &())
            .await
            .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e.body_text()))?;
        read_multipart(&state, form).await?
    } else {
        let Json(req) = Json::<TranscribeRequest>::from_request(request, &())
            .await
            .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e.body_text()))?;
        (req, Upload(None), None)
    };

    let (file, default_title) = match (&upload.0, &req.path) {
        (Some(path), None) => (
            path.clone(),
            file_stem(upload_name.as_deref().unwrap_or("")),
        ),
        (None, Some(path)) => {
            let file = PathBuf::from(path);
            if !file.is_file() {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!("file not found: {path}"),
                ));
            }
            (file, file_stem(path))
        }
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "provide either a multipart `file` upload or a local `path`",
            ))
        }
    };
    ensure_model(&state, req.model.as_deref())?;

    let mut options = commands::transcribe_options(&state.settings.lock());
    if let Some(language) = req.language.as_deref().map(str::trim) {
        options.language = (!language.is_empty() && language != "auto").then(|| language.into());
    }
    if let Some(itn) = req.itn {
        options.itn = itn;
    }
    let title = req
        .title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or(default_title);
    let id = commands::queue_file(&state, file, title.clone(), options, upload.0.clone())
        .map_err(|e| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, e))?;
    // The job owns the upload now.
    upload.0 = None;
    let _ = app.emit(commands::EVENT_API_JOB, json!({ "id": id, "title": title }));

    if !req.wait.unwrap_or(true) {
        let body =
            json!({ "id": id, "status": JobStatus::Queued, "statusUrl": format!("/jobs/{id}") });
        return Ok((StatusCode::ACCEPTED, Json(body)).into_response());
    }
    let job = state
        .api
        .wait(&id)
        .await
        .ok_or_else(|| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "job disappeared"))?;
    finished_response(&state, job, req.format.unwrap_or(ExportFormat::Json))
}

fn transcript_json(t: &Transcript) -> Value {
    let mut value = serde_json::to_value(t).unwrap_or_default();
    value["status"] = json!(JobStatus::Done);
    value["text"] = json!(store::export(t, ExportFormat::Txt).trim());
    value
}

fn formatted(t: &Transcript, format: ExportFormat) -> Response {
    if matches!(format, ExportFormat::Json) {
        return Json(transcript_json(t)).into_response();
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/plain; charset=utf-8".parse().expect("static header"),
    );
    if let Ok(id) = t.id.parse() {
        headers.insert("x-murmur-transcript-id", id);
    }
    (headers, store::export(t, format)).into_response()
}

fn finished_response(state: &AppState, job: JobState, format: ExportFormat) -> ApiResult<Response> {
    match job.status {
        JobStatus::Done => state
            .store
            .load_transcript(&job.id)
            .map(|t| formatted(&t, format))
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "transcript was not saved",
                )
            }),
        JobStatus::Cancelled => Err(ApiError::new(StatusCode::CONFLICT, "job was cancelled")),
        _ => Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            job.error.unwrap_or_else(|| "transcription failed".into()),
        )),
    }
}

async fn job_status(
    State(app): State<AppHandle>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<Json<Value>> {
    let state = app.state::<AppState>();
    match state.api.job(&id) {
        Some(job) if job.status == JobStatus::Done => {
            let mut body = json!(job);
            if let Some(t) = state.store.load_transcript(&id) {
                body["result"] = transcript_json(&t);
            }
            Ok(Json(body))
        }
        Some(job) => Ok(Json(json!(job))),
        None => state
            .store
            .load_transcript(&id)
            .map(|t| {
                Json(json!({
                    "id": id,
                    "status": JobStatus::Done,
                    "progress": 1.0,
                    "duration": t.duration,
                    "result": transcript_json(&t),
                }))
            })
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown job")),
    }
}

async fn cancel_job(
    State(app): State<AppHandle>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Json<Value> {
    let cancelled = app.state::<AppState>().cancel(&id);
    Json(json!({ "id": id, "cancelling": cancelled }))
}

#[derive(Deserialize)]
struct ListQuery {
    limit: Option<usize>,
}

async fn list_transcripts(
    State(app): State<AppHandle>,
    Query(query): Query<ListQuery>,
) -> Json<Value> {
    let mut items = app.state::<AppState>().store.list_transcripts();
    items.truncate(query.limit.unwrap_or(20));
    Json(json!({ "transcripts": items }))
}

#[derive(Deserialize)]
struct FormatQuery {
    format: Option<String>,
}

async fn get_transcript(
    State(app): State<AppHandle>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Query(query): Query<FormatQuery>,
) -> ApiResult<Response> {
    let format = query
        .format
        .as_deref()
        .map(parse_format)
        .transpose()?
        .unwrap_or(ExportFormat::Json);
    let t = app
        .state::<AppState>()
        .store
        .load_transcript(&id)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown transcript"))?;
    Ok(formatted(&t, format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_loopback_hosts() {
        assert!(is_local_host("127.0.0.1:7895"));
        assert!(is_local_host("localhost:7895"));
        assert!(is_local_host("[::1]:7895"));
        assert!(is_local_host("localhost"));
        assert!(!is_local_host("evil.example:7895"));
        assert!(!is_local_host("127.0.0.1.evil.example"));
        assert!(!is_local_host("192.168.1.2:7895"));
    }

    #[test]
    fn parses_form_values() {
        assert!(parse_bool("TRUE").unwrap());
        assert!(!parse_bool("0").unwrap());
        assert!(parse_bool("maybe").is_err());
        assert!(matches!(parse_format("SRT").unwrap(), ExportFormat::Srt));
        assert!(parse_format("docx").is_err());
    }
}
