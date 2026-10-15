//! Safe wrapper over the audio.cpp C ABI (`include/audiocpp.h`).
//!
//! The library is loaded at runtime so the app can start, report a useful error
//! and let the user fix things even when the engine is missing or mismatched.
//! Every handle frees itself on drop; the ABI keeps parents alive internally, so
//! drop order between handles does not matter.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::Arc;

use libloading::Library;

const ABI_MAJOR: u32 = 0;
const MIN_ABI_MINOR: u32 = 2;

pub const STATUS_OK: c_int = 0;
const STATUS_NOT_AVAILABLE: c_int = 7;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("无法加载推理引擎 {path}: {source}")]
    Load {
        path: String,
        #[source]
        source: libloading::Error,
    },
    #[error("推理引擎缺少符号 {0}")]
    Symbol(&'static str),
    #[error("推理引擎 ABI 版本不兼容（需要 {ABI_MAJOR}.{MIN_ABI_MINOR}+，实际 {0}）")]
    Abi(String),
    #[error("{op} 失败（{status}）：{detail}")]
    Call {
        op: &'static str,
        status: String,
        detail: String,
    },
    #[error("参数包含非法的 NUL 字符")]
    Nul,
}

pub type Result<T> = std::result::Result<T, EngineError>;

type FnU32 = unsafe extern "C" fn() -> u32;
type FnStr = unsafe extern "C" fn() -> *const c_char;
type FnStatusStr = unsafe extern "C" fn(c_int) -> *const c_char;
type FnRegistryCreate = unsafe extern "C" fn(*const c_char, *mut *mut c_void) -> c_int;
type FnFree = unsafe extern "C" fn(*mut c_void);
type FnCount = unsafe extern "C" fn(*const c_void) -> usize;
type FnIndexStr = unsafe extern "C" fn(*const c_void, usize, *mut *const c_char) -> c_int;
type FnModelLoad = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const ModelConfig,
    *const c_void,
    *mut *mut c_void,
) -> c_int;
type FnHandleStr = unsafe extern "C" fn(*const c_void) -> *const c_char;
type FnSupports = unsafe extern "C" fn(*const c_void, *const c_char, *const c_char) -> c_int;
type FnHandleInt = unsafe extern "C" fn(*const c_void) -> c_int;
type FnSessionCreate = unsafe extern "C" fn(
    *const c_void,
    *const c_char,
    *const c_char,
    *const BackendConfig,
    *const c_void,
    *mut *mut c_void,
) -> c_int;
type FnSessionRun = unsafe extern "C" fn(*mut c_void, *const c_void, *mut *mut c_void) -> c_int;
type FnCreate = unsafe extern "C" fn() -> *mut c_void;
type FnSetKv = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int;
type FnSetAudio = unsafe extern "C" fn(*mut c_void, *const f32, usize, c_int, c_int) -> c_int;
type FnResultText =
    unsafe extern "C" fn(*const c_void, *mut *const c_char, *mut *const c_char) -> c_int;
type FnResultSegment = unsafe extern "C" fn(
    *const c_void,
    usize,
    *mut i64,
    *mut i64,
    *mut f32,
    *mut *const c_char,
) -> c_int;
type FnResultWord = unsafe extern "C" fn(
    *const c_void,
    usize,
    *mut *const c_char,
    *mut i64,
    *mut i64,
    *mut f32,
) -> c_int;
type FnStreamStart = unsafe extern "C" fn(*mut c_void, *const c_void) -> c_int;
type FnStreamPush = unsafe extern "C" fn(
    *mut c_void,
    *const f32,
    usize,
    c_int,
    c_int,
    i64,
    *mut *mut c_void,
) -> c_int;
type FnStreamFinish = unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> c_int;
type FnModelOption = unsafe extern "C" fn(
    *const c_void,
    c_int,
    usize,
    *mut *const c_char,
    *mut *const c_char,
    *mut *const c_char,
    *mut *const c_char,
    *mut *const c_char,
    *mut *const c_char,
    *mut c_int,
) -> c_int;
type FnModelOptionCount = unsafe extern "C" fn(*const c_void, c_int) -> usize;
type FnEventVad =
    unsafe extern "C" fn(*const c_void, usize, *mut c_int, *mut i64, *mut f32) -> c_int;

#[repr(C)]
struct ModelConfig {
    family_hint: *const c_char,
    config_id: *const c_char,
    weight_id: *const c_char,
    model_spec_override: *const c_char,
}

#[repr(C)]
struct BackendConfig {
    backend: *const c_char,
    device: c_int,
    threads: c_int,
}

struct Api {
    abi_version: FnU32,
    build_version: FnStr,
    last_error: FnStr,
    status_string: FnStatusStr,
    options_create: FnCreate,
    options_set: FnSetKv,
    options_free: FnFree,
    registry_create: FnRegistryCreate,
    registry_free: FnFree,
    model_load: FnModelLoad,
    model_free: FnFree,
    model_family: FnHandleStr,
    model_supports: FnSupports,
    model_supports_timestamps: FnHandleInt,
    model_language_count: FnCount,
    model_language: FnIndexStr,
    model_option_count: FnModelOptionCount,
    model_option: FnModelOption,
    session_create: FnSessionCreate,
    session_free: FnFree,
    session_run: FnSessionRun,
    request_create: FnCreate,
    request_free: FnFree,
    request_set_audio: FnSetAudio,
    request_set_option: FnSetKv,
    result_free: FnFree,
    result_text: FnResultText,
    result_segment_count: FnCount,
    result_segment: FnResultSegment,
    result_word_count: FnCount,
    result_word: FnResultWord,
    stream_start: FnStreamStart,
    stream_push: FnStreamPush,
    stream_finish: FnStreamFinish,
    event_free: FnFree,
    event_voice_activity_count: FnCount,
    event_voice_activity: FnEventVad,
}

/// The loaded engine library. Cheap to clone; every handle keeps it alive.
#[derive(Clone)]
pub struct Engine {
    api: Arc<Api>,
    _lib: Arc<Library>,
}

macro_rules! sym {
    ($lib:expr, $name:literal) => {{
        // SAFETY: the signature matches the declaration in audiocpp.h.
        let symbol = unsafe { $lib.get(concat!($name, "\0").as_bytes()) }
            .map_err(|_| EngineError::Symbol($name))?;
        *symbol
    }};
}

fn cstring(value: &str) -> Result<CString> {
    CString::new(value).map_err(|_| EngineError::Nul)
}

fn path_cstring(path: &Path) -> Result<CString> {
    cstring(&path.to_string_lossy())
}

/// Windows does not search a DLL's own directory for its dependencies, so the
/// Vulkan loader and VC runtime staged next to audiocpp.dll would be missed.
#[cfg(windows)]
unsafe fn open_library(path: &Path) -> std::result::Result<Library, libloading::Error> {
    use libloading::os::windows::{
        Library as WinLibrary, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    };
    WinLibrary::load_with_flags(
        path,
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
    )
    .map(Library::from)
}

#[cfg(not(windows))]
unsafe fn open_library(path: &Path) -> std::result::Result<Library, libloading::Error> {
    Library::new(path)
}

/// Copies a borrowed C string. The ABI never returns NULL, but be defensive.
unsafe fn borrowed(ptr: *const c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

impl Engine {
    pub fn library_file_name() -> &'static str {
        if cfg!(target_os = "windows") {
            "audiocpp.dll"
        } else if cfg!(target_os = "macos") {
            "libaudiocpp.dylib"
        } else {
            "libaudiocpp.so"
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        // SAFETY: loading a trusted library shipped inside the app bundle.
        let lib = unsafe { open_library(path) }.map_err(|source| EngineError::Load {
            path: path.display().to_string(),
            source,
        })?;
        let api = Api {
            abi_version: sym!(lib, "audiocpp_abi_version"),
            build_version: sym!(lib, "audiocpp_build_version"),
            last_error: sym!(lib, "audiocpp_last_error"),
            status_string: sym!(lib, "audiocpp_status_string"),
            options_create: sym!(lib, "audiocpp_options_create"),
            options_set: sym!(lib, "audiocpp_options_set"),
            options_free: sym!(lib, "audiocpp_options_free"),
            registry_create: sym!(lib, "audiocpp_registry_create"),
            registry_free: sym!(lib, "audiocpp_registry_free"),
            model_load: sym!(lib, "audiocpp_model_load"),
            model_free: sym!(lib, "audiocpp_model_free"),
            model_family: sym!(lib, "audiocpp_model_family"),
            model_supports: sym!(lib, "audiocpp_model_supports"),
            model_supports_timestamps: sym!(lib, "audiocpp_model_supports_timestamps"),
            model_language_count: sym!(lib, "audiocpp_model_language_count"),
            model_language: sym!(lib, "audiocpp_model_language"),
            model_option_count: sym!(lib, "audiocpp_model_option_count"),
            model_option: sym!(lib, "audiocpp_model_option"),
            session_create: sym!(lib, "audiocpp_session_create"),
            session_free: sym!(lib, "audiocpp_session_free"),
            session_run: sym!(lib, "audiocpp_session_run"),
            request_create: sym!(lib, "audiocpp_request_create"),
            request_free: sym!(lib, "audiocpp_request_free"),
            request_set_audio: sym!(lib, "audiocpp_request_set_audio"),
            request_set_option: sym!(lib, "audiocpp_request_set_option"),
            result_free: sym!(lib, "audiocpp_result_free"),
            result_text: sym!(lib, "audiocpp_result_text"),
            result_segment_count: sym!(lib, "audiocpp_result_segment_count"),
            result_segment: sym!(lib, "audiocpp_result_segment"),
            result_word_count: sym!(lib, "audiocpp_result_word_count"),
            result_word: sym!(lib, "audiocpp_result_word"),
            stream_start: sym!(lib, "audiocpp_stream_start"),
            stream_push: sym!(lib, "audiocpp_stream_push"),
            stream_finish: sym!(lib, "audiocpp_stream_finish"),
            event_free: sym!(lib, "audiocpp_event_free"),
            event_voice_activity_count: sym!(lib, "audiocpp_event_voice_activity_count"),
            event_voice_activity: sym!(lib, "audiocpp_event_voice_activity"),
        };
        // SAFETY: plain getter with no arguments.
        let version = unsafe { (api.abi_version)() };
        let (major, minor) = (version >> 16, (version >> 8) & 0xff);
        if major != ABI_MAJOR || minor < MIN_ABI_MINOR {
            return Err(EngineError::Abi(format!(
                "{}.{}.{}",
                major,
                minor,
                version & 0xff
            )));
        }
        Ok(Self {
            api: Arc::new(api),
            _lib: Arc::new(lib),
        })
    }

    pub fn build_version(&self) -> String {
        // SAFETY: static string owned by the library.
        unsafe { borrowed((self.api.build_version)()) }
    }

    fn check(&self, op: &'static str, status: c_int) -> Result<()> {
        if status == STATUS_OK {
            return Ok(());
        }
        // SAFETY: both return borrowed, never-NULL strings.
        let (status_name, detail) = unsafe {
            (
                borrowed((self.api.status_string)(status)),
                borrowed((self.api.last_error)()),
            )
        };
        Err(EngineError::Call {
            op,
            status: status_name,
            detail,
        })
    }

    fn options(&self, entries: &[(&str, String)]) -> Result<Options> {
        // SAFETY: returns an owned handle freed by Options::drop.
        let raw = unsafe { (self.api.options_create)() };
        let options = Options {
            engine: self.clone(),
            raw,
        };
        for (key, value) in entries {
            let (k, v) = (cstring(key)?, cstring(value)?);
            // SAFETY: valid handle and NUL-terminated strings; values are copied.
            let status = unsafe { (self.api.options_set)(raw, k.as_ptr(), v.as_ptr()) };
            self.check("options_set", status)?;
        }
        Ok(options)
    }

    pub fn registry(&self) -> Result<Registry> {
        let mut raw = ptr::null_mut();
        // SAFETY: NULL config path selects the default loader set.
        let status = unsafe { (self.api.registry_create)(ptr::null(), &mut raw) };
        self.check("registry_create", status)?;
        Ok(Registry {
            engine: self.clone(),
            raw,
        })
    }
}

struct Options {
    engine: Engine,
    raw: *mut c_void,
}

impl Drop for Options {
    fn drop(&mut self) {
        // SAFETY: handle created by options_create; freeing is idempotent for NULL.
        unsafe { (self.engine.api.options_free)(self.raw) }
    }
}

pub struct Registry {
    engine: Engine,
    raw: *mut c_void,
}

// SAFETY: handles may move between threads; each is only used by one thread at a time.
unsafe impl Send for Registry {}

impl Drop for Registry {
    fn drop(&mut self) {
        // SAFETY: handle created by registry_create.
        unsafe { (self.engine.api.registry_free)(self.raw) }
    }
}

impl Registry {
    pub fn load_model(
        &self,
        path: &Path,
        family: Option<&str>,
        load_options: &[(&str, String)],
    ) -> Result<Model> {
        let engine = &self.engine;
        let path_c = path_cstring(path)?;
        let family_c = family.map(cstring).transpose()?;
        let config = ModelConfig {
            family_hint: family_c.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
            config_id: ptr::null(),
            weight_id: ptr::null(),
            model_spec_override: ptr::null(),
        };
        let options = engine.options(load_options)?;
        let mut raw = ptr::null_mut();
        // SAFETY: all pointers are valid for the call; strings are copied.
        let status = unsafe {
            (engine.api.model_load)(self.raw, path_c.as_ptr(), &config, options.raw, &mut raw)
        };
        engine.check("model_load", status)?;
        Ok(Model {
            engine: engine.clone(),
            raw,
        })
    }
}

pub struct Model {
    engine: Engine,
    raw: *mut c_void,
}

// SAFETY: see Registry.
unsafe impl Send for Model {}

impl Drop for Model {
    fn drop(&mut self) {
        // SAFETY: handle created by model_load.
        unsafe { (self.engine.api.model_free)(self.raw) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Offline,
    Streaming,
}

impl RunMode {
    fn as_str(self) -> &'static str {
        match self {
            RunMode::Offline => "offline",
            RunMode::Streaming => "streaming",
        }
    }
}

impl Model {
    pub fn family(&self) -> String {
        // SAFETY: borrowed string valid while the model lives; copied immediately.
        unsafe { borrowed((self.engine.api.model_family)(self.raw)) }
    }

    pub fn supports(&self, task: &str, mode: RunMode) -> bool {
        let (Ok(task), Ok(mode)) = (cstring(task), cstring(mode.as_str())) else {
            return false;
        };
        // SAFETY: valid handle and strings.
        unsafe { (self.engine.api.model_supports)(self.raw, task.as_ptr(), mode.as_ptr()) == 1 }
    }

    pub fn supports_timestamps(&self) -> bool {
        // SAFETY: valid handle.
        unsafe { (self.engine.api.model_supports_timestamps)(self.raw) == 1 }
    }

    pub fn languages(&self) -> Vec<String> {
        let api = &self.engine.api;
        // SAFETY: valid handle; index bounded by count.
        let count = unsafe { (api.model_language_count)(self.raw) };
        (0..count)
            .filter_map(|i| {
                let mut out = ptr::null();
                let status = unsafe { (api.model_language)(self.raw, i, &mut out) };
                (status == STATUS_OK).then(|| unsafe { borrowed(out) })
            })
            .collect()
    }

    /// Names of the request (`session == false`) or session options the family declares.
    pub fn option_names(&self, session: bool) -> Vec<String> {
        let scope: c_int = if session { 1 } else { 0 };
        let api = &self.engine.api;
        // SAFETY: valid handle; index bounded by count; unused outs are NULL.
        let count = unsafe { (api.model_option_count)(self.raw, scope) };
        (0..count)
            .filter_map(|i| {
                let mut name = ptr::null();
                let status = unsafe {
                    (api.model_option)(
                        self.raw,
                        scope,
                        i,
                        &mut name,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                    )
                };
                (status == STATUS_OK).then(|| unsafe { borrowed(name) })
            })
            .collect()
    }

    pub fn session(
        &self,
        task: &str,
        mode: RunMode,
        backend: &str,
        threads: i32,
        session_options: &[(&str, String)],
    ) -> Result<Session> {
        let engine = &self.engine;
        let task_c = cstring(task)?;
        let mode_c = cstring(mode.as_str())?;
        let backend_c = cstring(backend)?;
        let config = BackendConfig {
            backend: backend_c.as_ptr(),
            device: 0,
            threads,
        };
        let options = engine.options(session_options)?;
        let mut raw = ptr::null_mut();
        // SAFETY: all pointers valid for the duration of the call.
        let status = unsafe {
            (engine.api.session_create)(
                self.raw,
                task_c.as_ptr(),
                mode_c.as_ptr(),
                &config,
                options.raw,
                &mut raw,
            )
        };
        engine.check("session_create", status)?;
        Ok(Session {
            engine: engine.clone(),
            raw,
        })
    }
}

pub struct Session {
    engine: Engine,
    raw: *mut c_void,
}

// SAFETY: see Registry.
unsafe impl Send for Session {}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: handle created by session_create.
        unsafe { (self.engine.api.session_free)(self.raw) }
    }
}

/// Mono audio handed to the engine. Samples are copied by the ABI.
pub struct AudioInput<'a> {
    pub samples: &'a [f32],
    pub sample_rate: u32,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Transcript {
    pub text: String,
    pub language: String,
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceActivity {
    Start(i64),
    End(i64),
}

impl Session {
    fn request(&self, audio: &AudioInput, options: &[(&str, String)]) -> Result<Request> {
        let api = &self.engine.api;
        // SAFETY: returns an owned handle freed by Request::drop.
        let raw = unsafe { (api.request_create)() };
        let request = Request {
            engine: self.engine.clone(),
            raw,
        };
        // SAFETY: the sample slice is valid for the call and copied by the ABI.
        let status = unsafe {
            (api.request_set_audio)(
                raw,
                audio.samples.as_ptr(),
                audio.samples.len(),
                audio.sample_rate as c_int,
                1,
            )
        };
        self.engine.check("request_set_audio", status)?;
        for (key, value) in options {
            let (k, v) = (cstring(key)?, cstring(value)?);
            // SAFETY: valid handle and strings; copied by the ABI.
            let status = unsafe { (api.request_set_option)(raw, k.as_ptr(), v.as_ptr()) };
            self.engine.check("request_set_option", status)?;
        }
        Ok(request)
    }

    pub fn transcribe(
        &mut self,
        audio: &AudioInput,
        options: &[(&str, String)],
    ) -> Result<Transcript> {
        let request = self.request(audio, options)?;
        let mut raw = ptr::null_mut();
        // SAFETY: valid session and request handles.
        let status = unsafe { (self.engine.api.session_run)(self.raw, request.raw, &mut raw) };
        self.engine.check("session_run", status)?;
        let result = ResultHandle {
            engine: self.engine.clone(),
            raw,
        };
        Ok(result.transcript(audio.sample_rate))
    }

    pub fn stream_start(&mut self) -> Result<()> {
        // SAFETY: NULL request is allowed.
        let status = unsafe { (self.engine.api.stream_start)(self.raw, ptr::null()) };
        self.engine.check("stream_start", status)
    }

    /// Pushes one chunk to a streaming VAD session and returns the voice
    /// activity transitions it produced.
    pub fn push_vad(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        start_sample: i64,
    ) -> Result<Vec<VoiceActivity>> {
        let api = &self.engine.api;
        let mut event = ptr::null_mut();
        // SAFETY: slice valid for the call; event ownership passes to us.
        let status = unsafe {
            (api.stream_push)(
                self.raw,
                samples.as_ptr(),
                samples.len(),
                sample_rate as c_int,
                1,
                start_sample,
                &mut event,
            )
        };
        self.engine.check("stream_push", status)?;
        if event.is_null() {
            return Ok(Vec::new());
        }
        let event = EventHandle {
            engine: self.engine.clone(),
            raw: event,
        };
        // SAFETY: valid event handle; index bounded by count.
        let count = unsafe { (api.event_voice_activity_count)(event.raw) };
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let (mut kind, mut sample, mut prob) = (0, 0i64, 0f32);
            let status = unsafe {
                (api.event_voice_activity)(event.raw, i, &mut kind, &mut sample, &mut prob)
            };
            if status != STATUS_OK {
                continue;
            }
            match kind {
                0 => out.push(VoiceActivity::Start(sample)),
                1 => out.push(VoiceActivity::End(sample)),
                _ => {}
            }
        }
        Ok(out)
    }

    pub fn stream_finish(&mut self) -> Result<()> {
        let mut raw = ptr::null_mut();
        // SAFETY: valid handle; the result is freed immediately.
        let status = unsafe { (self.engine.api.stream_finish)(self.raw, &mut raw) };
        drop(ResultHandle {
            engine: self.engine.clone(),
            raw,
        });
        self.engine.check("stream_finish", status)
    }
}

struct Request {
    engine: Engine,
    raw: *mut c_void,
}

impl Drop for Request {
    fn drop(&mut self) {
        // SAFETY: handle created by request_create.
        unsafe { (self.engine.api.request_free)(self.raw) }
    }
}

struct ResultHandle {
    engine: Engine,
    raw: *mut c_void,
}

impl Drop for ResultHandle {
    fn drop(&mut self) {
        // SAFETY: handle from session_run / stream_finish, or NULL.
        unsafe { (self.engine.api.result_free)(self.raw) }
    }
}

impl ResultHandle {
    fn transcript(&self, sample_rate: u32) -> Transcript {
        let api = &self.engine.api;
        let rate = f64::from(sample_rate.max(1));
        let mut transcript = Transcript::default();
        let (mut text, mut language) = (ptr::null(), ptr::null());
        // SAFETY: valid result handle; strings copied before the handle drops.
        let status = unsafe { (api.result_text)(self.raw, &mut text, &mut language) };
        if status == STATUS_OK {
            unsafe {
                transcript.text = borrowed(text);
                transcript.language = borrowed(language);
            }
        } else if status != STATUS_NOT_AVAILABLE {
            log::warn!("result_text returned status {status}");
        }
        // SAFETY: indices bounded by the reported counts.
        let words = unsafe { (api.result_word_count)(self.raw) };
        for i in 0..words {
            let (mut word, mut start, mut end, mut conf) = (ptr::null(), 0i64, 0i64, 0f32);
            let status = unsafe {
                (api.result_word)(self.raw, i, &mut word, &mut start, &mut end, &mut conf)
            };
            if status == STATUS_OK {
                transcript.words.push(Word {
                    text: unsafe { borrowed(word) },
                    start: start as f64 / rate,
                    end: end as f64 / rate,
                });
            }
        }
        if transcript.text.is_empty() {
            // Some families only fill segments; join them as a fallback.
            let segments = unsafe { (api.result_segment_count)(self.raw) };
            let mut parts = Vec::new();
            for i in 0..segments {
                let mut seg_text = ptr::null();
                let status = unsafe {
                    (api.result_segment)(
                        self.raw,
                        i,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        ptr::null_mut(),
                        &mut seg_text,
                    )
                };
                if status == STATUS_OK {
                    let t = unsafe { borrowed(seg_text) };
                    if !t.trim().is_empty() {
                        parts.push(t);
                    }
                }
            }
            transcript.text = parts.join(" ");
        }
        transcript
    }
}

struct EventHandle {
    engine: Engine,
    raw: *mut c_void,
}

impl Drop for EventHandle {
    fn drop(&mut self) {
        // SAFETY: handle from stream_push.
        unsafe { (self.engine.api.event_free)(self.raw) }
    }
}

/// Locates the engine library and its bundled assets.
#[derive(Debug, Clone)]
pub struct EngineLayout {
    pub library: PathBuf,
    pub silero_vad: PathBuf,
}

impl EngineLayout {
    pub fn from_root(root: &Path) -> Self {
        Self {
            library: root.join("lib").join(Engine::library_file_name()),
            silero_vad: root.join("assets").join("silero_vad"),
        }
    }
}
