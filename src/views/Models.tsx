import { useMemo, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { ask } from "@tauri-apps/plugin-dialog";
import { api, modelSize, type CatalogModel } from "../lib/api";
import { useStore, type DownloadProgress } from "../lib/store";
import { formatBytes, formatEta } from "../lib/format";
import { errorMessage } from "../lib/errors";
import { useCatalogText } from "../lib/catalogText";
import { Meter, Progress, Segmented } from "../components/controls";
import {
  IconCheck,
  IconCheckCircle,
  IconDownload,
  IconFolder,
  IconSearch,
  IconSparkle,
  IconTrash,
  IconX,
} from "../components/icons";

type Filter = "all" | "installed";

/** Whether every whitespace-separated term of `query` appears in the model's (localized) `fields`. */
function matches(fields: string[], query: string): boolean {
  const text = fields.join(" ").toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((term) => text.includes(term));
}

export function ModelsView() {
  const { t } = useTranslation(["models", "common"]);
  const c = useCatalogText();
  const { models, downloads, settings, saveSettings } = useStore();
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const installed = useMemo(() => new Set(models?.installed.map((m) => m.id)), [models?.installed]);
  const visible = useMemo(
    () =>
      (models?.catalog.models ?? []).filter(
        (m) =>
          m.kind !== "aligner" &&
          (filter === "all" || installed.has(m.id)) &&
          matches(
            [
              m.name,
              m.family,
              c.languages(m),
              c.summary(m),
              c.description(m),
              m.license,
              ...m.tags.map((tag) => c.tag(tag)),
            ],
            query,
          ),
      ),
    [models?.catalog.models, filter, installed, query, c],
  );
  if (!models || !settings) return null;

  const recommended = models.catalog.models.find((m) => m.id === models.catalog.recommended);
  // The recommended model is featured on top of the unfiltered list only.
  const featured = !query && recommended && visible.includes(recommended) ? recommended : undefined;
  const groups: { name: string; items: CatalogModel[] }[] = [];
  for (const m of visible) {
    if (m === featured) continue;
    const group = groups.find((g) => g.name === m.group);
    if (group) group.items.push(m);
    else groups.push({ name: m.group, items: [m] });
  }
  const totalInstalled = models.installed.reduce((sum, m) => sum + m.size, 0);

  return (
    <>
      <header className="toolbar drag" data-tauri-drag-region>
        <div className="toolbar-title">{t("models:toolbar.title")}</div>
        {models.installed.length > 0 && (
          <div className="toolbar-subtitle">
            {t("models:toolbar.installedSummary", {
              count: models.installed.length,
              size: formatBytes(totalInstalled),
            })}
          </div>
        )}
        <div className="toolbar-spacer" />
        <div className="search-field model-search no-drag">
          <IconSearch size={13} />
          <input
            placeholder={t("models:search.placeholder")}
            aria-label={t("models:search.label")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <div className="no-drag">
          <Segmented
            label={t("models:filter.label")}
            value={filter}
            onChange={setFilter}
            options={[
              { value: "all", label: t("models:filter.all") },
              { value: "installed", label: t("models:filter.installed") },
            ]}
          />
        </div>
      </header>
      <div className="page">
        <p className="page-intro muted">
          <Trans
            t={t}
            i18nKey="models:intro"
            count={models.catalog.models.filter((m) => m.kind !== "aligner").length}
            values={{ name: recommended?.name ?? "" }}
            components={{ strong: <strong /> }}
          />
        </p>

        {featured && (
          <ModelCard
            model={featured}
            featured
            installed={installed.has(featured.id)}
            download={downloads[featured.id]}
            queued={models.downloading.includes(featured.id)}
          />
        )}

        {groups.map((g) => (
          <section key={g.name}>
            <h2 className="section-title">
              {c.group(g.name)} <span className="muted">{g.items.length}</span>
            </h2>
            <div className="model-grid">
              {g.items.map((m) => (
                <ModelCard
                  key={m.id}
                  model={m}
                  installed={installed.has(m.id)}
                  download={downloads[m.id]}
                  queued={models.downloading.includes(m.id)}
                />
              ))}
            </div>
          </section>
        ))}

        {visible.length === 0 && (
          <div className="empty compact">
            <p>{query ? t("models:empty.noResults", { query }) : t("models:empty.noneInstalled")}</p>
          </div>
        )}

        <div className="source-row">
          <span className="muted">{t("models:source.label")}</span>
          <Segmented
            label={t("models:source.label")}
            value={settings.downloadSource}
            onChange={(downloadSource) => void saveSettings({ downloadSource })}
            options={[
              { value: "auto", label: t("models:source.auto") },
              { value: "huggingface", label: "Hugging Face" },
              { value: "modelscope", label: "ModelScope" },
              { value: "hfmirror", label: t("models:source.hfMirror") },
            ]}
          />
          <span className="muted source-hint">{t("models:source.hint")}</span>
        </div>
      </div>
    </>
  );
}

function ModelCard({
  model,
  featured,
  installed,
  download,
  queued,
}: {
  model: CatalogModel;
  featured?: boolean;
  installed: boolean;
  download?: DownloadProgress;
  queued: boolean;
}) {
  const { t } = useTranslation(["models", "common"]);
  const c = useCatalogText();
  const { models, refreshModels, toast, info } = useStore();
  const loaded = models?.loaded?.id === model.id;
  const loading = models?.loading === model.id;
  const downloading = !!download || queued;
  const aligner = model.aligner ? models?.catalog.models.find((m) => m.id === model.aligner) : undefined;

  const startDownload = async () => {
    try {
      await api.downloadModel(model.id);
      await refreshModels();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const remove = async () => {
    const ok = await ask(t("models:deleteDialog.message", { name: model.name, size: formatBytes(modelSize(model)) }), {
      title: t("models:deleteDialog.title"),
      kind: "warning",
      okLabel: t("common:actions.delete"),
      cancelLabel: t("common:actions.cancel"),
    });
    if (!ok) return;
    try {
      await api.deleteModel(model.id);
      await refreshModels();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const reveal = () => {
    const entry = models?.installed.find((m) => m.id === model.id);
    if (entry) void revealItemInDir(entry.path);
  };

  return (
    <article className={`model-card card${featured ? " featured" : ""}${loaded ? " loaded" : ""}`}>
      {featured && (
        <div className="featured-label">
          <IconSparkle size={12} /> {t("models:card.recommended")}
        </div>
      )}
      <div className="model-head">
        <div className="model-name">
          {model.name}
          <span className="badge">{model.precision}</span>
          {loaded && <span className="badge green">{t("models:card.inUse")}</span>}
        </div>
        <div className="model-size muted tabular">{formatBytes(modelSize(model))}</div>
      </div>
      <p className="model-summary">{c.summary(model)}</p>
      <p className="model-description">{c.description(model)}</p>
      <div className="model-facts">
        <div>
          <span className="muted">{t("models:card.languages")}</span>
          {c.languages(model)}
        </div>
        <div>
          <span className="muted">{t("models:card.speed")}</span>
          <Meter level={model.speed} />
        </div>
        <div>
          <span className="muted">{t("models:card.accuracy")}</span>
          <Meter level={model.accuracy} />
        </div>
      </div>
      <div className="model-capabilities">
        <CapabilityBadge supported={model.wordTimestamps} label={t("models:card.wordTimestamps")} />
        <CapabilityBadge supported={model.speakerDiarization} label={t("models:card.speakerDiarization")} />
      </div>
      {aligner && <AlignerAddon aligner={aligner} owner={model} />}
      <div className="model-tags">
        {model.tags.map((tag) => (
          <span key={tag} className="badge">
            {c.tag(tag)}
          </span>
        ))}
        <span className="model-license muted">{model.license}</span>
      </div>

      <div className="model-actions">
        {downloading ? (
          <DownloadStatus model={model} download={download} />
        ) : installed ? (
          <>
            {loaded ? (
              <span className="loaded-label">
                <IconCheckCircle size={15} /> {t("models:card.loaded")}
                {models?.loaded && (
                  <span className="muted">
                    {" "}
                    · {models.loaded.backend.toUpperCase()} · {t("models:card.loadTime", { seconds: (models.loaded.loadMs / 1000).toFixed(1) })}
                  </span>
                )}
              </span>
            ) : (
              <button
                className="btn primary"
                disabled={loading || !!models?.loading || !!info?.engineError}
                onClick={() => void api.loadModel(model.id).catch((e) => toast(errorMessage(e), "error"))}
              >
                {loading ? (
                  <>
                    <span className="spinner light" /> {t("models:card.loading")}
                  </>
                ) : (
                  t("models:card.use")
                )}
              </button>
            )}
            <div className="toolbar-spacer" />
            <button className="icon-btn" aria-label={t("models:card.reveal")} onClick={reveal}>
              <IconFolder size={15} />
            </button>
            <button className="icon-btn" aria-label={t("models:card.deleteLabel")} onClick={() => void remove()}>
              <IconTrash size={15} />
            </button>
          </>
        ) : (
          <button className={`btn${featured ? " primary" : ""}`} onClick={() => void startDownload()}>
            <IconDownload size={14} /> {t("common:actions.download")}
          </button>
        )}
      </div>
    </article>
  );
}

/** Optional companion download that adds word timestamps to `owner`. */
function AlignerAddon({ aligner, owner }: { aligner: CatalogModel; owner: CatalogModel }) {
  const { t } = useTranslation(["models", "common"]);
  const c = useCatalogText();
  const { models, downloads, refreshModels, toast } = useStore();
  const installed = !!models?.installed.some((m) => m.id === aligner.id);
  const download = downloads[aligner.id];
  const downloading = !!download || !!models?.downloading.includes(aligner.id);
  const ownerLoaded = models?.loaded?.id === owner.id;
  const active = ownerLoaded && !!models?.loaded?.wordTimestamps;

  const run = async (action: () => Promise<unknown>) => {
    try {
      await action();
      await refreshModels();
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };

  const remove = async () => {
    const ok = await ask(t("models:aligner.deleteMessage", { owner: owner.name }), {
      title: t("models:aligner.deleteTitle", { name: aligner.name }),
      kind: "warning",
      okLabel: t("common:actions.delete"),
      cancelLabel: t("common:actions.cancel"),
    });
    if (ok) await run(() => api.deleteModel(aligner.id));
  };

  return (
    <div className="aligner-addon">
      <div className="aligner-line">
        <span>
          <strong>{aligner.name}</strong>
          <span className="muted"> · {formatBytes(modelSize(aligner))} · {c.summary(aligner)}</span>
        </span>
        <div className="toolbar-spacer" />
        {downloading ? null : installed ? (
          <>
            {ownerLoaded && !active ? (
              <button className="btn" onClick={() => void run(() => api.loadModel(owner.id))}>
                {t("models:aligner.reloadToEnable")}
              </button>
            ) : (
              <span className="aligner-state muted">
                <IconCheck size={12} /> {t("models:aligner.installed")}
              </span>
            )}
            <button className="icon-btn" aria-label={t("models:aligner.deleteLabel")} onClick={() => void remove()}>
              <IconTrash size={14} />
            </button>
          </>
        ) : (
          <button className="btn" onClick={() => void run(() => api.downloadModel(aligner.id))}>
            <IconDownload size={13} /> {t("common:actions.download")}
          </button>
        )}
      </div>
      {downloading && <DownloadStatus model={aligner} download={download} />}
    </div>
  );
}

function CapabilityBadge({ supported, label }: { supported: boolean; label: string }) {
  return (
    <span className={`badge${supported ? " green" : ""}`}>
      {supported ? <IconCheck size={11} /> : <IconX size={11} />} {label}
    </span>
  );
}

function DownloadStatus({ model, download }: { model: CatalogModel; download?: DownloadProgress }) {
  const { t } = useTranslation("models");
  const fraction = download ? download.downloaded / Math.max(1, download.total) : 0;
  return (
    <div className="download-status">
      <div className="download-line">
        <span className="tabular">
          {download?.verifying
            ? t("download.verifying")
            : download
              ? `${formatBytes(download.downloaded)} / ${formatBytes(download.total)}`
              : t("download.connecting")}
        </span>
        {download && !download.verifying && (
          <span className="muted tabular">
            {formatBytes(download.bytesPerSec)}/s · {formatEta(download.total - download.downloaded, download.bytesPerSec)}
            {download.source && ` · ${download.source}`}
          </span>
        )}
        <div className="toolbar-spacer" />
        <button className="icon-btn" aria-label={t("download.cancel")} onClick={() => void api.cancelDownload(model.id)}>
          <IconX size={14} />
        </button>
      </div>
      <Progress value={fraction} indeterminate={!download || download.verifying} />
    </div>
  );
}
