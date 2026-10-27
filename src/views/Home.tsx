import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, type ExportFormat } from "../lib/api";
import { useStore, AUDIO_EXTENSIONS, type Job } from "../lib/store";
import { formatClock, formatDuration, joinSegments } from "../lib/format";
import { Progress } from "../components/controls";
import {
  IconCheckCircle,
  IconCopy,
  IconCube,
  IconDoc,
  IconDownload,
  IconUpload,
  IconWarning,
  IconWaveform,
  IconX,
} from "../components/icons";
import { LanguagePicker } from "./LanguagePicker";

const PHASE_LABEL: Record<Job["phase"], string> = {
  queued: "排队中",
  fetchingDecoder: "下载解码器",
  decoding: "解码音频",
  detecting: "检测语音",
  transcribing: "识别中",
  done: "已完成",
  failed: "失败",
};

const FORMATS = "MP3、M4A、WAV、FLAC、OGG、MP4、MOV 等";

export function HomeView() {
  const { models, jobs, transcribeFiles, setView } = useStore();
  const ready = !!models?.loaded;
  // Newest first, so a fresh upload lands right under the picker.
  const jobList = Object.values(jobs).reverse();

  const pick = async () => {
    const selected = await open({
      multiple: true,
      title: "选择要转写的音频或视频",
      filters: [{ name: "音频与视频", extensions: AUDIO_EXTENSIONS }],
    });
    if (!selected) return;
    void transcribeFiles(Array.isArray(selected) ? selected : [selected]);
  };

  return (
    <>
      <header className="toolbar drag" data-tauri-drag-region>
        <div className="toolbar-title">转写文件</div>
        <div className="toolbar-spacer" />
        <div className="no-drag">
          <LanguagePicker />
        </div>
      </header>
      <div className="page">
        {!ready ? (
          <div className="empty">
            <div className="empty-icon">
              <IconCube size={30} />
            </div>
            <h2>{models?.loading ? "正在加载模型…" : "还没有可用的模型"}</h2>
            <p>
              {models?.loading
                ? "模型加载完成后即可开始转写。"
                : "Murmur 在本机完成所有识别，需要先下载一个语音识别模型。"}
            </p>
            {!models?.loading && (
              <button className="btn primary large" onClick={() => setView({ name: "models" })}>
                前往下载模型
              </button>
            )}
          </div>
        ) : (
          <div className="home-flow">
            <button className="uploader" onClick={() => void pick()}>
              <span className="uploader-icon">
                <IconUpload size={20} />
              </span>
              <span className="uploader-body">
                <span className="uploader-title">
                  {jobList.length > 0 ? "继续添加音频或视频" : "选择音频或视频文件"}
                </span>
                <span className="uploader-hint">也可以直接拖到窗口里 · 支持 {FORMATS}格式</span>
              </span>
              <span className="btn primary uploader-cta">选择文件</span>
            </button>

            {jobList.length === 0 ? (
              <Steps />
            ) : (
              jobList.map((job) => <JobCard key={job.id} job={job} />)
            )}
          </div>
        )}
      </div>
    </>
  );
}

function Steps() {
  const steps = [
    { icon: <IconUpload size={18} />, title: "上传", text: "选择或拖入文件，可一次添加多个" },
    { icon: <IconWaveform size={18} />, title: "识别", text: "在本机完成，边识别边显示文字" },
    { icon: <IconDownload size={18} />, title: "下载", text: "完成后直接下载 SRT 字幕和 JSON 数据" },
  ];
  return (
    <div className="steps">
      {steps.map((s, i) => (
        <div className="step" key={s.title}>
          <span className="step-icon">{s.icon}</span>
          <div>
            <div className="step-title">
              <span className="step-num tabular">{i + 1}</span>
              {s.title}
            </div>
            <div className="step-text">{s.text}</div>
          </div>
        </div>
      ))}
      <p className="steps-note muted">所有识别都在本机完成，音频不会离开这台设备。结果会自动保存到左侧记录。</p>
    </div>
  );
}

function JobCard({ job }: { job: Job }) {
  if (job.phase === "done") return <ResultCard job={job} />;
  if (job.phase === "failed") return <FailedCard job={job} />;
  return <ProgressCard job={job} />;
}

function ProgressCard({ job }: { job: Job }) {
  const indeterminate = job.phase === "queued" || (job.phase === "decoding" && job.progress === 0);
  return (
    <div className="job card">
      <div className="job-head">
        <span className="job-badge running">
          <IconWaveform size={15} />
        </span>
        <div className="job-name">
          <div className="job-title">{job.title}</div>
          <div className="job-phase muted tabular">
            {PHASE_LABEL[job.phase]}
            {job.phase !== "queued" && ` ${Math.round(job.progress * 100)}%`}
            {job.duration > 0 && ` · 音频 ${formatClock(job.duration)}`}
          </div>
        </div>
        <button className="btn plain" onClick={() => void api.cancelJob(job.id)}>
          取消
        </button>
      </div>
      <Progress value={job.progress} indeterminate={indeterminate} />
      {job.segments.length > 0 && (
        <div className="job-live">
          {job.segments.slice(-4).map((s) => (
            <div className="job-line" key={s.index}>
              <span className="tabular">{formatClock(s.start)}</span>
              <span>{s.text}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function FailedCard({ job }: { job: Job }) {
  const { dismissJob } = useStore();
  return (
    <div className="job card">
      <div className="job-head">
        <span className="job-badge failed">
          <IconWarning size={15} />
        </span>
        <div className="job-name">
          <div className="job-title">{job.title}</div>
          <div className="job-phase error-text">转写失败：{job.error}</div>
        </div>
        <button className="icon-btn" aria-label="关闭" onClick={() => dismissJob(job.id)}>
          <IconX size={14} />
        </button>
      </div>
    </div>
  );
}

const DOWNLOADS: { format: ExportFormat; label: string }[] = [
  { format: "srt", label: "SRT 字幕" },
  { format: "json", label: "JSON 数据" },
];

function ResultCard({ job }: { job: Job }) {
  const { dismissJob, setView, toast } = useStore();
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [words, setWords] = useState<number | null>(null);

  // The engine streams segments without the final cleanup, so show the saved copy.
  const [segments, setSegments] = useState(job.segments);
  useEffect(() => {
    api.getTranscript(job.id).then(
      (t) => {
        setSegments(t.segments);
        setWords(t.segments.reduce((n, s) => n + (s.words?.length ?? 0), 0));
      },
      () => undefined,
    );
  }, [job.id]);

  const text = joinSegments(segments.map((s) => s.text));
  const shown = expanded ? segments : segments.slice(0, 6);

  const download = async (formats: ExportFormat[]) => {
    const dir = await open({ directory: true, title: "选择保存位置" });
    if (typeof dir !== "string") return;
    setBusy(true);
    try {
      const paths = await api.exportFiles(job.id, dir, formats);
      const names = paths.map((p) => p.split(/[\\/]/).pop()).join("、");
      toast(`已保存 ${names}`);
      void revealItemInDir(paths[0]);
    } catch (e) {
      toast(`保存失败：${String(e)}`, "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="job card result">
      <div className="job-head">
        <span className="job-badge done">
          <IconCheckCircle size={16} />
        </span>
        <div className="job-name">
          <div className="job-title">{job.title}</div>
          <div className="job-phase muted tabular">
            转写完成 · 音频 {formatDuration(job.duration)} · {segments.length} 段
            {words ? ` · ${words} 个词级时间戳` : ""}
            {job.speed ? ` · ${job.speed.toFixed(0)}× 实时` : ""}
          </div>
        </div>
        <button className="icon-btn" aria-label="从首页移除" title="从首页移除（记录仍保留）" onClick={() => dismissJob(job.id)}>
          <IconX size={14} />
        </button>
      </div>

      <div className="result-actions">
        <button className="btn primary" disabled={busy} onClick={() => void download(["srt", "json"])}>
          <IconDownload size={14} />
          下载 SRT + JSON
        </button>
        {DOWNLOADS.map((d) => (
          <button key={d.format} className="btn" disabled={busy} onClick={() => void download([d.format])}>
            {d.label}
          </button>
        ))}
        <div className="toolbar-spacer" />
        <button
          className="btn plain"
          onClick={() => void writeText(text).then(() => toast("已复制到剪贴板"))}
        >
          <IconCopy size={14} />
          复制全文
        </button>
        <button className="btn plain" onClick={() => setView({ name: "transcript", id: job.id })}>
          <IconDoc size={14} />
          打开记录
        </button>
      </div>

      {segments.length === 0 ? (
        <p className="muted result-empty">没有识别到语音内容。</p>
      ) : (
        <div className="result-text selectable">
          {shown.map((s) => (
            <div className="job-line" key={s.index}>
              <span className="tabular">{formatClock(s.start)}</span>
              <span>{s.text}</span>
            </div>
          ))}
          {segments.length > 6 && (
            <button className="link-btn" onClick={() => setExpanded((e) => !e)}>
              {expanded ? "收起" : `展开全部 ${segments.length} 段`}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
