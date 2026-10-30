//! Fallback decoder for media symphonia cannot handle (HE-AAC, AC-3, WMA, …):
//! a pinned static ffmpeg build, downloaded and verified on first use.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::audio::{AudioError, TARGET_RATE};
use crate::models::{self, DownloadEvent, DownloadSource, FetchError};

/// Release tag of https://github.com/eugeneware/ffmpeg-static (mirrored by npmmirror).
const RELEASE: &str = "b6.1.1";

struct Asset {
    name: &'static str,
    size: u64,
    sha256: &'static str,
}

const ASSET: Option<Asset> = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    Some(Asset {
        name: "ffmpeg-darwin-arm64.gz",
        size: 19_246_198,
        sha256: "8923876afa8db5585022d7860ec7e589af192f441c56793971276d450ed3bbfa",
    })
} else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
    Some(Asset {
        name: "ffmpeg-darwin-x64.gz",
        size: 25_296_431,
        sha256: "929b375c1182d956c51f7ac25e0b2b0411fb01f6f407aa15c9758efeb4242106",
    })
} else if cfg!(target_os = "windows") {
    // Windows on ARM runs the x64 build under emulation.
    Some(Asset {
        name: "ffmpeg-win32-x64.gz",
        size: 29_581_307,
        sha256: "8883a3dffbd0a16cf4ef95206ea05283f78908dbfb118f73c83f4951dcc06d77",
    })
} else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some(Asset {
        name: "ffmpeg-linux-x64.gz",
        size: 29_354_986,
        sha256: "bfe8a8fc511530457b528c48d77b5737527b504a3797a9bc4866aeca69c2dffa",
    })
} else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
    Some(Asset {
        name: "ffmpeg-linux-arm64.gz",
        size: 25_568_691,
        sha256: "754a678672298bc68156adff58aa7385a592c2b30b1d0ae8750c45c915c4bac0",
    })
} else {
    None
};

#[derive(Debug, Clone)]
pub struct Ffmpeg {
    dir: PathBuf,
    source: DownloadSource,
}

impl Ffmpeg {
    pub fn new(tools_dir: &Path, source: DownloadSource) -> Self {
        Self {
            dir: tools_dir.join(format!("ffmpeg-{RELEASE}")),
            source,
        }
    }

    fn binary(&self) -> PathBuf {
        self.dir
            .join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" })
    }

    /// Returns the ffmpeg binary, downloading it first when missing.
    /// `on_progress` receives the download fraction.
    pub fn ensure(
        &self,
        cancel: &AtomicBool,
        on_progress: impl Fn(f32),
    ) -> Result<PathBuf, AudioError> {
        let bin = self.binary();
        if bin.is_file() {
            return Ok(bin);
        }
        let asset = ASSET
            .as_ref()
            .ok_or_else(|| AudioError::Decoder("当前平台没有可用的 ffmpeg 构建".into()))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| AudioError::Decoder(e.to_string()))?;
        runtime.block_on(self.install(asset, &bin, cancel, &on_progress))?;
        Ok(bin)
    }

    async fn install(
        &self,
        asset: &Asset,
        bin: &Path,
        cancel: &AtomicBool,
        on_progress: &impl Fn(f32),
    ) -> Result<(), AudioError> {
        let failed = |e: String| AudioError::Decoder(e);
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|e| failed(e.to_string()))?;
        let client = models::http_client().map_err(|e| failed(e.to_string()))?;
        let partial = self.dir.join(format!("{}.part", asset.name));
        let emit = |event: DownloadEvent| {
            if let DownloadEvent::Progress {
                downloaded, total, ..
            } = event
            {
                on_progress(downloaded as f32 / total as f32);
            }
        };
        let mut errors = Vec::new();
        for (label, url) in source_urls(asset.name, self.source) {
            match models::fetch(
                &client, &url, label, &partial, "ffmpeg", asset.size, 0, asset.size, cancel, &emit,
            )
            .await
            {
                Ok(()) => {
                    errors.clear();
                    break;
                }
                Err(FetchError::Cancelled) => return Err(AudioError::Decode("已取消".into())),
                Err(FetchError::Other(e)) => {
                    log::warn!("ffmpeg download from {label} failed: {e}");
                    errors.push(format!("{label}：{e}"));
                }
            }
        }
        if !errors.is_empty() {
            return Err(failed(errors.join("；")));
        }
        match models::sha256_file(&partial, cancel).await {
            Ok(digest) if digest == asset.sha256 => {}
            Ok(_) => {
                let _ = tokio::fs::remove_file(&partial).await;
                return Err(failed("文件校验失败（SHA-256 不匹配），请重试".into()));
            }
            Err(FetchError::Cancelled) => return Err(AudioError::Decode("已取消".into())),
            Err(FetchError::Other(e)) => return Err(failed(e)),
        }
        unpack(&partial, bin).map_err(|e| failed(e.to_string()))?;
        let _ = tokio::fs::remove_file(&partial).await;
        log::info!("installed ffmpeg {RELEASE} at {}", bin.display());
        Ok(())
    }
}

fn source_urls(asset: &str, source: DownloadSource) -> [(&'static str, String); 2] {
    let github = (
        "GitHub",
        format!("https://github.com/eugeneware/ffmpeg-static/releases/download/{RELEASE}/{asset}"),
    );
    let npmmirror = (
        "npmmirror",
        format!("https://cdn.npmmirror.com/binaries/ffmpeg-static/{RELEASE}/{asset}"),
    );
    match source {
        DownloadSource::Auto | DownloadSource::Huggingface => [github, npmmirror],
        DownloadSource::Hfmirror | DownloadSource::Modelscope => [npmmirror, github],
    }
}

fn unpack(gz: &Path, bin: &Path) -> std::io::Result<()> {
    let tmp = bin.with_extension("tmp");
    {
        let mut input = flate2::read::GzDecoder::new(BufReader::new(File::open(gz)?));
        let mut output = File::create(&tmp)?;
        std::io::copy(&mut input, &mut output)?;
        output.sync_all()?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&tmp, bin)
}

/// Decodes the first audio stream of `path` to mono f32 at `TARGET_RATE`.
pub fn decode(
    bin: &Path,
    path: &Path,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(f32),
) -> Result<Vec<f32>, AudioError> {
    let mut command = Command::new(bin);
    command
        .args(["-nostdin", "-hide_banner", "-nostats", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-vn", "-sn", "-dn", "-ac", "1", "-ar"])
        .arg(TARGET_RATE.to_string())
        .args(["-f", "f32le", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|e| AudioError::Decoder(format!("无法启动 ffmpeg：{e}")))?;

    let duration_ms = Arc::new(AtomicU64::new(0));
    let stderr = child.stderr.take().expect("stderr is piped");
    let seen = duration_ms.clone();
    // Drained on its own thread so a chatty ffmpeg can never block on a full pipe.
    let log = std::thread::spawn(move || {
        let mut tail = VecDeque::new();
        for line in BufReader::new(stderr).split(b'\n').map_while(Result::ok) {
            let line = String::from_utf8_lossy(&line).trim().to_string();
            if let Some(ms) = parse_duration(&line) {
                let _ = seen.compare_exchange(0, ms, Ordering::Relaxed, Ordering::Relaxed);
            }
            if tail.len() == 4 {
                tail.pop_front();
            }
            tail.push_back(line);
        }
        tail
    });

    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut buf = vec![0u8; 64 << 10];
    let mut carry = 0usize;
    let mut out: Vec<f32> = Vec::new();
    let mut last_report = 0f32;
    let samples_per_ms = u64::from(TARGET_RATE) / 1000;
    let result = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            break Err(AudioError::Decode("已取消".into()));
        }
        let n = match stdout.read(&mut buf[carry..]) {
            Ok(0) => break Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                let _ = child.kill();
                break Err(AudioError::Decode(e.to_string()));
            }
        };
        let filled = carry + n;
        let whole = filled - filled % 4;
        out.extend(
            buf[..whole]
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        );
        buf.copy_within(whole..filled, 0);
        carry = filled - whole;
        let total = duration_ms.load(Ordering::Relaxed) * samples_per_ms;
        if total > 0 {
            let fraction = (out.len() as f32 / total as f32).min(1.0);
            if fraction - last_report >= 0.01 {
                last_report = fraction;
                on_progress(fraction);
            }
        }
    };
    drop(stdout);
    let status = child.wait();
    let tail = log.join().unwrap_or_default();
    result?;
    let status = status.map_err(|e| AudioError::Decode(e.to_string()))?;
    if !status.success() {
        let reason = tail
            .iter()
            .rev()
            .find(|l| !l.is_empty())
            .cloned()
            .unwrap_or_else(|| status.to_string());
        return Err(AudioError::Decode(format!("ffmpeg：{reason}")));
    }
    if out.is_empty() {
        return Err(AudioError::Empty);
    }
    on_progress(1.0);
    Ok(out)
}

/// Parses ffmpeg's `Duration: HH:MM:SS.ss, …` input banner line into milliseconds.
fn parse_duration(line: &str) -> Option<u64> {
    let stamp = line.strip_prefix("Duration: ")?.split(',').next()?;
    let mut parts = stamp.split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    Some(((hours * 3600.0 + minutes * 60.0 + seconds) * 1000.0) as u64)
}
