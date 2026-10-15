import { useMemo, useState } from "react";
import { useStore } from "../lib/store";
import { formatDuration, formatRelativeDate } from "../lib/format";
import { IconCube, IconGear, IconGlobe, IconMic, IconSearch, IconWaveform, IconDoc } from "../components/icons";

export function Sidebar() {
  const { view, setView, history, models, live, jobs, settings, apiStatus } = useStore();
  const apiRunning = apiStatus?.running ?? false;
  const [query, setQuery] = useState("");

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return history;
    return history.filter((h) => h.title.toLowerCase().includes(q) || h.preview.toLowerCase().includes(q));
  }, [history, query]);

  const loaded = models?.loaded;
  const loadedName = loaded ? models?.catalog.models.find((m) => m.id === loaded.id)?.name : null;
  const loadingName = models?.loading
    ? models.catalog.models.find((m) => m.id === models.loading)?.name
    : null;
  const activeJobs = Object.keys(jobs).length;

  return (
    <aside className="sidebar">
      <div className="sidebar-titlebar drag" data-tauri-drag-region />

      <div className="search-field no-drag">
        <IconSearch size={13} />
        <input
          placeholder="搜索"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label="搜索转写记录"
        />
      </div>

      <nav className="no-drag" aria-label="主导航">
        <button
          className={`sidebar-item${view.name === "home" ? " active" : ""}`}
          onClick={() => setView({ name: "home" })}
        >
          <IconWaveform className="icon" />
          转写文件
          {activeJobs > 0 && <span className="count">{activeJobs}</span>}
        </button>
        <button
          className={`sidebar-item${view.name === "live" ? " active" : ""}`}
          onClick={() => setView({ name: "live" })}
        >
          <IconMic className="icon" />
          实时听写
          {live.id && <span className="rec-dot" aria-label="正在录音" />}
        </button>
        <button
          className={`sidebar-item${view.name === "models" ? " active" : ""}`}
          onClick={() => setView({ name: "models" })}
        >
          <IconCube className="icon" />
          模型
        </button>
        <button
          className={`sidebar-item${view.name === "api" ? " active" : ""}`}
          onClick={() => setView({ name: "api" })}
        >
          <IconGlobe className="icon" />
          API 接口
          {settings?.apiEnabled && <span className={`status-dot ${apiRunning ? "ready" : "idle"}`} />}
        </button>
      </nav>

      <div className="sidebar-section-label">记录</div>
      <div className="sidebar-history no-drag">
        {filtered.length === 0 && (
          <div className="sidebar-empty">{query ? "没有匹配的记录" : "转写记录会显示在这里"}</div>
        )}
        {filtered.map((h) => (
          <button
            key={h.id}
            className={`history-item${view.name === "transcript" && view.id === h.id ? " active" : ""}`}
            onClick={() => setView({ name: "transcript", id: h.id })}
          >
            <div className="title">
              {h.source === "live" ? <IconMic size={12} /> : <IconDoc size={12} />}
              <span>{h.title}</span>
            </div>
            <div className="meta">
              {formatRelativeDate(h.createdAt)} · {formatDuration(h.duration)}
            </div>
          </button>
        ))}
      </div>

      <div className="sidebar-footer no-drag">
        <button className="model-chip" onClick={() => setView({ name: "models" })}>
          <span className={`status-dot ${loaded ? "ready" : loadingName ? "busy" : "idle"}`} />
          <span className="model-chip-text">
            {loadingName ? `正在加载 ${loadingName}…` : loadedName ?? "未加载模型"}
          </span>
          {loaded && <span className="badge">{loaded.backend.toUpperCase()}</span>}
        </button>
        <button
          className={`sidebar-item${view.name === "settings" ? " active" : ""}`}
          onClick={() => setView({ name: "settings" })}
        >
          <IconGear className="icon" />
          设置
        </button>
      </div>
    </aside>
  );
}
