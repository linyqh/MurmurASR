//! The engine worker: one thread owns the audio.cpp registry, the loaded ASR
//! model and the VAD, and serves commands sequentially. Inference never runs on
//! the UI or async runtime threads.

pub mod ffi;
pub mod vad;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{select, Receiver, Sender};
use serde::{Deserialize, Serialize};

use crate::audio::{self, StreamResampler, TARGET_RATE};
use ffi::{AudioInput, Engine, EngineLayout, Model, Registry, RunMode, Session, Word};
use vad::{EndpointEvent, Endpointer, Span};

const SEGMENT_MAX_SECONDS: f32 = 25.0;
const LIVE_MAX_SECONDS: f32 = 20.0;
const LIVE_SILENCE_HOLD_SECONDS: f32 = 0.6;
const LIVE_PARTIAL_INTERVAL: Duration = Duration::from_millis(900);
const LIVE_MIN_PARTIAL_SECONDS: f32 = 0.8;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedModelInfo {
    pub id: String,
    pub family: String,
    pub languages: Vec<String>,
    pub accepts_language: bool,
    pub accepts_itn: bool,
    #[serde(skip)]
    accepts_chunk_mode: bool,
    /// Whether requests ask the model for `return_timestamps`.
    #[serde(skip)]
    request_timestamps: bool,
    /// Whether the model emits sub-word tokens that must be joined into words.
    #[serde(skip)]
    merge_tokens: bool,
    pub word_timestamps: bool,
    pub backend: String,
    pub load_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub index: usize,
    pub start: f64,
    pub end: f64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum EngineEvent {
    ModelLoading {
        id: String,
    },
    ModelLoaded {
        info: LoadedModelInfo,
    },
    ModelError {
        id: String,
        error: String,
    },
    ModelUnloaded,

    JobStarted {
        job: String,
        duration: f64,
    },
    JobPhase {
        job: String,
        phase: String,
        progress: f32,
    },
    JobSegment {
        job: String,
        segment: Segment,
        progress: f32,
    },
    JobDone {
        job: String,
        language: String,
        elapsed_ms: u64,
        audio_seconds: f64,
    },
    JobFailed {
        job: String,
        error: String,
    },
    JobCancelled {
        job: String,
    },

    LiveStarted {
        session: String,
    },
    LiveLevel {
        session: String,
        rms: f32,
    },
    LivePartial {
        session: String,
        text: String,
    },
    LiveSegment {
        session: String,
        segment: Segment,
    },
    LiveStopped {
        session: String,
        duration: f64,
        audio_path: Option<String>,
    },
    LiveError {
        session: String,
        error: String,
    },
}

#[derive(Debug, Clone)]
pub struct TranscribeOptions {
    pub language: Option<String>,
    pub itn: bool,
}

enum Command {
    LoadModel {
        id: String,
        path: PathBuf,
        family: String,
        aligner: Option<PathBuf>,
        backend: String,
        threads: i32,
    },
    Unload,
    TranscribeFile {
        job: String,
        path: PathBuf,
        options: TranscribeOptions,
        cancel: Arc<AtomicBool>,
    },
    StartLive {
        session: String,
        device: Option<String>,
        options: TranscribeOptions,
        save_to: Option<PathBuf>,
        stop: Arc<AtomicBool>,
    },
    /// Drop every engine handle, then acknowledge. Must complete before the
    /// process exits: ggml's static Metal device destructor aborts if any
    /// buffer is still alive when `exit()` runs its destructors.
    Shutdown(Sender<()>),
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("推理引擎不可用：{0}")]
    Unavailable(String),
    #[error("推理线程已退出")]
    Gone,
}

pub struct EngineHandle {
    tx: Sender<Command>,
    status: Result<String, String>,
    /// Set on shutdown so queued commands are skipped.
    abort: Arc<AtomicBool>,
    /// Cancel flag of the job or live session currently running, if any.
    active: ActiveFlag,
}

type ActiveFlag = Arc<parking_lot::Mutex<Option<Arc<AtomicBool>>>>;

impl EngineHandle {
    pub fn spawn(layout: EngineLayout, emit: impl Fn(EngineEvent) + Send + 'static) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let abort = Arc::new(AtomicBool::new(false));
        let worker_abort = abort.clone();
        let active: ActiveFlag = Arc::default();
        let worker_active = active.clone();
        let status = match Engine::load(&layout.library) {
            Ok(engine) => {
                let version = engine.build_version();
                std::thread::Builder::new()
                    .name("murmur-engine".into())
                    .stack_size(8 << 20)
                    .spawn(move || {
                        Worker::new(engine, layout, emit, worker_abort).run(rx, worker_active)
                    })
                    .expect("spawn engine thread");
                Ok(version)
            }
            Err(e) => {
                log::error!("engine load failed: {e}");
                Err(e.to_string())
            }
        };
        Self {
            tx,
            status,
            abort,
            active,
        }
    }

    /// Stops any in-flight work and waits until the worker has released all
    /// engine handles. Returns false if the worker did not finish in time.
    pub fn shutdown(&self, timeout: Duration) -> bool {
        if self.status.is_err() {
            return true;
        }
        self.abort.store(true, Ordering::Relaxed);
        if let Some(flag) = self.active.lock().as_ref() {
            flag.store(true, Ordering::Relaxed);
        }
        let (ack_tx, ack_rx) = crossbeam_channel::bounded(1);
        if self.tx.send(Command::Shutdown(ack_tx)).is_err() {
            return true;
        }
        ack_rx.recv_timeout(timeout).is_ok()
    }

    pub fn version(&self) -> Result<&str, WorkerError> {
        self.status
            .as_deref()
            .map_err(|e| WorkerError::Unavailable(e.to_string()))
    }

    fn send(&self, command: Command) -> Result<(), WorkerError> {
        self.version()?;
        self.tx.send(command).map_err(|_| WorkerError::Gone)
    }

    pub fn load_model(
        &self,
        id: String,
        path: PathBuf,
        family: String,
        aligner: Option<PathBuf>,
        backend: String,
        threads: i32,
    ) -> Result<(), WorkerError> {
        self.send(Command::LoadModel {
            id,
            path,
            family,
            aligner,
            backend,
            threads,
        })
    }

    pub fn unload(&self) -> Result<(), WorkerError> {
        self.send(Command::Unload)
    }

    pub fn transcribe_file(
        &self,
        job: String,
        path: PathBuf,
        options: TranscribeOptions,
        cancel: Arc<AtomicBool>,
    ) -> Result<(), WorkerError> {
        self.send(Command::TranscribeFile {
            job,
            path,
            options,
            cancel,
        })
    }

    pub fn start_live(
        &self,
        session: String,
        device: Option<String>,
        options: TranscribeOptions,
        save_to: Option<PathBuf>,
        stop: Arc<AtomicBool>,
    ) -> Result<(), WorkerError> {
        self.send(Command::StartLive {
            session,
            device,
            options,
            save_to,
            stop,
        })
    }
}

struct Loaded {
    info: LoadedModelInfo,
    model: Model,
    session: Session,
}

struct Worker<F: Fn(EngineEvent)> {
    engine: Engine,
    layout: EngineLayout,
    registry: Option<Registry>,
    vad_model: Option<Model>,
    loaded: Option<Loaded>,
    emit: F,
    abort: Arc<AtomicBool>,
}

impl<F: Fn(EngineEvent)> Worker<F> {
    fn new(engine: Engine, layout: EngineLayout, emit: F, abort: Arc<AtomicBool>) -> Self {
        Self {
            engine,
            layout,
            registry: None,
            vad_model: None,
            loaded: None,
            emit,
            abort,
        }
    }

    fn release_engine(&mut self) {
        self.loaded = None;
        self.vad_model = None;
        self.registry = None;
    }

    fn run(mut self, rx: Receiver<Command>, active: ActiveFlag) {
        while let Ok(command) = rx.recv() {
            if self.abort.load(Ordering::Relaxed) && !matches!(command, Command::Shutdown(_)) {
                continue;
            }
            match command {
                Command::Shutdown(ack) => {
                    self.release_engine();
                    let _ = ack.send(());
                    return;
                }
                Command::LoadModel {
                    id,
                    path,
                    family,
                    aligner,
                    backend,
                    threads,
                } => self.load(id, &path, &family, aligner.as_deref(), &backend, threads),
                Command::Unload => {
                    self.loaded = None;
                    (self.emit)(EngineEvent::ModelUnloaded);
                }
                Command::TranscribeFile {
                    job,
                    path,
                    options,
                    cancel,
                } => {
                    let started = Instant::now();
                    *active.lock() = Some(cancel.clone());
                    let result = self.transcribe_file(&job, &path, &options, &cancel);
                    *active.lock() = None;
                    match result {
                        Ok((language, audio_seconds)) => (self.emit)(EngineEvent::JobDone {
                            job,
                            language,
                            elapsed_ms: started.elapsed().as_millis() as u64,
                            audio_seconds,
                        }),
                        Err(_) if cancel.load(Ordering::Relaxed) => {
                            (self.emit)(EngineEvent::JobCancelled { job })
                        }
                        Err(error) => (self.emit)(EngineEvent::JobFailed { job, error }),
                    }
                }
                Command::StartLive {
                    session,
                    device,
                    options,
                    save_to,
                    stop,
                } => {
                    *active.lock() = Some(stop.clone());
                    let result = self.live(&session, device, &options, save_to, &stop);
                    *active.lock() = None;
                    if let Err(error) = result {
                        (self.emit)(EngineEvent::LiveError { session, error });
                    }
                }
            }
        }
        self.release_engine();
    }

    fn registry(&mut self) -> Result<&Registry, String> {
        if self.registry.is_none() {
            self.registry = Some(self.engine.registry().map_err(|e| e.to_string())?);
        }
        Ok(self.registry.as_ref().expect("registry initialised above"))
    }

    fn vad_session(&mut self) -> Result<Session, String> {
        if self.vad_model.is_none() {
            let path = self.layout.silero_vad.clone();
            let model = self
                .registry()?
                .load_model(&path, Some("silero_vad"), &[])
                .map_err(|e| format!("加载 VAD 失败：{e}"))?;
            self.vad_model = Some(model);
        }
        self.vad_model
            .as_ref()
            .expect("vad model initialised above")
            .session("vad", RunMode::Streaming, "cpu", 1, &[])
            .map_err(|e| e.to_string())
    }

    fn load(
        &mut self,
        id: String,
        path: &Path,
        family: &str,
        aligner: Option<&Path>,
        backend: &str,
        threads: i32,
    ) {
        (self.emit)(EngineEvent::ModelLoading { id: id.clone() });
        // Release the previous model first so two never sit in memory together.
        self.loaded = None;
        let started = Instant::now();
        let vad_path = self.layout.silero_vad.to_string_lossy().into_owned();
        let result = (|| -> Result<Loaded, String> {
            let model = self
                .registry()?
                .load_model(path, Some(family), &[])
                .map_err(|e| e.to_string())?;
            if !model.supports("asr", RunMode::Offline) {
                return Err(format!("模型 {family} 不支持语音识别"));
            }
            // Only pass options a family declares: some validate strictly.
            let (session_names, request_names) = declared_options(&model, family);
            let mut session_options = Vec::new();
            if session_names.iter().any(|n| n == "vad_model_path") {
                session_options.push(("vad_model_path", vad_path.clone()));
            }
            // Families such as Qwen3-ASR only produce word timestamps through a
            // companion forced aligner, and reject `return_timestamps` without one.
            let aligner_option = session_names
                .iter()
                .find(|n| n.ends_with("forced_aligner_path"));
            let aligner_attached = match (aligner_option, aligner) {
                (Some(name), Some(aligner)) => {
                    session_options.push((name.as_str(), aligner.to_string_lossy().into_owned()));
                    true
                }
                _ => false,
            };
            let options = session_options.as_slice();
            let (session, used_backend) =
                match model.session("asr", RunMode::Offline, backend, threads, options) {
                    Ok(s) => (s, backend.to_string()),
                    Err(e) if backend != "cpu" => {
                        log::warn!("{backend} session failed ({e}); falling back to cpu");
                        let s = model
                            .session("asr", RunMode::Offline, "cpu", threads, options)
                            .map_err(|e| e.to_string())?;
                        (s, "cpu".to_string())
                    }
                    Err(e) => return Err(e.to_string()),
                };
            let declares = |name: &str| request_names.iter().any(|n| n == name);
            let request_timestamps =
                declares("return_timestamps") && (aligner_option.is_none() || aligner_attached);
            let info = LoadedModelInfo {
                id: id.clone(),
                family: model.family(),
                languages: model.languages(),
                accepts_language: declares("language"),
                accepts_itn: declares("enable_itn"),
                accepts_chunk_mode: declares("audio_chunk_mode"),
                request_timestamps,
                merge_tokens: family == "nemotron_asr",
                word_timestamps: request_timestamps || model.supports_timestamps(),
                backend: used_backend,
                load_ms: 0,
            };
            Ok(Loaded {
                info,
                model,
                session,
            })
        })();
        match result {
            Ok(mut loaded) => {
                // Warm the graph so the first real request is fast.
                let silence = vec![0.0f32; TARGET_RATE as usize];
                let _ = loaded.session.transcribe(
                    &AudioInput {
                        samples: &silence,
                        sample_rate: TARGET_RATE,
                    },
                    &[],
                );
                loaded.info.load_ms = started.elapsed().as_millis() as u64;
                log::info!(
                    "loaded {} ({}) on {} in {} ms",
                    loaded.info.id,
                    loaded.info.family,
                    loaded.info.backend,
                    loaded.info.load_ms
                );
                (self.emit)(EngineEvent::ModelLoaded {
                    info: loaded.info.clone(),
                });
                let _ = &loaded.model;
                self.loaded = Some(loaded);
            }
            Err(error) => (self.emit)(EngineEvent::ModelError { id, error }),
        }
    }

    fn request_options(
        info: &LoadedModelInfo,
        options: &TranscribeOptions,
        timestamps: bool,
    ) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if info.accepts_language {
            if let Some(lang) = options.language.as_deref().filter(|l| !l.is_empty()) {
                out.push(("language", lang.to_string()));
            }
        }
        if info.accepts_itn {
            out.push(("enable_itn", options.itn.to_string()));
        }
        // Segments are already short; keep the model from re-chunking them.
        if info.accepts_chunk_mode {
            out.push(("audio_chunk_mode", "none".to_string()));
        }
        if timestamps && info.request_timestamps {
            out.push(("return_timestamps", "true".to_string()));
        }
        out
    }

    /// Transcribes one chunk, retrying without timestamps if alignment fails so
    /// a single bad alignment cannot sink the whole job.
    fn transcribe_chunk(
        loaded: &mut Loaded,
        samples: &[f32],
        request: &[(&str, String)],
        plain: &[(&str, String)],
    ) -> Result<ffi::Transcript, String> {
        let audio = AudioInput {
            samples,
            sample_rate: TARGET_RATE,
        };
        let mut transcript = match loaded.session.transcribe(&audio, request) {
            Ok(t) => t,
            Err(e) if request.len() != plain.len() => {
                log::warn!("transcription with timestamps failed ({e}); retrying without");
                loaded
                    .session
                    .transcribe(&audio, plain)
                    .map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        };
        if loaded.info.merge_tokens {
            transcript.words = merge_tokens(transcript.words);
        }
        Ok(transcript)
    }

    fn transcribe_file(
        &mut self,
        job: &str,
        path: &Path,
        options: &TranscribeOptions,
        cancel: &AtomicBool,
    ) -> Result<(String, f64), String> {
        if self.loaded.is_none() {
            return Err("请先在“模型”中下载并加载一个模型".into());
        }
        let emit = &self.emit;
        emit(EngineEvent::JobPhase {
            job: job.into(),
            phase: "decoding".into(),
            progress: 0.0,
        });
        let samples = audio::decode_file_mono_16k(path, cancel, |p| {
            emit(EngineEvent::JobPhase {
                job: job.into(),
                phase: "decoding".into(),
                progress: p,
            })
        })
        .map_err(|e| e.to_string())?;
        let rate = TARGET_RATE as f64;
        let duration = samples.len() as f64 / rate;
        (self.emit)(EngineEvent::JobStarted {
            job: job.into(),
            duration,
        });

        let mut vad = self.vad_session()?;
        let total = samples.len();
        let spans = vad::detect_speech(&mut vad, &samples, TARGET_RATE, |offset| {
            if offset % (TARGET_RATE as usize * 10) < vad::VAD_BLOCK {
                (self.emit)(EngineEvent::JobPhase {
                    job: job.into(),
                    phase: "detecting".into(),
                    progress: offset as f32 / total.max(1) as f32,
                });
            }
            cancel.load(Ordering::Relaxed)
        })
        .map_err(|e| e.to_string())?;
        drop(vad);
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }

        let chunks = file_chunks(&spans, total);
        let loaded = self.loaded.as_mut().expect("checked above");
        let request = Self::request_options(&loaded.info, options, true);
        let plain = Self::request_options(&loaded.info, options, false);
        let speech_total: usize = chunks.iter().map(Span::samples).sum::<usize>().max(1);
        let mut done = 0usize;
        let mut language = String::new();
        let mut index = 0;
        for chunk in &chunks {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let piece = &samples[chunk.start..chunk.end];
            let transcript = Self::transcribe_chunk(loaded, piece, &request, &plain)?;
            done += chunk.samples();
            if language.is_empty() && !transcript.language.is_empty() {
                language = transcript.language.clone();
            }
            let text = clean_text(&transcript.text);
            if text.is_empty() {
                continue;
            }
            let offset = chunk.start as f64 / rate;
            let segment = Segment {
                index,
                start: offset,
                end: chunk.end as f64 / rate,
                text,
                words: offset_words(transcript.words, offset),
            };
            index += 1;
            (self.emit)(EngineEvent::JobSegment {
                job: job.into(),
                segment,
                progress: done as f32 / speech_total as f32,
            });
        }
        Ok((language, duration))
    }

    fn live(
        &mut self,
        session_id: &str,
        device: Option<String>,
        options: &TranscribeOptions,
        save_to: Option<PathBuf>,
        stop: &AtomicBool,
    ) -> Result<(), String> {
        if self.loaded.is_none() {
            return Err("请先在“模型”中下载并加载一个模型".into());
        }
        let mut vad = self.vad_session()?;
        vad.stream_start().map_err(|e| e.to_string())?;
        let (audio_tx, audio_rx) = crossbeam_channel::bounded::<(u32, Vec<f32>)>(256);
        let capture = audio::start_capture(device, audio_tx).map_err(|e| e.to_string())?;
        (self.emit)(EngineEvent::LiveStarted {
            session: session_id.into(),
        });

        let r = TARGET_RATE as usize;
        let mut endpointer = Endpointer::new(
            (r as f32 * LIVE_SILENCE_HOLD_SECONDS) as usize,
            (r as f32 * LIVE_MAX_SECONDS) as usize,
        );
        let mut resampler: Option<StreamResampler> = None;
        let mut recording: Vec<f32> = Vec::new();
        let mut last_partial = Instant::now();
        let mut partial_len = 0usize;
        let mut index = 0usize;
        let mut level_acc = (0f32, 0usize);
        let mut last_level = Instant::now();
        let ticker = crossbeam_channel::tick(Duration::from_millis(100));
        let loaded = self.loaded.as_mut().expect("checked above");
        let request = Self::request_options(&loaded.info, options, true);
        let plain = Self::request_options(&loaded.info, options, false);
        let emit = &self.emit;

        // Partials skip timestamps: alignment on every refresh would stall the mic loop.
        let mut transcribe =
            |span: Span, recording: &[f32], final_: bool| -> Result<Segment, String> {
                let start = span.start.saturating_sub(r / 5);
                let end = (span.end + r / 10).min(recording.len());
                let mut segment = Segment {
                    index: 0,
                    start: span.start as f64 / r as f64,
                    end: span.end as f64 / r as f64,
                    text: String::new(),
                    words: Vec::new(),
                };
                if end <= start + r / 10 {
                    return Ok(segment);
                }
                let request = if final_ { &request } else { &plain };
                let transcript =
                    Self::transcribe_chunk(loaded, &recording[start..end], request, &plain)?;
                let offset = start as f64 / r as f64;
                segment.text = clean_text(&transcript.text);
                segment.words = offset_words(transcript.words, offset);
                Ok(segment)
            };

        let result = loop {
            if stop.load(Ordering::Relaxed) {
                break Ok(());
            }
            select! {
                recv(audio_rx) -> msg => {
                    let Ok((rate, block)) = msg else { break Err("麦克风已断开".to_string()) };
                    let rs = resampler.get_or_insert_with(|| StreamResampler::new(rate, TARGET_RATE));
                    let pcm = rs.process(&block);
                    for &s in &pcm {
                        level_acc.0 += s * s;
                    }
                    level_acc.1 += pcm.len();
                    recording.extend_from_slice(&pcm);
                    let events = match endpointer.feed(&mut vad, &pcm) {
                        Ok(events) => events,
                        Err(e) => break Err(e.to_string()),
                    };
                    for event in events {
                        match event {
                            EndpointEvent::SpeechStart(_) => {
                                last_partial = Instant::now();
                                partial_len = 0;
                            }
                            EndpointEvent::Utterance { span, .. } => {
                                match transcribe(span, &recording, true) {
                                    Ok(segment) if !segment.text.is_empty() => {
                                        emit(EngineEvent::LiveSegment {
                                            session: session_id.into(),
                                            segment: Segment { index, ..segment },
                                        });
                                        index += 1;
                                    }
                                    Ok(_) => {}
                                    Err(e) => log::warn!("live segment failed: {e}"),
                                }
                                emit(EngineEvent::LivePartial { session: session_id.into(), text: String::new() });
                                partial_len = 0;
                            }
                        }
                    }
                }
                recv(ticker) -> _ => {
                    if last_level.elapsed() >= Duration::from_millis(80) && level_acc.1 > 0 {
                        let rms = (level_acc.0 / level_acc.1 as f32).sqrt();
                        emit(EngineEvent::LiveLevel { session: session_id.into(), rms });
                        level_acc = (0.0, 0);
                        last_level = Instant::now();
                    }
                    if endpointer.in_speech() && last_partial.elapsed() >= LIVE_PARTIAL_INTERVAL {
                        let start = endpointer.speech_start();
                        let now = recording.len();
                        let grown = now.saturating_sub(start);
                        if grown >= (r as f32 * LIVE_MIN_PARTIAL_SECONDS) as usize && now > partial_len {
                            if let Ok(segment) = transcribe(Span { start, end: now }, &recording, false) {
                                emit(EngineEvent::LivePartial { session: session_id.into(), text: segment.text });
                            }
                            partial_len = now;
                        }
                        last_partial = Instant::now();
                    }
                }
            }
        };

        drop(capture);
        // Drain whatever the device delivered before it stopped.
        while let Ok((rate, block)) = audio_rx.try_recv() {
            let rs = resampler.get_or_insert_with(|| StreamResampler::new(rate, TARGET_RATE));
            recording.extend(rs.process(&block));
        }
        if let Some(span) = endpointer.flush(recording.len()) {
            if let Ok(segment) = transcribe(span, &recording, true) {
                if !segment.text.is_empty() {
                    emit(EngineEvent::LiveSegment {
                        session: session_id.into(),
                        segment: Segment { index, ..segment },
                    });
                }
            }
        }
        emit(EngineEvent::LivePartial {
            session: session_id.into(),
            text: String::new(),
        });
        let _ = vad.stream_finish();

        let audio_path = save_to.and_then(|path| match audio::write_wav_16k(&path, &recording) {
            Ok(()) => Some(path.to_string_lossy().into_owned()),
            Err(e) => {
                log::warn!("failed to save recording: {e}");
                None
            }
        });
        emit(EngineEvent::LiveStopped {
            session: session_id.into(),
            duration: recording.len() as f64 / r as f64,
            audio_path,
        });
        result
    }
}

/// Speech chunks for offline file transcription: 200 ms padding, gaps under
/// 800 ms merged, at most `SEGMENT_MAX_SECONDS` per chunk.
fn file_chunks(spans: &[Span], total: usize) -> Vec<Span> {
    let r = TARGET_RATE as usize;
    vad::plan_chunks(
        spans,
        total,
        r / 5,
        (r as f32 * 0.8) as usize,
        (r as f32 * SEGMENT_MAX_SECONDS) as usize,
    )
}

/// Drives the real worker synchronously, for integration tests.
#[doc(hidden)]
pub mod testing {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Feeds audio through the live endpointer in 100 ms blocks (as the mic
    /// path does) and returns the detected utterances in seconds.
    pub fn live_utterances(layout: EngineLayout, audio: &Path) -> Result<Vec<(f64, f64)>, String> {
        let engine = Engine::load(&layout.library).map_err(|e| e.to_string())?;
        let mut worker = Worker::new(engine, layout, |_| {}, Arc::default());
        let samples = audio::decode_file_mono_16k(audio, &AtomicBool::new(false), |_| {})
            .map_err(|e| e.to_string())?;
        let mut vad = worker.vad_session()?;
        vad.stream_start().map_err(|e| e.to_string())?;
        let r = TARGET_RATE as usize;
        let mut ep = Endpointer::new(
            (r as f32 * LIVE_SILENCE_HOLD_SECONDS) as usize,
            (r as f32 * LIVE_MAX_SECONDS) as usize,
        );
        let mut out = Vec::new();
        for block in samples.chunks(r / 10) {
            for e in ep.feed(&mut vad, block).map_err(|e| e.to_string())? {
                if let EndpointEvent::Utterance { span, .. } = e {
                    out.push((span.start as f64 / r as f64, span.end as f64 / r as f64));
                }
            }
        }
        if let Some(span) = ep.flush(samples.len()) {
            out.push((span.start as f64 / r as f64, span.end as f64 / r as f64));
        }
        drop(vad);
        Ok(out)
    }

    pub fn run_file(
        layout: EngineLayout,
        model_path: &Path,
        family: &str,
        aligner: Option<&Path>,
        backend: &str,
        audio: &Path,
        language: Option<String>,
    ) -> Result<Vec<EngineEvent>, String> {
        let engine = Engine::load(&layout.library).map_err(|e| e.to_string())?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        let mut worker = Worker::new(
            engine,
            layout,
            move |e| sink.borrow_mut().push(e),
            Arc::default(),
        );
        worker.load("test".into(), model_path, family, aligner, backend, 4);
        if let Some(EngineEvent::ModelError { error, .. }) = events.borrow().last() {
            return Err(error.clone());
        }
        let options = TranscribeOptions {
            language,
            itn: true,
        };
        let result = worker.transcribe_file("job", audio, &options, &AtomicBool::new(false));
        drop(worker);
        result?;
        let out = events.borrow().clone();
        Ok(out)
    }
}

/// Strips model control tags (e.g. SenseVoice `<|zh|>`) and tidies whitespace.
pub fn clean_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<|") {
        out.push_str(&rest[..start]);
        match rest[start..].find("|>") {
            Some(end) => rest = &rest[start + end + 2..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Option names the model accepts as (session, request). Some loaders (Qwen3-ASR,
/// Fun-ASR) don't expose their spec through introspection, so the options Murmur
/// uses are listed here from audio.cpp's `model_specs/<family>.json`.
fn declared_options(model: &Model, family: &str) -> (Vec<String>, Vec<String>) {
    let (session, request) = (model.option_names(true), model.option_names(false));
    if !session.is_empty() || !request.is_empty() {
        return (session, request);
    }
    let (session, request): (&[&str], &[&str]) = match family {
        "qwen3_asr" => (
            &["qwen3_asr.forced_aligner_path"],
            &["language", "return_timestamps", "audio_chunk_mode"],
        ),
        "fun_asr_nano" => (&[], &["language", "enable_itn", "audio_chunk_mode"]),
        _ => (&[], &[]),
    };
    let owned = |names: &[&str]| names.iter().map(|n| n.to_string()).collect();
    (owned(session), owned(request))
}

/// Shifts chunk-relative word times onto the recording timeline, dropping
/// entries with no visible text.
fn offset_words(words: Vec<Word>, offset: f64) -> Vec<Word> {
    words
        .into_iter()
        .filter_map(|w| {
            let text = w.text.trim();
            (!text.is_empty()).then(|| Word {
                text: text.to_string(),
                start: w.start + offset,
                end: w.end.max(w.start) + offset,
            })
        })
        .collect()
}

/// Joins sub-word tokens into words: a token starting with whitespace opens a
/// new word, anything else continues the previous one. Tokens without spaces
/// between them (CJK) therefore stay one unit per token.
fn merge_tokens(tokens: Vec<Word>) -> Vec<Word> {
    let mut words: Vec<Word> = Vec::new();
    for token in tokens {
        let opens = token.text.starts_with(char::is_whitespace);
        match words.last_mut() {
            Some(last) if !opens && !is_cjk_start(&token.text) && !ends_cjk(&last.text) => {
                last.text.push_str(&token.text);
                last.end = token.end.max(last.end);
            }
            _ => words.push(token),
        }
    }
    words.retain(|w| !w.text.trim().is_empty());
    words
}

fn is_cjk(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}')
}

fn is_cjk_start(text: &str) -> bool {
    text.chars().next().is_some_and(is_cjk)
}

fn ends_cjk(text: &str) -> bool {
    text.chars().last().is_some_and(is_cjk)
}

#[cfg(test)]
mod tests {
    use super::{clean_text, merge_tokens, offset_words, EngineEvent, Word};

    #[test]
    fn events_use_camel_case_fields() {
        let done = serde_json::to_value(EngineEvent::JobDone {
            job: "j".into(),
            language: "en".into(),
            elapsed_ms: 1500,
            audio_seconds: 30.0,
        })
        .unwrap();
        assert_eq!(done["kind"], "jobDone");
        assert_eq!(done["elapsedMs"], 1500);
        assert_eq!(done["audioSeconds"], 30.0);
        let stopped = serde_json::to_value(EngineEvent::LiveStopped {
            session: "s".into(),
            duration: 1.0,
            audio_path: Some("/a.wav".into()),
        })
        .unwrap();
        assert_eq!(stopped["audioPath"], "/a.wav");
    }

    fn w(text: &str, start: f64, end: f64) -> Word {
        Word {
            text: text.into(),
            start,
            end,
        }
    }

    #[test]
    fn merges_subword_tokens() {
        let words = merge_tokens(vec![
            w("Hel", 0.0, 0.1),
            w("lo", 0.1, 0.2),
            w(" wor", 0.3, 0.4),
            w("ld", 0.4, 0.5),
            w(",", 0.5, 0.55),
        ]);
        let texts: Vec<_> = words.iter().map(|w| w.text.trim()).collect();
        assert_eq!(texts, ["Hello", "world,"]);
        assert_eq!((words[1].start, words[1].end), (0.3, 0.55));
    }

    #[test]
    fn keeps_cjk_tokens_separate() {
        let words = merge_tokens(vec![
            w("你", 0.0, 0.1),
            w("好", 0.1, 0.2),
            w("AI", 0.2, 0.3),
        ]);
        let texts: Vec<_> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, ["你", "好", "AI"]);
    }

    #[test]
    fn offsets_and_trims_words() {
        let words = offset_words(vec![w(" hi ", 0.5, 0.4), w("  ", 1.0, 1.1)], 10.0);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "hi");
        assert_eq!((words[0].start, words[0].end), (10.5, 10.5));
    }

    #[test]
    fn strips_control_tags() {
        assert_eq!(
            clean_text("<|zh|><|NEUTRAL|><|Speech|>你好 世界"),
            "你好 世界"
        );
        assert_eq!(clean_text("  hello   world \n"), "hello world");
        assert_eq!(clean_text("abc <|unterminated"), "abc");
    }
}
