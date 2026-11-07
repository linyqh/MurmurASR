import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { isRunning, useStore } from "../lib/store";
import { formatDuration, formatRelativeDate } from "../lib/format";
import { IconCube, IconGear, IconGlobe, IconMic, IconSearch, IconWaveform, IconDoc } from "../components/icons";

export function Sidebar() {
  const { t } = useTranslation("sidebar");
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
  const activeJobs = Object.values(jobs).filter(isRunning).length;

  return (
    <aside className="sidebar">
      <div className="sidebar-titlebar drag" data-tauri-drag-region />

      <div className="brand drag" data-tauri-drag-region>
        <div className="brand-name" aria-label="MurmurASR">
          <span className="brand-name-light">Murmur</span>
          <span className="brand-name-accent">ASR</span>
        </div>
        <div className="brand-tagline">{t("brand.tagline")}</div>
      </div>

      <div className="search-field no-drag">
        <IconSearch size={13} />
        <input
          placeholder={t("search.placeholder")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          aria-label={t("search.label")}
        />
      </div>

      <nav className="no-drag" aria-label={t("nav.label")}>
        <button
          className={`sidebar-item${view.name === "home" ? " active" : ""}`}
          onClick={() => setView({ name: "home" })}
        >
          <IconWaveform className="icon" />
          {t("nav.home")}
          {activeJobs > 0 && <span className="count">{activeJobs}</span>}
        </button>
        <button
          className={`sidebar-item${view.name === "live" ? " active" : ""}`}
          onClick={() => setView({ name: "live" })}
        >
          <IconMic className="icon" />
          {t("nav.live")}
          {live.id && <span className="rec-dot" aria-label={t("nav.recording")} />}
        </button>
        <button
          className={`sidebar-item${view.name === "models" ? " active" : ""}`}
          onClick={() => setView({ name: "models" })}
        >
          <IconCube className="icon" />
          {t("nav.models")}
        </button>
        <button
          className={`sidebar-item${view.name === "api" ? " active" : ""}`}
          onClick={() => setView({ name: "api" })}
        >
          <IconGlobe className="icon" />
          {t("nav.api")}
          {settings?.apiEnabled && <span className={`status-dot ${apiRunning ? "ready" : "idle"}`} />}
        </button>
      </nav>

      <div className="sidebar-section-label">{t("history.label")}</div>
      <div className="sidebar-history no-drag">
        {filtered.length === 0 && (
          <div className="sidebar-empty">{query ? t("history.noMatches") : t("history.empty")}</div>
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
            {loadingName ? t("model.loading", { name: loadingName }) : (loadedName ?? t("model.none"))}
          </span>
          {loaded && <span className="badge">{loaded.backend.toUpperCase()}</span>}
        </button>
        <button
          className={`sidebar-item${view.name === "settings" ? " active" : ""}`}
          onClick={() => setView({ name: "settings" })}
        >
          <IconGear className="icon" />
          {t("nav.settings")}
        </button>
      </div>
    </aside>
  );
}
