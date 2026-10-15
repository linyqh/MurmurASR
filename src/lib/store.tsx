import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  api,
  onApiJob,
  onDownloadEvent,
  onEngineEvent,
  onHistoryChanged,
  type ApiStatus,
  type AppInfo,
  type DownloadEvent,
  type EngineEvent,
  type ModelsSnapshot,
  type Segment,
  type Settings,
  type TranscriptSummary,
} from "./api";

export type View =
  | { name: "home" }
  | { name: "live" }
  | { name: "models" }
  | { name: "api" }
  | { name: "settings" }
  | { name: "transcript"; id: string };

export interface DownloadProgress {
  downloaded: number;
  total: number;
  bytesPerSec: number;
  source: string;
  verifying: boolean;
}

export interface Job {
  id: string;
  title: string;
  phase: "queued" | "decoding" | "detecting" | "transcribing";
  progress: number;
  duration: number;
  segments: Segment[];
}

export interface LiveState {
  id: string | null;
  starting: boolean;
  level: number;
  partial: string;
  segments: Segment[];
  startedAt: number;
}

export interface Toast {
  id: number;
  text: string;
  tone: "info" | "error";
}

interface Store {
  info: AppInfo | null;
  settings: Settings | null;
  apiStatus: ApiStatus | null;
  models: ModelsSnapshot | null;
  downloads: Record<string, DownloadProgress>;
  history: TranscriptSummary[];
  jobs: Record<string, Job>;
  live: LiveState;
  view: View;
  toasts: Toast[];
  setView: (view: View) => void;
  saveSettings: (patch: Partial<Settings>) => Promise<void>;
  refreshModels: () => Promise<void>;
  refreshHistory: () => Promise<void>;
  toast: (text: string, tone?: Toast["tone"]) => void;
  transcribeFiles: (paths: string[]) => Promise<void>;
  startLive: (title: string) => Promise<void>;
  stopLive: () => Promise<void>;
  subscribe: (fn: (e: EngineEvent) => void) => () => void;
}

const StoreContext = createContext<Store | null>(null);

const IDLE_LIVE: LiveState = {
  id: null,
  starting: false,
  level: 0,
  partial: "",
  segments: [],
  startedAt: 0,
};

function initialView(): View {
  // Dev previews can deep-link a view via the URL hash (e.g. #transcript:t1).
  if (!import.meta.env.DEV || !location.hash) return { name: "home" };
  const [name, id] = location.hash.slice(1).split(":");
  if (name === "transcript" && id) return { name, id };
  if (name === "live" || name === "models" || name === "api" || name === "settings") return { name };
  return { name: "home" };
}

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function StoreProvider({ children }: { children: ReactNode }) {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [apiStatus, setApiStatus] = useState<ApiStatus | null>(null);
  const [models, setModels] = useState<ModelsSnapshot | null>(null);
  const [downloads, setDownloads] = useState<Record<string, DownloadProgress>>({});
  const [history, setHistory] = useState<TranscriptSummary[]>([]);
  const [jobs, setJobs] = useState<Record<string, Job>>({});
  const [live, setLive] = useState<LiveState>(IDLE_LIVE);
  const [view, setView] = useState<View>(initialView);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const listeners = useRef(new Set<(e: EngineEvent) => void>());
  const jobsRef = useRef(jobs);
  jobsRef.current = jobs;
  const modelsRef = useRef(models);
  modelsRef.current = models;

  const toast = useCallback((text: string, tone: Toast["tone"] = "info") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t.slice(-2), { id, text, tone }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), tone === "error" ? 6000 : 3200);
  }, []);

  const refreshModels = useCallback(async () => {
    setModels(await api.listModels());
  }, []);

  // The server restarts whenever its settings change.
  useEffect(() => {
    if (settings) void api.apiStatus().then(setApiStatus);
  }, [settings?.apiEnabled, settings?.apiLan, settings?.apiPort]);

  const refreshHistory = useCallback(async () => {
    setHistory(await api.listHistory());
  }, []);

  const saveSettings = useCallback(
    async (patch: Partial<Settings>) => {
      if (!settings) return;
      const next = { ...settings, ...patch };
      setSettings(next);
      try {
        setSettings(await api.updateSettings(next));
      } catch (e) {
        toast(`保存设置失败：${errorText(e)}`, "error");
      }
    },
    [settings, toast],
  );

  useEffect(() => {
    void api.appInfo().then(setInfo);
    void api.getSettings().then(setSettings);
    void refreshModels();
    void refreshHistory();
  }, [refreshModels, refreshHistory]);

  useEffect(() => {
    const onEngine = (e: EngineEvent) => {
      listeners.current.forEach((fn) => fn(e));
      switch (e.kind) {
        case "modelLoading":
          setModels((m) => (m ? { ...m, loading: e.id } : m));
          break;
        case "modelLoaded":
          setModels((m) => (m ? { ...m, loaded: e.info, loading: null } : m));
          break;
        case "modelError":
          setModels((m) => (m ? { ...m, loaded: null, loading: null } : m));
          toast(`模型加载失败：${e.error}`, "error");
          break;
        case "modelUnloaded":
          setModels((m) => (m ? { ...m, loaded: null } : m));
          break;
        case "jobPhase":
          setJobs((j) =>
            j[e.job] ? { ...j, [e.job]: { ...j[e.job], phase: e.phase, progress: e.progress } } : j,
          );
          break;
        case "jobStarted":
          setJobs((j) =>
            j[e.job]
              ? { ...j, [e.job]: { ...j[e.job], phase: "transcribing", progress: 0, duration: e.duration } }
              : j,
          );
          break;
        case "jobSegment":
          setJobs((j) =>
            j[e.job]
              ? {
                  ...j,
                  [e.job]: {
                    ...j[e.job],
                    phase: "transcribing",
                    progress: e.progress,
                    segments: [...j[e.job].segments, e.segment],
                  },
                }
              : j,
          );
          break;
        case "jobDone": {
          const title = jobsRef.current[e.job]?.title ?? "";
          setJobs(({ [e.job]: _, ...rest }) => rest);
          const speed = e.audioSeconds > 0 ? e.audioSeconds / Math.max(0.001, e.elapsedMs / 1000) : 0;
          toast(`“${title}”转写完成${speed > 0 ? `（${speed.toFixed(0)}× 实时）` : ""}`);
          break;
        }
        case "jobFailed": {
          const title = jobsRef.current[e.job]?.title ?? "";
          setJobs(({ [e.job]: _, ...rest }) => rest);
          toast(`“${title}”转写失败：${e.error}`, "error");
          break;
        }
        case "jobCancelled":
          setJobs(({ [e.job]: _, ...rest }) => rest);
          break;
        case "liveStarted":
          setLive((l) => ({ ...l, starting: false, startedAt: Date.now() }));
          break;
        case "liveLevel":
          setLive((l) => (l.id === e.session ? { ...l, level: e.rms } : l));
          break;
        case "livePartial":
          setLive((l) => (l.id === e.session ? { ...l, partial: e.text } : l));
          break;
        case "liveSegment":
          setLive((l) =>
            l.id === e.session ? { ...l, partial: "", segments: [...l.segments, e.segment] } : l,
          );
          break;
        case "liveStopped":
          setLive((l) => (l.id === e.session ? { ...l, id: null, level: 0, partial: "" } : l));
          break;
        case "liveError":
          setLive((l) => (l.id === e.session ? { ...IDLE_LIVE, segments: l.segments } : l));
          toast(`实时转写出错：${e.error}`, "error");
          break;
      }
    };
    const onDownload = (e: DownloadEvent) => {
      switch (e.state) {
        case "progress":
          setDownloads((d) => ({
            ...d,
            [e.id]: {
              downloaded: e.downloaded,
              total: e.total,
              bytesPerSec: e.bytesPerSec,
              source: e.source,
              verifying: false,
            },
          }));
          break;
        case "verifying":
          setDownloads((d) => (d[e.id] ? { ...d, [e.id]: { ...d[e.id], verifying: true } } : d));
          break;
        case "done":
        case "failed":
        case "cancelled":
          setDownloads(({ [e.id]: _, ...rest }) => rest);
          if (e.state === "failed") toast(`下载失败：${e.error}`, "error");
          if (e.state === "done") {
            const model = modelsRef.current?.catalog.models.find((m) => m.id === e.id);
            toast(`${model?.name ?? e.id} 下载完成`);
            // First model on this machine: make it usable without another click.
            if (model?.kind !== "aligner" && !modelsRef.current?.loaded && !modelsRef.current?.loading) {
              void api.loadModel(e.id).catch((err) => toast(errorText(err), "error"));
            }
          }
          void refreshModels();
          break;
      }
    };
    const onApi = ({ id, title }: { id: string; title: string }) =>
      setJobs((j) => ({ ...j, [id]: { id, title, phase: "queued", progress: 0, duration: 0, segments: [] } }));
    const subs = [
      onEngineEvent(onEngine),
      onDownloadEvent(onDownload),
      onApiJob(onApi),
      onHistoryChanged(() => void refreshHistory()),
    ];
    return () => subs.forEach((p) => void p.then((un) => un()));
  }, [refreshModels, refreshHistory, toast]);

  const transcribeFiles = useCallback(
    async (paths: string[]) => {
      for (const path of paths) {
        try {
          const id = await api.transcribeFile(path);
          const title = path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? path;
          setJobs((j) => ({
            ...j,
            [id]: { id, title, phase: "queued", progress: 0, duration: 0, segments: [] },
          }));
        } catch (e) {
          toast(errorText(e), "error");
        }
      }
    },
    [toast],
  );

  const startLive = useCallback(
    async (title: string) => {
      setLive({ ...IDLE_LIVE, starting: true });
      try {
        const id = await api.startLive(title);
        setLive((l) => ({ ...l, id }));
      } catch (e) {
        setLive(IDLE_LIVE);
        toast(errorText(e), "error");
      }
    },
    [toast],
  );

  const stopLive = useCallback(async () => {
    if (live.id) await api.stopLive(live.id);
  }, [live.id]);

  const subscribe = useCallback((fn: (e: EngineEvent) => void) => {
    listeners.current.add(fn);
    return () => {
      listeners.current.delete(fn);
    };
  }, []);

  const value = useMemo<Store>(
    () => ({
      info,
      settings,
      apiStatus,
      models,
      downloads,
      history,
      jobs,
      live,
      view,
      toasts,
      setView,
      saveSettings,
      refreshModels,
      refreshHistory,
      toast,
      transcribeFiles,
      startLive,
      stopLive,
      subscribe,
    }),
    [
      info,
      settings,
      apiStatus,
      models,
      downloads,
      history,
      jobs,
      live,
      view,
      toasts,
      saveSettings,
      refreshModels,
      refreshHistory,
      toast,
      transcribeFiles,
      startLive,
      stopLive,
      subscribe,
    ],
  );

  return <StoreContext.Provider value={value}>{children}</StoreContext.Provider>;
}

export function useStore(): Store {
  const store = useContext(StoreContext);
  if (!store) throw new Error("useStore must be used inside StoreProvider");
  return store;
}

export const AUDIO_EXTENSIONS = [
  "wav",
  "mp3",
  "m4a",
  "aac",
  "flac",
  "ogg",
  "oga",
  "opus",
  "webm",
  "mp4",
  "mov",
  "mkv",
  "caf",
  "aiff",
  "aif",
];
