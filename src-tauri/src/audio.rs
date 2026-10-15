//! Audio I/O: file decoding, mono 16 kHz conversion and microphone capture.

use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam_channel::Sender;

pub const TARGET_RATE: u32 = 16_000;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("无法打开音频文件：{0}")]
    Open(#[from] std::io::Error),
    #[error("不支持的音频格式：{0}")]
    Unsupported(String),
    #[error("音频解码失败：{0}")]
    Decode(String),
    #[error("文件中没有音频数据")]
    Empty,
    #[error("麦克风不可用：{0}")]
    Device(String),
}

/// Decodes any container/codec symphonia supports into mono f32 at `TARGET_RATE`.
/// `on_progress` receives a 0..1 fraction when the track length is known.
pub fn decode_file_mono_16k(
    path: &Path,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(f32),
) -> Result<Vec<f32>, AudioError> {
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::errors::Error;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| AudioError::Unsupported(e.to_string()))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| AudioError::Unsupported("没有音轨".into()))?;
    let track_id = track.id;
    let total_frames = track.num_frames;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| AudioError::Unsupported("无法识别的编码参数".into()))?
        .clone();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| AudioError::Unsupported(e.to_string()))?;

    let mut mono: Vec<f32> = Vec::new();
    let mut out: Vec<f32> = Vec::new();
    let mut resampler: Option<StreamResampler> = None;
    let mut source_rate = params.sample_rate.unwrap_or(0);
    let mut interleaved: Vec<f32> = Vec::new();
    let mut decoded_frames: u64 = 0;
    let mut last_report = 0f32;

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(AudioError::Decode("已取消".into()));
        }
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(Error::ResetRequired) => break,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(AudioError::Decode(e.to_string())),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buffer = match decoder.decode(&packet) {
            Ok(buffer) => buffer,
            Err(Error::DecodeError(e)) => {
                log::debug!("skipping corrupt packet: {e}");
                continue;
            }
            Err(e) => return Err(AudioError::Decode(e.to_string())),
        };
        let spec = buffer.spec();
        source_rate = spec.rate();
        let channels = spec.channels().count().max(1);
        interleaved.resize(buffer.samples_interleaved(), 0.0);
        buffer.copy_to_slice_interleaved(&mut interleaved);
        mono.clear();
        downmix_into(&interleaved, channels, &mut mono);
        // Resample per packet so long files stay cancellable and memory stays flat.
        out.extend(
            resampler
                .get_or_insert_with(|| StreamResampler::new(source_rate, TARGET_RATE))
                .process(&mono),
        );
        decoded_frames += (interleaved.len() / channels) as u64;
        if let Some(total) = total_frames.filter(|t| *t > 0) {
            let fraction = (decoded_frames as f32 / total as f32).min(1.0);
            if fraction - last_report >= 0.01 {
                last_report = fraction;
                on_progress(fraction);
            }
        }
    }

    if let Some(rs) = resampler.as_mut() {
        out.extend(rs.finish());
    }
    if out.is_empty() || source_rate == 0 {
        return Err(AudioError::Empty);
    }
    on_progress(1.0);
    Ok(out)
}

fn downmix_into(interleaved: &[f32], channels: usize, out: &mut Vec<f32>) {
    if channels == 1 {
        out.extend_from_slice(interleaved);
        return;
    }
    let scale = 1.0 / channels as f32;
    out.extend(
        interleaved
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() * scale),
    );
}

/// Batch windowed-sinc resampler; the reference `StreamResampler` is tested against.
#[cfg(test)]
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    const HALF_TAPS: i64 = 16;
    let ratio = f64::from(to) / f64::from(from);
    // Low-pass at the lower Nyquist so downsampling does not alias.
    let cutoff = ratio.min(1.0) * 0.95;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    let n = input.len() as i64;
    for i in 0..out_len {
        let center = i as f64 / ratio;
        let base = center.floor() as i64;
        let mut acc = 0.0f64;
        let mut norm = 0.0f64;
        let span = (HALF_TAPS as f64 / cutoff).ceil() as i64;
        for k in (base - span + 1)..=(base + span) {
            if k < 0 || k >= n {
                continue;
            }
            let x = (k as f64 - center) * cutoff;
            let sinc = if x.abs() < 1e-9 {
                1.0
            } else {
                (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
            };
            // Blackman window over the kernel support.
            let t = (k as f64 - center) / (span as f64);
            if t.abs() > 1.0 {
                continue;
            }
            let w = 0.42
                + 0.5 * (std::f64::consts::PI * t).cos()
                + 0.08 * (2.0 * std::f64::consts::PI * t).cos();
            let weight = sinc * w;
            acc += f64::from(input[k as usize]) * weight;
            norm += weight;
        }
        out.push(if norm.abs() > 1e-12 {
            (acc / norm) as f32
        } else {
            0.0
        });
    }
    out
}

/// Streaming resampler for live capture: keeps enough history between blocks so
/// that the sinc kernel sees continuous signal across block boundaries.
pub struct StreamResampler {
    from: u32,
    to: u32,
    history: Vec<f32>,
    /// Absolute input position (in input samples) of the next output sample.
    next_pos: f64,
    consumed: u64,
}

impl StreamResampler {
    pub fn new(from: u32, to: u32) -> Self {
        Self {
            from,
            to,
            history: Vec::new(),
            next_pos: 0.0,
            consumed: 0,
        }
    }

    pub fn process(&mut self, block: &[f32]) -> Vec<f32> {
        if self.from == self.to {
            return block.to_vec();
        }
        self.history.extend_from_slice(block);
        let ratio = f64::from(self.to) / f64::from(self.from);
        let cutoff = ratio.min(1.0) * 0.95;
        let span = (16.0 / cutoff).ceil() as i64;
        let step = 1.0 / ratio;
        let available_end = self.consumed as f64 + self.history.len() as f64;
        let mut out = Vec::new();
        while self.next_pos + span as f64 + 1.0 < available_end {
            let center = self.next_pos - self.consumed as f64;
            let base = center.floor() as i64;
            let (mut acc, mut norm) = (0.0f64, 0.0f64);
            for k in (base - span + 1)..=(base + span) {
                if k < 0 || k as usize >= self.history.len() {
                    continue;
                }
                let d = k as f64 - center;
                let t = d / span as f64;
                if t.abs() > 1.0 {
                    continue;
                }
                let x = d * cutoff;
                let sinc = if x.abs() < 1e-9 {
                    1.0
                } else {
                    (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
                };
                let w = 0.42
                    + 0.5 * (std::f64::consts::PI * t).cos()
                    + 0.08 * (2.0 * std::f64::consts::PI * t).cos();
                acc += f64::from(self.history[k as usize]) * sinc * w;
                norm += sinc * w;
            }
            out.push(if norm.abs() > 1e-12 {
                (acc / norm) as f32
            } else {
                0.0
            });
            self.next_pos += step;
        }
        self.trim_history();
        out
    }

    /// Flushes the samples held back for the kernel's right half. The output
    /// then covers the whole input, as the batch `resample` does.
    pub fn finish(&mut self) -> Vec<f32> {
        if self.from == self.to {
            return Vec::new();
        }
        let total_in = self.consumed as usize + self.history.len();
        let target =
            ((total_in as f64) * f64::from(self.to) / f64::from(self.from)).round() as usize;
        let produced = (self.next_pos * f64::from(self.to) / f64::from(self.from)).round() as usize;
        let pad =
            vec![0.0f32; (64.0 * f64::from(self.from) / f64::from(self.to)).ceil() as usize + 64];
        let mut tail = self.process(&pad);
        tail.truncate(target.saturating_sub(produced));
        tail
    }

    fn trim_history(&mut self) {
        let ratio = f64::from(self.to) / f64::from(self.from);
        let span = (16.0 / (ratio.min(1.0) * 0.95)).ceil() as i64;
        // Drop history that no future kernel can reach.
        let keep_from = (self.next_pos.floor() as i64 - span - 1).max(self.consumed as i64) as u64;
        let drop = (keep_from - self.consumed) as usize;
        if drop > 0 && drop <= self.history.len() {
            self.history.drain(..drop);
            self.consumed += drop as u64;
        }
    }
}

pub fn write_wav_16k(path: &Path, samples: &[f32]) -> Result<(), AudioError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| AudioError::Decode(e.to_string()))?;
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        writer
            .write_sample(v)
            .map_err(|e| AudioError::Decode(e.to_string()))?;
    }
    writer
        .finalize()
        .map_err(|e| AudioError::Decode(e.to_string()))
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub fn list_input_devices() -> Vec<InputDevice> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let default_id = host
        .default_input_device()
        .and_then(|d| d.id().ok())
        .map(|id| id.to_string());
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| {
            let id = device.id().ok()?.to_string();
            let name = device
                .description()
                .map(|d| d.name().to_string())
                .unwrap_or_else(|_| device.to_string());
            Some(InputDevice {
                is_default: default_id.as_deref() == Some(id.as_str()),
                id,
                name,
            })
        })
        .collect()
}

/// Handle to a running capture stream. Capture stops when this is dropped.
pub struct Capture {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Starts capturing from `device_id` (or the system default). Mono blocks at
/// the device's native rate are sent through `tx` together with that rate.
/// cpal streams are not `Send` on every platform, so the stream lives on its own
/// thread for its whole lifetime.
pub fn start_capture(
    device_id: Option<String>,
    tx: Sender<(u32, Vec<f32>)>,
) -> Result<Capture, AudioError> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = crossbeam_channel::bounded::<Result<(), AudioError>>(1);
    let stop_flag = stop.clone();
    let thread = std::thread::Builder::new()
        .name("murmur-capture".into())
        .spawn(move || {
            let stream = match open_stream(device_id, tx) {
                Ok(stream) => stream,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));
            while !stop_flag.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            drop(stream);
        })
        .map_err(|e| AudioError::Device(e.to_string()))?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(Capture {
            stop,
            thread: Some(thread),
        }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => Err(AudioError::Device("采集线程意外退出".into())),
    }
}

fn open_stream(
    device_id: Option<String>,
    tx: Sender<(u32, Vec<f32>)>,
) -> Result<cpal::Stream, AudioError> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::SampleFormat;

    let host = cpal::default_host();
    let device = match device_id {
        Some(id) => {
            let parsed: cpal::DeviceId = id
                .parse()
                .map_err(|_| AudioError::Device(format!("无效的设备 ID：{id}")))?;
            host.device_by_id(&parsed)
                .ok_or_else(|| AudioError::Device("所选麦克风已断开".into()))?
        }
        None => host
            .default_input_device()
            .ok_or_else(|| AudioError::Device("未找到麦克风".into()))?,
    };
    let supported = device
        .default_input_config()
        .map_err(|e| AudioError::Device(e.to_string()))?;
    let rate = supported.sample_rate();
    let channels = usize::from(supported.channels()).max(1);
    let config = supported.config();
    let err_fn = |err: cpal::Error| log::warn!("capture stream error: {err}");

    macro_rules! build {
        ($t:ty, $conv:expr) => {{
            let tx = tx.clone();
            device.build_input_stream::<$t, _, _>(
                config,
                move |data: &[$t], _| {
                    let mut mono = Vec::with_capacity(data.len() / channels);
                    for frame in data.chunks_exact(channels) {
                        let sum: f32 = frame.iter().map(|&s| $conv(s)).sum();
                        mono.push(sum / channels as f32);
                    }
                    let _ = tx.try_send((rate, mono));
                },
                err_fn,
                None,
            )
        }};
    }

    let stream = match supported.sample_format() {
        SampleFormat::F32 => build!(f32, |s: f32| s),
        SampleFormat::I16 => build!(i16, |s: i16| f32::from(s) / 32768.0),
        SampleFormat::I32 => build!(i32, |s: i32| s as f32 / 2_147_483_648.0),
        SampleFormat::U16 => build!(u16, |s: u16| (f32::from(s) - 32768.0) / 32768.0),
        SampleFormat::I8 => build!(i8, |s: i8| f32::from(s) / 128.0),
        other => {
            return Err(AudioError::Device(format!("不支持的采样格式 {other}")));
        }
    }
    .map_err(|e| AudioError::Device(e.to_string()))?;
    stream
        .play()
        .map_err(|e| AudioError::Device(e.to_string()))?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, secs: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    fn dominant_amplitude(signal: &[f32], freq: f32, rate: u32) -> f32 {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &s) in signal.iter().enumerate() {
            let phase = 2.0 * std::f64::consts::PI * f64::from(freq) * i as f64 / f64::from(rate);
            re += f64::from(s) * phase.cos();
            im += f64::from(s) * phase.sin();
        }
        (2.0 * (re * re + im * im).sqrt() / signal.len() as f64) as f32
    }

    #[test]
    fn resample_preserves_length_and_passband() {
        let input = sine(440.0, 48_000, 1.0);
        let out = resample(&input, 48_000, 16_000);
        assert_eq!(out.len(), 16_000);
        let amp = dominant_amplitude(&out[1000..15_000], 440.0, 16_000);
        assert!((amp - 1.0).abs() < 0.05, "amplitude {amp}");
    }

    #[test]
    fn resample_rejects_above_nyquist() {
        let input = sine(12_000.0, 48_000, 1.0);
        let out = resample(&input, 48_000, 16_000);
        let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
        assert!(rms < 0.05, "aliased energy {rms}");
    }

    #[test]
    fn streaming_resampler_matches_batch() {
        let input = sine(300.0, 44_100, 2.0);
        let batch = resample(&input, 44_100, 16_000);
        let mut stream = StreamResampler::new(44_100, 16_000);
        let mut out = Vec::new();
        for block in input.chunks(441) {
            out.extend(stream.process(block));
        }
        out.extend(stream.finish());
        assert_eq!(out.len(), batch.len());
        for i in 100..out.len() - 100 {
            assert!((out[i] - batch[i]).abs() < 1e-3, "mismatch at {i}");
        }
    }

    #[test]
    fn downmix_averages_channels() {
        let mut out = Vec::new();
        downmix_into(&[1.0, 0.0, 0.5, 0.5], 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5]);
    }
}
