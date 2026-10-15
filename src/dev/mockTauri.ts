/**
 * Browser-only stand-in for the Tauri IPC, used to preview the UI with `?mock=<scenario>`
 * during development. Never loaded inside the app.
 */
import catalog from "../../src-tauri/catalog.json";

type Handler = (args: Record<string, unknown>) => unknown;

const now = Date.now();
const loaded = {
  id: "qwen3-asr-0.6b-q8",
  family: "qwen3_asr",
  languages: ["Auto", "Chinese", "English", "Cantonese", "Japanese", "Korean", "French", "German"],
  acceptsLanguage: true,
  acceptsItn: false,
  wordTimestamps: false,
  backend: "metal",
  loadMs: 1040,
};

const segments = [
  { index: 0, start: 0, end: 8.7, text: "大家好，欢迎收听今天的节目。今天我们来聊一聊人工智能在语音识别领域的最新进展。" },
  { index: 1, start: 9.9, end: 18, text: "近年来，端到端的语音识别模型取得了突破性的进展，识别准确率大幅提升。" },
  { index: 2, start: 19.7, end: 31.4, text: "与此同时，模型的体积也在不断缩小。现在已经可以在个人电脑上实时运行。感谢大家的收听，我们下期再见。" },
  { index: 3, start: 33.1, end: 52.6, text: "Friends, neighbors, and fellow builders of the common life—we meet at a time when it is easy to confuse noise with strength and speed with progress." },
];

/** Spreads per-character (CJK) or per-word timings evenly across a segment. */
function mockWords(seg: { start: number; end: number; text: string }) {
  const units = /[一-鿿]/.test(seg.text)
    ? (seg.text.match(/[一-鿿]|[A-Za-z0-9]+/g) ?? [])
    : seg.text.split(/\s+/).map((w) => w.replace(/[^\p{L}\p{N}']/gu, "")).filter(Boolean);
  const step = (seg.end - seg.start) / Math.max(1, units.length);
  return units.map((text, i) => ({ text, start: seg.start + i * step, end: seg.start + (i + 0.85) * step }));
}

const history = [
  { id: "t1", title: "AI 播客第 42 期", source: "file", createdAt: now - 3_600_000, duration: 1854, preview: segments[0].text },
  { id: "t2", title: "实时转写 10月15日 09:12", source: "live", createdAt: now - 6 * 3_600_000, duration: 312, preview: "今天的站会主要同步三件事" },
  { id: "t3", title: "产品评审会议录音", source: "file", createdAt: now - 26 * 3_600_000, duration: 3720, preview: "我们先看一下上周的数据" },
  { id: "t4", title: "Commencement speech", source: "file", createdAt: now - 9 * 86_400_000, duration: 418, preview: "Friends, neighbors" },
];

function scenario() {
  return new URLSearchParams(location.search).get("mock") ?? "home";
}

function snapshot() {
  const s = scenario();
  const fresh = s.startsWith("onboarding") || s === "empty";
  return {
    catalog,
    installed: fresh
      ? []
      : [
          { id: "qwen3-asr-0.6b-q8", path: "/models/qwen3.gguf", size: 1151272416 },
          { id: "sensevoice-small-q8", path: "/models/sense.gguf", size: 254211200 },
          ...(s === "models"
            ? [{ id: "qwen3-forced-aligner-0.6b-q8", path: "/models/aligner.gguf", size: 1129966496 }]
            : []),
        ],
    downloading: s === "models" ? ["fun-asr-nano-q8"] : s === "onboarding-dl" ? ["qwen3-asr-0.6b-q8"] : [],
    loaded: fresh || s === "onboarding-dl" ? null : loaded,
    loading: null,
  };
}

const handlers: Record<string, Handler> = {
  app_info: () => ({
    version: "0.1.0",
    engineVersion: "0.9.1",
    engineError: null,
    platform: "macos",
    arch: "aarch64",
    dataDir: "/Users/demo/Library/Application Support/app.murmur.asr",
    modelsDir: "/Users/demo/Library/Application Support/app.murmur.asr/models",
    backends: ["metal", "cpu"],
  }),
  get_settings: () => ({
    activeModel: "qwen3-asr-0.6b-q8",
    backend: "metal",
    threads: 6,
    language: "auto",
    itn: true,
    downloadSource: "auto",
    inputDevice: null,
    saveRecordings: true,
    modelsDir: null,
    onboarded: !scenario().startsWith("onboarding"),
    apiEnabled: true,
    apiLan: true,
    apiPort: 7895,
  }),
  update_settings: (a) => a.settings,
  api_status: () => ({
    running: true,
    url: "http://127.0.0.1:7895",
    lanUrl: "http://192.168.1.23:7895",
    error: null,
    skillDir: "/Applications/Murmur.app/Contents/Resources/skills/murmur-asr",
  }),
  list_models: snapshot,
  list_history: () => (scenario() === "empty" || scenario().startsWith("onboarding") ? [] : history),
  list_input_devices: () => [
    { id: "coreaudio:builtin", name: "MacBook Pro 麦克风", isDefault: true },
    { id: "coreaudio:airpods", name: "AirPods Pro", isDefault: false },
  ],
  export_files: (a) => (a.formats as string[]).map((f) => `${a.dir}/AI 播客第 43 期.${f}`),
  get_transcript: (a) => ({
    ...(history.find((h) => h.id === a.id) ?? { id: a.id, title: "AI 播客第 43 期", source: "file", createdAt: now, duration: 1854 }),
    language: "Chinese",
    model: "Qwen3-ASR 0.6B",
    audioPath: "/tmp/demo.m4a",
    segments: segments.map((seg, i) => (i === 1 ? seg : { ...seg, words: mockWords(seg) })),
    elapsedMs: 9000,
  }),
  transcribe_file: () => {
    setTimeout(() => {
      emit("engine://event", { kind: "jobStarted", job: "job-1", duration: 1854 });
      emit("engine://event", { kind: "jobSegment", job: "job-1", segment: segments[0], progress: 0.42 });
      emit("engine://event", { kind: "jobSegment", job: "job-1", segment: segments[1], progress: 0.46 });
    }, 50);
    return "job-1";
  },
  start_live: () => {
    setTimeout(() => {
      emit("engine://event", { kind: "liveStarted", session: "live-1" });
      emit("engine://event", { kind: "liveSegment", session: "live-1", segment: segments[0] });
      emit("engine://event", { kind: "liveSegment", session: "live-1", segment: segments[1] });
      emit("engine://event", { kind: "livePartial", session: "live-1", text: "与此同时，模型的体积也在不断" });
      let t = 0;
      setInterval(() => {
        t += 1;
        emit("engine://event", { kind: "liveLevel", session: "live-1", rms: 0.01 + 0.08 * Math.abs(Math.sin(t / 2.3)) * Math.random() });
      }, 80);
    }, 50);
    return "live-1";
  },
};

let callbackId = 0;
const callbacks = new Map<number, (payload: unknown) => void>();
let listeners: { id: number; event: string; handler: number }[] = [];
let listenerId = 0;

export function emit(event: string, payload: unknown) {
  for (const l of listeners.filter((x) => x.event === event)) {
    callbacks.get(l.handler)?.({ event, id: 0, payload });
  }
}

export function installMock() {
  const w = window as unknown as Record<string, unknown>;
  w.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
    transformCallback: (cb: (p: unknown) => void) => {
      const id = ++callbackId;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback: (id: number) => callbacks.delete(id),
    convertFileSrc: (p: string) => `asset://localhost${p}`,
    invoke: async (cmd: string, args: Record<string, unknown>) => {
      if (cmd === "plugin:event|listen") {
        const id = ++listenerId;
        listeners.push({ id, event: args.event as string, handler: args.handler as number });
        return id;
      }
      if (cmd === "plugin:event|unlisten") {
        listeners = listeners.filter((l) => l.id !== args.eventId);
        return null;
      }
      if (cmd === "plugin:dialog|open") return ["/Users/demo/Podcasts/AI 播客第 43 期.m4a"];
      if (cmd.startsWith("plugin:")) return null;
      const h = handlers[cmd];
      if (!h) throw new Error(`mock: unhandled ${cmd}`);
      return h(args);
    },
  };
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (_event: string, id: number) => {
      listeners = listeners.filter((l) => l.id !== id);
    },
  };

  // Drive scenario-specific live events once the UI has subscribed.
  setTimeout(() => {
    const s = scenario();
    if (s === "models") {
      emit("models://download", {
        state: "progress",
        id: "fun-asr-nano-q8",
        downloaded: 412_000_000,
        total: 1_045_334_432,
        bytesPerSec: 18_400_000,
        source: "ModelScope",
      });
    }
    if (s === "onboarding-dl") {
      emit("models://download", {
        state: "progress",
        id: "qwen3-asr-0.6b-q8",
        downloaded: 702_000_000,
        total: 1_151_272_416,
        bytesPerSec: 24_800_000,
        source: "Hugging Face",
      });
    }
    // Drive the real UI so the store registers job/session ids as in the app.
    if (s === "jobs" || s === "result") (document.querySelector(".uploader") as HTMLElement | null)?.click();
    if (s === "result") {
      setTimeout(() => {
        emit("engine://event", { kind: "jobSegment", job: "job-1", segment: segments[2], progress: 0.9 });
        emit("engine://event", { kind: "jobSegment", job: "job-1", segment: segments[3], progress: 1 });
        emit("engine://event", { kind: "jobDone", job: "job-1", language: "Chinese", elapsedMs: 92_000, audioSeconds: 1854 });
      }, 300);
    }
    if (s === "live") (document.querySelector(".record-btn") as HTMLElement | null)?.click();
  }, 400);
}
