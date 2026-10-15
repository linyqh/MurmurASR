import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { useStore, AUDIO_EXTENSIONS, type Job } from "../lib/store";
import { formatClock, joinSegments } from "../lib/format";
import { Progress } from "../components/controls";
import { IconCube, IconUpload, IconWaveform, IconX } from "../components/icons";
import { LanguagePicker } from "./LanguagePicker";

const PHASE_LABEL: Record<Job["phase"], string> = {
  queued: "排队中",
  decoding: "解码音频",
  detecting: "检测语音",
  transcribing: "识别中",
};

export function HomeView() {
  const { models, jobs, transcribeFiles, setView } = useStore();
  const ready = !!models?.loaded;
  const jobList = Object.values(jobs);

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
      <div className={`page${ready && jobList.length === 0 ? " home-page" : ""}`}>
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
          <>
            <button className={`dropzone${jobList.length > 0 ? " compact" : ""}`} onClick={pick}>
              <div className="dropzone-icon">
                <IconUpload size={jobList.length > 0 ? 18 : 30} />
              </div>
              <div className="dropzone-body">
                <div className="dropzone-title">
                  {jobList.length > 0 ? "继续添加文件" : "拖入音频或视频文件，或点按选取"}
                </div>
                <div className="dropzone-hint">
                  支持 MP3、M4A、WAV、FLAC、OGG、MP4、MOV 等格式，所有处理均在本机完成
                </div>
              </div>
            </button>

            {jobList.length > 0 && (
              <section className="jobs">
                <h3 className="section-title">正在处理</h3>
                {jobList.map((job) => (
                  <JobCard key={job.id} job={job} />
                ))}
              </section>
            )}
          </>
        )}
      </div>
    </>
  );
}

function JobCard({ job }: { job: Job }) {
  const preview = joinSegments(job.segments.slice(-3).map((s) => s.text));
  const indeterminate = job.phase === "queued" || (job.phase === "decoding" && job.progress === 0);
  return (
    <div className="job card">
      <div className="job-head">
        <IconWaveform size={16} className="job-icon" />
        <div className="job-title">{job.title}</div>
        <div className="job-phase muted tabular">
          {PHASE_LABEL[job.phase]}
          {job.phase !== "queued" && ` ${Math.round(job.progress * 100)}%`}
          {job.duration > 0 && ` · ${formatClock(job.duration)}`}
        </div>
        <button className="icon-btn" aria-label="取消" onClick={() => void api.cancelJob(job.id)}>
          <IconX size={14} />
        </button>
      </div>
      <Progress value={job.progress} indeterminate={indeterminate} />
      {preview && <div className="job-preview">{preview}</div>}
    </div>
  );
}
