#!/usr/bin/env python3
"""Transcribe a local audio/video file through the running Murmur app."""

from __future__ import annotations

import argparse
import json
import os
import platform
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

DEFAULT_URL = "http://127.0.0.1:7895"
APP_ID = "app.murmur.asr"


def discovery_file() -> Path:
    system = platform.system()
    if system == "Darwin":
        base = Path.home() / "Library" / "Application Support"
    elif system == "Windows":
        base = Path(os.environ.get("APPDATA", Path.home() / "AppData" / "Roaming"))
    else:
        base = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local" / "share"))
    return base / APP_ID / "api.json"


def base_url(explicit: str | None) -> str:
    if explicit:
        return explicit.rstrip("/")
    if env := os.environ.get("MURMUR_API_URL"):
        return env.rstrip("/")
    try:
        return json.loads(discovery_file().read_text())["baseUrl"].rstrip("/")
    except (OSError, ValueError, KeyError):
        return DEFAULT_URL


class ApiError(Exception):
    pass


def request(url: str, method: str = "GET", body: dict | None = None, timeout: float = 30):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    if data is not None:
        req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read()
            if resp.headers.get_content_type() == "application/json":
                return json.loads(raw)
            return raw.decode("utf-8")
    except urllib.error.HTTPError as e:
        try:
            message = json.loads(e.read()).get("error", e.reason)
        except ValueError:
            message = e.reason
        raise ApiError(f"HTTP {e.code}: {message}") from None
    except urllib.error.URLError as e:
        raise ApiError(
            f"cannot reach Murmur at {url} ({e.reason}). Open the Murmur app and enable "
            "Settings → 本地接口 → 允许其他程序调用."
        ) from None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("audio", help="path to an audio or video file")
    parser.add_argument("--format", default="txt", choices=["txt", "json", "srt", "vtt", "md"])
    parser.add_argument("--language", help="language code from /health, or 'auto'")
    parser.add_argument("--model", help="catalog model id to use (see GET /models)")
    parser.add_argument("--itn", choices=["true", "false"], help="number/punctuation normalization")
    parser.add_argument("--title", help="title shown in Murmur's history")
    parser.add_argument("-o", "--output", help="write the result to this file instead of stdout")
    parser.add_argument("--url", help=f"API base URL (default: discovery file or {DEFAULT_URL})")
    args = parser.parse_args()

    audio = Path(args.audio).expanduser().resolve()
    if not audio.is_file():
        print(f"error: file not found: {audio}", file=sys.stderr)
        return 2
    url = base_url(args.url)

    try:
        health = request(f"{url}/health", timeout=5)
        if health.get("engine", {}).get("error"):
            raise ApiError(f"Murmur engine unavailable: {health['engine']['error']}")
        model = args.model
        if not model and not health.get("model") and not health.get("loading"):
            installed = health.get("installedModels") or []
            if not installed:
                raise ApiError("no ASR model is installed; download one in Murmur's 模型 page")
            model = installed[0]
            print(f"loading model {model} …", file=sys.stderr)

        body: dict = {"path": str(audio), "format": args.format, "wait": False}
        if model:
            body["model"] = model
        if args.language:
            body["language"] = args.language
        if args.itn:
            body["itn"] = args.itn == "true"
        if args.title:
            body["title"] = args.title
        job = request(f"{url}/transcribe", "POST", body)

        last = None
        while True:
            state = request(f"{url}/jobs/{job['id']}")
            status = state["status"]
            if status == "done":
                break
            if status in ("failed", "cancelled"):
                raise ApiError(f"transcription {status}: {state.get('error', '')}".rstrip(": "))
            if status != last:
                print(f"{status} …", file=sys.stderr)
                last = status
            time.sleep(1.5)

        if args.format == "json":
            result = json.dumps(state["result"], ensure_ascii=False, indent=2) + "\n"
        else:
            result = request(f"{url}/transcripts/{job['id']}?format={args.format}")
    except ApiError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1

    if args.output:
        Path(args.output).write_text(result, encoding="utf-8")
        print(f"wrote {args.output}", file=sys.stderr)
    else:
        sys.stdout.write(result)
    return 0


if __name__ == "__main__":
    sys.exit(main())
