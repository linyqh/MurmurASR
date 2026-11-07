import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import { api, type ExportFormat } from "../lib/api";
import { useStore, AUDIO_EXTENSIONS, type Job } from "../lib/store";
import { formatClock, formatDuration, joinSegments } from "../lib/format";
import { errorMessage } from "../lib/errors";
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

export function HomeView() {
  const { t } = useTranslation("home");
  const { models, jobs, transcribeFiles, setView } = useStore();
  const ready = !!models?.loaded;
  // Newest first, so a fresh upload lands right under the picker.
  const jobList = Object.values(jobs).reverse();

  const pick = async () => {
    const selected = await open({
      multiple: true,
      title: t("dialog.pickTitle"),
      filters: [{ name: t("dialog.filterName"), extensions: AUDIO_EXTENSIONS }],
    });
    if (!selected) return;
    void transcribeFiles(Array.isArray(selected) ? selected : [selected]);
  };

  return (
    <>
      <header className="toolbar drag" data-tauri-drag-region>
        <div className="toolbar-title">{t("toolbar.title")}</div>
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
            <h2>{models?.loading ? t("empty.loadingTitle") : t("empty.noModelTitle")}</h2>
            <p>
              {models?.loading ? t("empty.loadingText") : t("empty.noModelText")}
            </p>
            {!models?.loading && (
              <button className="btn primary large" onClick={() => setView({ name: "models" })}>
                {t("empty.goToModels")}
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
                  {jobList.length > 0 ? t("uploader.pickMore") : t("uploader.pickFirst")}
                </span>
                <span className="uploader-hint">
                  {t("uploader.hint", { formats: t("uploader.formats") })}
                </span>
              </span>
              <span className="btn primary uploader-cta">{t("uploader.cta")}</span>
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
  const { t } = useTranslation("home");
  const steps = [
    { icon: <IconUpload size={18} />, title: t("steps.upload.title"), text: t("steps.upload.text") },
    { icon: <IconWaveform size={18} />, title: t("steps.recognize.title"), text: t("steps.recognize.text") },
    { icon: <IconDownload size={18} />, title: t("steps.download.title"), text: t("steps.download.text") },
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
      <p className="steps-note muted">{t("steps.note")}</p>
    </div>
  );
}

function JobCard({ job }: { job: Job }) {
  if (job.phase === "done") return <ResultCard job={job} />;
  if (job.phase === "failed") return <FailedCard job={job} />;
  return <ProgressCard job={job} />;
}

function ProgressCard({ job }: { job: Job }) {
  const { t } = useTranslation("home");
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
            {t(`phase.${job.phase}`)}
            {job.phase !== "queued" && ` ${Math.round(job.progress * 100)}%`}
            {job.duration > 0 && ` · ${t("job.audioLength", { duration: formatClock(job.duration) })}`}
          </div>
        </div>
        <button className="btn plain" onClick={() => void api.cancelJob(job.id)}>
          {t("job.cancel")}
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
  const { t } = useTranslation(["home", "common"]);
  const { dismissJob } = useStore();
  return (
    <div className="job card">
      <div className="job-head">
        <span className="job-badge failed">
          <IconWarning size={15} />
        </span>
        <div className="job-name">
          <div className="job-title">{job.title}</div>
          <div className="job-phase error-text">{t("job.failed", { error: job.error })}</div>
        </div>
        <button className="icon-btn" aria-label={t("common:actions.close")} onClick={() => dismissJob(job.id)}>
          <IconX size={14} />
        </button>
      </div>
    </div>
  );
}

function ResultCard({ job }: { job: Job }) {
  const { t } = useTranslation(["home", "common"]);
  const downloads: { format: ExportFormat; label: string }[] = [
    { format: "srt", label: t("result.downloadSrt") },
    { format: "json", label: t("result.downloadJson") },
  ];
  const { dismissJob, setView, toast } = useStore();
  const [expanded, setExpanded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [words, setWords] = useState<number | null>(null);

  // The engine streams segments without the final cleanup, so show the saved copy.
  const [segments, setSegments] = useState(job.segments);
  useEffect(() => {
    api.getTranscript(job.id).then(
      (tr) => {
        setSegments(tr.segments);
        setWords(tr.segments.reduce((n, s) => n + (s.words?.length ?? 0), 0));
      },
      () => undefined,
    );
  }, [job.id]);

  const text = joinSegments(segments.map((s) => s.text));
  const shown = expanded ? segments : segments.slice(0, 6);

  const download = async (formats: ExportFormat[]) => {
    const dir = await open({ directory: true, title: t("dialog.saveTitle") });
    if (typeof dir !== "string") return;
    setBusy(true);
    try {
      const paths = await api.exportFiles(job.id, dir, formats);
      const names = paths.map((p) => p.split(/[\\/]/).pop()).join(t("result.nameSeparator"));
      toast(t("result.saved", { names }));
      void revealItemInDir(paths[0]);
    } catch (e) {
      toast(t("result.saveFailed", { error: errorMessage(e) }), "error");
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
            {t("result.done")} · {t("result.audioLength", { duration: formatDuration(job.duration) })} ·{" "}
            {t("result.segments", { count: segments.length })}
            {words ? ` · ${t("result.wordTimestamps", { count: words })}` : ""}
            {job.speed ? ` · ${t("result.speed", { speed: job.speed.toFixed(0) })}` : ""}
          </div>
        </div>
        <button className="icon-btn" aria-label={t("result.dismiss")} title={t("result.dismissTitle")} onClick={() => dismissJob(job.id)}>
          <IconX size={14} />
        </button>
      </div>

      <div className="result-actions">
        <button className="btn primary" disabled={busy} onClick={() => void download(["srt", "json"])}>
          <IconDownload size={14} />
          {t("result.downloadBoth")}
        </button>
        {downloads.map((d) => (
          <button key={d.format} className="btn" disabled={busy} onClick={() => void download([d.format])}>
            {d.label}
          </button>
        ))}
        <div className="toolbar-spacer" />
        <button
          className="btn plain"
          onClick={() => void writeText(text).then(() => toast(t("common:actions.copied")))}
        >
          <IconCopy size={14} />
          {t("result.copyAll")}
        </button>
        <button className="btn plain" onClick={() => setView({ name: "transcript", id: job.id })}>
          <IconDoc size={14} />
          {t("result.openTranscript")}
        </button>
      </div>

      {segments.length === 0 ? (
        <p className="muted result-empty">{t("result.noSpeech")}</p>
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
              {expanded ? t("result.collapse") : t("result.expand", { count: segments.length })}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
