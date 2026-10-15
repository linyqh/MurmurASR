import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface CatalogModel {
  id: string;
  /** Absent for ordinary ASR models. */
  kind?: "asr" | "aligner";
  name: string;
  family: string;
  precision: string;
  file: string;
  size: number;
  sha256: string;
  license: string;
  languages: string;
  summary: string;
  speed: number;
  accuracy: number;
  tags: string[];
  wordTimestamps: boolean;
  speakerDiarization: boolean;
  /** Companion aligner that adds word timestamps once installed. */
  aligner?: string;
  sources: Record<string, string>;
}

export interface Catalog {
  version: number;
  recommended: string;
  models: CatalogModel[];
}

export interface InstalledModel {
  id: string;
  path: string;
  size: number;
}

export interface LoadedModelInfo {
  id: string;
  family: string;
  languages: string[];
  acceptsLanguage: boolean;
  acceptsItn: boolean;
  wordTimestamps: boolean;
  backend: string;
  loadMs: number;
}

export interface ModelsSnapshot {
  catalog: Catalog;
  installed: InstalledModel[];
  downloading: string[];
  loaded: LoadedModelInfo | null;
  loading: string | null;
}

export type DownloadSource = "auto" | "huggingface" | "hfmirror" | "modelscope";

export interface Settings {
  activeModel: string | null;
  backend: string;
  threads: number;
  language: string;
  itn: boolean;
  downloadSource: DownloadSource;
  inputDevice: string | null;
  saveRecordings: boolean;
  modelsDir: string | null;
  onboarded: boolean;
}

export interface AppInfo {
  version: string;
  engineVersion: string | null;
  engineError: string | null;
  platform: string;
  arch: string;
  dataDir: string;
  modelsDir: string;
  backends: string[];
}

export interface Word {
  text: string;
  start: number;
  end: number;
}

export interface Segment {
  index: number;
  start: number;
  end: number;
  text: string;
  words?: Word[];
}

export interface Transcript {
  id: string;
  title: string;
  source: "file" | "live";
  createdAt: number;
  duration: number;
  language: string;
  model: string;
  audioPath: string | null;
  segments: Segment[];
  elapsedMs: number;
}

export interface TranscriptSummary {
  id: string;
  title: string;
  source: "file" | "live";
  createdAt: number;
  duration: number;
  preview: string;
}

export interface InputDevice {
  id: string;
  name: string;
  isDefault: boolean;
}

export type ExportFormat = "txt" | "srt" | "vtt" | "json" | "md";

export type DownloadEvent =
  | { state: "progress"; id: string; downloaded: number; total: number; bytesPerSec: number; source: string }
  | { state: "verifying"; id: string }
  | { state: "done"; id: string; path: string }
  | { state: "failed"; id: string; error: string }
  | { state: "cancelled"; id: string };

export type EngineEvent =
  | { kind: "modelLoading"; id: string }
  | { kind: "modelLoaded"; info: LoadedModelInfo }
  | { kind: "modelError"; id: string; error: string }
  | { kind: "modelUnloaded" }
  | { kind: "jobStarted"; job: string; duration: number }
  | { kind: "jobPhase"; job: string; phase: "decoding" | "detecting"; progress: number }
  | { kind: "jobSegment"; job: string; segment: Segment; progress: number }
  | { kind: "jobDone"; job: string; language: string; elapsedMs: number; audioSeconds: number }
  | { kind: "jobFailed"; job: string; error: string }
  | { kind: "jobCancelled"; job: string }
  | { kind: "liveStarted"; session: string }
  | { kind: "liveLevel"; session: string; rms: number }
  | { kind: "livePartial"; session: string; text: string }
  | { kind: "liveSegment"; session: string; segment: Segment }
  | { kind: "liveStopped"; session: string; duration: number; audioPath: string | null }
  | { kind: "liveError"; session: string; error: string };

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) => invoke<Settings>("update_settings", { settings }),
  listModels: () => invoke<ModelsSnapshot>("list_models"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),
  cancelDownload: (id: string) => invoke<boolean>("cancel_download", { id }),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),
  loadModel: (id: string) => invoke<void>("load_model", { id }),
  transcribeFile: (path: string) => invoke<string>("transcribe_file", { path }),
  cancelJob: (id: string) => invoke<boolean>("cancel_job", { id }),
  startLive: (title: string) => invoke<string>("start_live", { title }),
  stopLive: (id: string) => invoke<boolean>("stop_live", { id }),
  listInputDevices: () => invoke<InputDevice[]>("list_input_devices"),
  listHistory: () => invoke<TranscriptSummary[]>("list_history"),
  getTranscript: (id: string) => invoke<Transcript>("get_transcript", { id }),
  updateTranscript: (id: string, patch: { title?: string; segments?: Segment[] }) =>
    invoke<Transcript>("update_transcript", { id, title: patch.title ?? null, segments: patch.segments ?? null }),
  deleteTranscript: (id: string) => invoke<void>("delete_transcript", { id }),
  exportTranscript: (id: string, format: ExportFormat, path?: string) =>
    invoke<string>("export_transcript", { id, format, path: path ?? null }),
};

export const onEngineEvent = (cb: (e: EngineEvent) => void): Promise<UnlistenFn> =>
  listen<EngineEvent>("engine://event", (e) => cb(e.payload));

export const onDownloadEvent = (cb: (e: DownloadEvent) => void): Promise<UnlistenFn> =>
  listen<DownloadEvent>("models://download", (e) => cb(e.payload));

export const onHistoryChanged = (cb: () => void): Promise<UnlistenFn> =>
  listen("history://changed", () => cb());
