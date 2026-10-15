---
name: murmur-asr
description: Use when an agent needs to transcribe an audio or video file (speech-to-text / ASR), produce subtitles (SRT/VTT), or get word-level timestamps through the Murmur desktop app's HTTP API, on this machine or another machine on the LAN. Inference runs on the Murmur machine, never in the cloud. Do not use for live microphone dictation, model training, or cloud ASR services.
metadata:
  source_project: Murmur
---

# Murmur ASR

Murmur is a local speech-to-text desktop app (audio.cpp engine). While it is running it exposes an HTTP API (default port 7895) on this machine and, when LAN access is on, to other devices on the same network. Results also appear in the app's history.

## Required flow

1. **Find the service.**
   - Murmur on this machine: read `baseUrl` from `~/Library/Application Support/app.murmur.asr/api.json` (macOS), `%APPDATA%\app.murmur.asr\api.json` (Windows) or `~/.local/share/app.murmur.asr/api.json` (Linux); fall back to `http://127.0.0.1:7895`.
   - Murmur on another machine: use the 局域网 address the user copies from Murmur's **API 接口** page (e.g. `http://192.168.1.23:7895`), or `$MURMUR_API_URL` if set.
2. **Check health:** `GET /health`. If the connection is refused, ask the user to open Murmur and check the **API 接口** page (sidebar): 启用 API 服务 must be on, plus 允许局域网访问 for remote calls. Do not install or start a different ASR tool.
3. **Ensure a model.** If `model` is `null` in `/health`, pick an id from `installedModels` and pass it as `model` (or `POST /models/load`). If nothing is installed, ask the user to download a model in Murmur's 模型 page — the API cannot download models.
4. **Transcribe** with `POST /transcribe` and return the text or the requested subtitle file.

The bundled helper does all four steps:

```bash
python3 scripts/murmur_asr.py /path/to/audio.m4a                     # plain text
python3 scripts/murmur_asr.py talk.mp4 --format srt -o talk.srt       # subtitles
python3 scripts/murmur_asr.py clip.wav --language Chinese --format json  # segments + word timestamps
python3 scripts/murmur_asr.py a.m4a --url http://192.168.1.23:7895     # Murmur on another machine (uploads)
```

## Calling the API directly

Same machine as Murmur (preferred — no upload, the app reads the file in place):

```bash
curl -s http://127.0.0.1:7895/transcribe \
  -H 'Content-Type: application/json' \
  -d '{"path": "/abs/path/meeting.m4a", "format": "srt"}'
```

From any machine (required over the LAN — `path` is refused for remote callers):

```bash
curl -s -F file=@meeting.m4a -F format=txt http://127.0.0.1:7895/transcribe
```

Options (JSON body or multipart fields): `language` (`auto` or a value copied exactly from `/health` → `model.languages`; values differ per model, e.g. `Chinese` for Qwen3-ASR), `itn` (number/punctuation normalization), `model` (catalog id), `title`, `format` (`json` default, `txt`, `srt`, `vtt`, `md`), `wait` (default `true`).

By default the request blocks until transcription finishes (roughly 10–20× faster than real time on Apple silicon; long files can take minutes). For long files use `"wait": false`, then poll `GET /jobs/{id}` every few seconds until `status` is `done`, `failed` or `cancelled`.

## Rules

- Call from a program (curl, Python, Node), not a browser page — requests with an `Origin` header are rejected by design.
- Remote callers can't use `path` or list history (`GET /transcripts`); fetch results by job id instead.
- Jobs run one at a time on the shared engine; don't fire many in parallel, queue them.
- Do not invent language values; copy them from `/health` → `model.languages`. Usually omit `language` and let the model detect it.
- Report API `error` messages verbatim instead of retrying blindly.

Full endpoint and response reference: [references/api.md](references/api.md).
