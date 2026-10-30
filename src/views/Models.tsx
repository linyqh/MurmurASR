import { useMemo, useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { ask } from "@tauri-apps/plugin-dialog";
import { api, modelSize, type CatalogModel } from "../lib/api";
import { useStore, type DownloadProgress } from "../lib/store";
import { formatBytes, formatEta } from "../lib/format";
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

/** Whether every whitespace-separated term of `query` appears in the model's text. */
function matches(model: CatalogModel, query: string): boolean {
  const text = [model.name, model.family, model.languages, model.summary, model.description, model.license, ...model.tags]
    .join(" ")
    .toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((term) => text.includes(term));
}

export function ModelsView() {
  const { models, downloads, settings, saveSettings } = useStore();
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const installed = useMemo(() => new Set(models?.installed.map((m) => m.id)), [models?.installed]);
  const visible = useMemo(
    () =>
      (models?.catalog.models ?? []).filter(
        (m) => m.kind !== "aligner" && (filter === "all" || installed.has(m.id)) && matches(m, query),
      ),
    [models?.catalog.models, filter, installed, query],
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
        <div className="toolbar-title">模型</div>
        {models.installed.length > 0 && (
          <div className="toolbar-subtitle">
            已安装 {models.installed.length} 个 · {formatBytes(totalInstalled)}
          </div>
        )}
        <div className="toolbar-spacer" />
        <div className="search-field model-search no-drag">
          <IconSearch size={13} />
          <input
            placeholder="搜索名称、语言或特点"
            aria-label="搜索模型"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <div className="no-drag">
          <Segmented
            label="筛选"
            value={filter}
            onChange={setFilter}
            options={[
              { value: "all", label: "全部" },
              { value: "installed", label: "已安装" },
            ]}
          />
        </div>
      </header>
      <div className="page">
        <p className="page-intro muted">
          共 {models.catalog.models.filter((m) => m.kind !== "aligner").length} 个模型，按需下载，保存在本机，识别过程完全离线。首次使用推荐下载
          <strong> {recommended?.name}</strong>，它在速度、准确率和语言覆盖之间取得了很好的平衡。可以搜索“中文”“粤语”“时间戳”等关键词快速找到合适的模型。
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
              {g.name} <span className="muted">{g.items.length}</span>
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
            <p>{query ? `没有找到与“${query}”相关的模型。` : "还没有安装任何模型。"}</p>
          </div>
        )}

        <div className="source-row">
          <span className="muted">下载源</span>
          <Segmented
            label="下载源"
            value={settings.downloadSource}
            onChange={(downloadSource) => void saveSettings({ downloadSource })}
            options={[
              { value: "auto", label: "自动" },
              { value: "huggingface", label: "Hugging Face" },
              { value: "modelscope", label: "ModelScope" },
              { value: "hfmirror", label: "HF 镜像" },
            ]}
          />
          <span className="muted source-hint">国内网络建议选择 ModelScope 或 HF 镜像</span>
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
      toast(String(e), "error");
    }
  };

  const remove = async () => {
    const ok = await ask(`删除“${model.name}”将释放 ${formatBytes(modelSize(model))} 空间，之后可重新下载。`, {
      title: "删除模型？",
      kind: "warning",
      okLabel: "删除",
      cancelLabel: "取消",
    });
    if (!ok) return;
    try {
      await api.deleteModel(model.id);
      await refreshModels();
    } catch (e) {
      toast(String(e), "error");
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
          <IconSparkle size={12} /> 推荐
        </div>
      )}
      <div className="model-head">
        <div className="model-name">
          {model.name}
          <span className="badge">{model.precision}</span>
          {loaded && <span className="badge green">使用中</span>}
        </div>
        <div className="model-size muted tabular">{formatBytes(modelSize(model))}</div>
      </div>
      <p className="model-summary">{model.summary}</p>
      <p className="model-description">{model.description}</p>
      <div className="model-facts">
        <div>
          <span className="muted">语言</span>
          {model.languages}
        </div>
        <div>
          <span className="muted">速度</span>
          <Meter level={model.speed} />
        </div>
        <div>
          <span className="muted">准确率</span>
          <Meter level={model.accuracy} />
        </div>
      </div>
      <div className="model-capabilities">
        <CapabilityBadge supported={model.wordTimestamps} label="词级时间戳" />
        <CapabilityBadge supported={model.speakerDiarization} label="说话人分段" />
      </div>
      {aligner && <AlignerAddon aligner={aligner} owner={model} />}
      <div className="model-tags">
        {model.tags.map((t) => (
          <span key={t} className="badge">
            {t}
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
                <IconCheckCircle size={15} /> 已加载
                {models?.loaded && (
                  <span className="muted">
                    {" "}
                    · {models.loaded.backend.toUpperCase()} · {(models.loaded.loadMs / 1000).toFixed(1)} 秒
                  </span>
                )}
              </span>
            ) : (
              <button
                className="btn primary"
                disabled={loading || !!models?.loading || !!info?.engineError}
                onClick={() => void api.loadModel(model.id).catch((e) => toast(String(e), "error"))}
              >
                {loading ? (
                  <>
                    <span className="spinner light" /> 加载中…
                  </>
                ) : (
                  "使用此模型"
                )}
              </button>
            )}
            <div className="toolbar-spacer" />
            <button className="icon-btn" aria-label="在访达中显示" onClick={reveal}>
              <IconFolder size={15} />
            </button>
            <button className="icon-btn" aria-label="删除模型" onClick={() => void remove()}>
              <IconTrash size={15} />
            </button>
          </>
        ) : (
          <button className={`btn${featured ? " primary" : ""}`} onClick={() => void startDownload()}>
            <IconDownload size={14} /> 下载
          </button>
        )}
      </div>
    </article>
  );
}

/** Optional companion download that adds word timestamps to `owner`. */
function AlignerAddon({ aligner, owner }: { aligner: CatalogModel; owner: CatalogModel }) {
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
      toast(String(e), "error");
    }
  };

  const remove = async () => {
    const ok = await ask(`删除后 ${owner.name} 将不再输出词级时间戳，可随时重新下载。`, {
      title: `删除“${aligner.name}”？`,
      kind: "warning",
      okLabel: "删除",
      cancelLabel: "取消",
    });
    if (ok) await run(() => api.deleteModel(aligner.id));
  };

  return (
    <div className="aligner-addon">
      <div className="aligner-line">
        <span>
          <strong>{aligner.name}</strong>
          <span className="muted"> · {formatBytes(modelSize(aligner))} · {aligner.summary}</span>
        </span>
        <div className="toolbar-spacer" />
        {downloading ? null : installed ? (
          <>
            {ownerLoaded && !active ? (
              <button className="btn" onClick={() => void run(() => api.loadModel(owner.id))}>
                重新加载以启用
              </button>
            ) : (
              <span className="aligner-state muted">
                <IconCheck size={12} /> 已安装
              </span>
            )}
            <button className="icon-btn" aria-label="删除对齐模型" onClick={() => void remove()}>
              <IconTrash size={14} />
            </button>
          </>
        ) : (
          <button className="btn" onClick={() => void run(() => api.downloadModel(aligner.id))}>
            <IconDownload size={13} /> 下载
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
  const fraction = download ? download.downloaded / Math.max(1, download.total) : 0;
  return (
    <div className="download-status">
      <div className="download-line">
        <span className="tabular">
          {download?.verifying
            ? "正在校验文件…"
            : download
              ? `${formatBytes(download.downloaded)} / ${formatBytes(download.total)}`
              : "正在连接…"}
        </span>
        {download && !download.verifying && (
          <span className="muted tabular">
            {formatBytes(download.bytesPerSec)}/s · {formatEta(download.total - download.downloaded, download.bytesPerSec)}
            {download.source && ` · ${download.source}`}
          </span>
        )}
        <div className="toolbar-spacer" />
        <button className="icon-btn" aria-label="取消下载" onClick={() => void api.cancelDownload(model.id)}>
          <IconX size={14} />
        </button>
      </div>
      <Progress value={fraction} indeterminate={!download || download.verifying} />
    </div>
  );
}
