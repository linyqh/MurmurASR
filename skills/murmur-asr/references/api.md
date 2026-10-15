# Murmur Local API Reference

Base URL: `http://127.0.0.1:7895` on the Murmur machine, or `http://<LAN-IP>:7895` from other devices when 允许局域网访问 is on (both shown on Murmur's **API 接口** page, where the port can be changed). The running app writes its addresses to `api.json` in its data directory:

| OS | Discovery file |
|---|---|
| macOS | `~/Library/Application Support/app.murmur.asr/api.json` |
| Windows | `%APPDATA%\app.murmur.asr\api.json` |
| Linux | `~/.local/share/app.murmur.asr/api.json` |

```json
{ "baseUrl": "http://127.0.0.1:7895", "lanUrl": "http://192.168.1.23:7895", "port": 7895, "pid": 12345, "version": "0.1.10" }
```

`lanUrl` is `null` when LAN access is off. The file is removed when the API is turned off or the app quits; it may be stale after a crash, so always confirm with `GET /health`.

Access rules:

- The server listens on loopback, or on all interfaces when LAN access is on. There is no authentication.
- Requests with an `Origin` header (browsers) or a `Host` that is a domain name other than `localhost` / `*.local` (DNS rebinding) get `403`.
- Callers on other machines get `403` for `path` in `/transcribe` and for `GET /transcripts`; they must upload files and fetch results by id.

Errors are JSON: `{ "error": "message" }`.

## `GET /health`

```json
{
  "ok": true,
  "app": "murmur",
  "version": "0.1.10",
  "engine": { "version": "0.9.1", "error": null },
  "model": {
    "id": "qwen3-asr-0.6b-q8",
    "family": "qwen3_asr",
    "languages": ["Auto", "Chinese", "English", "Cantonese", "..."],
    "acceptsLanguage": true,
    "acceptsItn": false,
    "wordTimestamps": true,
    "backend": "metal",
    "loadMs": 1180
  },
  "loading": null,
  "installedModels": ["qwen3-asr-0.6b-q8"],
  "defaults": { "language": "auto", "itn": true },
  "formats": ["json", "txt", "srt", "vtt", "md"]
}
```

`ok` is true only when the engine works and a model is loaded. `language` is ignored unless `model.acceptsLanguage`; `itn` is ignored unless `model.acceptsItn`.

## `GET /models`

Lists the ASR catalog with `installed` and `loaded` flags plus the `recommended` id. Models must be downloaded from inside the app.

## `POST /models/load`

Body `{ "id": "sensevoice-small-q8" }`. Blocks until loaded (up to 3 minutes), returns `{ "model": {...} }`. `400` if unknown or not downloaded, `500` with the engine error if loading fails.

## `POST /transcribe`

Send **either** JSON with a local `path`, **or** `multipart/form-data` with one `file` field. Other fields are the same in both forms:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `path` | string | — | Absolute path on the app's machine (JSON or form field) |
| `file` | file | — | Uploaded audio/video (multipart only) |
| `language` | string | app setting | `auto` or a value copied from `model.languages` (model-specific, e.g. `Chinese` for Qwen3-ASR) |
| `itn` | bool | app setting | Normalize numbers/punctuation (SenseVoice, Fun-ASR) |
| `model` | string | loaded model | Catalog id; loaded first if different |
| `title` | string | file name | Title in Murmur's history |
| `format` | string | `json` | `json`, `txt`, `srt`, `vtt`, `md` |
| `wait` | bool | `true` | Block until done; `false` returns `202` with a job id |

Supported inputs: WAV, MP3, M4A/AAC, FLAC, OGG/Opus, WebM, MP4, MOV, MKV, CAF, AIFF.

With `wait=true` and `format=json`, the response is the saved transcript:

```json
{
  "id": "0b6f…",
  "status": "done",
  "title": "meeting",
  "source": "file",
  "createdAt": 1760550000000,
  "duration": 421.3,
  "language": "Chinese",
  "model": "Qwen3-ASR 0.6B",
  "audioPath": "/abs/path/meeting.m4a",
  "elapsedMs": 20510,
  "text": "full plain text …",
  "segments": [
    {
      "index": 0, "start": 0.48, "end": 6.2, "text": "大家好，今天我们聊一聊…",
      "words": [{ "text": "大", "start": 0.48, "end": 0.6 }]
    }
  ]
}
```

`words` is present only when the model produces word timestamps (`model.wordTimestamps`). Other formats return `text/plain` with an `X-Murmur-Transcript-Id` header.

JSON responses also include `subtitles`, derived from the original `segments` on read. Each subtitle has `index`, `start`, `end`, `text` (including explicit line breaks), `words`, `timing` (`word` or `segment`), and `issues`. SRT and VTT export these same subtitle cues; TXT and Markdown continue to use the original transcript. Existing records are reflowed without another ASR run.

Subtitle layout follows the app's global subtitle line preference: one line by default, or at most two lines. Change it in Settings or the transcript preview. JSON includes `subtitleMaxLines` (`1` or `2`) to identify the preference used for its derived cues. Changing this preference reflows existing records and subsequent API/export results without rewriting the original recognition data.

`issues` can include `segment_timing` (missing or unreliable word alignment; original segment time retained), `reading_speed`, `line_length`, `short_duration`, or `long_duration`. These are review hints, not transcription failures. No text is removed to meet readability targets, and missing word timings are not fabricated.

With `wait=false`:

```json
{ "id": "0b6f…", "status": "queued", "statusUrl": "/jobs/0b6f…" }
```

Errors: `400` bad input or missing file, `409` no model loaded / job cancelled, `422` transcription failed (message from the engine), `503` engine unavailable.

## `GET /jobs/{id}`

```json
{ "id": "0b6f…", "status": "transcribing", "progress": 0.42, "duration": 421.3 }
```

`status` ∈ `queued`, `decoding`, `detecting`, `transcribing`, `done`, `failed`, `cancelled`. `progress` is 0–1 within the current phase. When `done`, the object includes `result` (same shape as the JSON transcript above). When `failed`, it includes `error`.

## `POST /jobs/{id}/cancel`

Returns `{ "id": "…", "cancelling": true }` if the job was still running.

## `GET /transcripts?limit=20`

Same machine only. Recent history (file and live), newest first: `id`, `title`, `source`, `createdAt`, `duration`, `preview`.

## `GET /transcripts/{id}?format=srt`

Any saved transcript in any export format.
