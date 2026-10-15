import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { save, ask } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, type ExportFormat, type Segment, type Transcript } from "../lib/api";
import { useStore } from "../lib/store";
import {
  activeWord,
  alignWords,
  formatClock,
  formatDuration,
  formatRelativeDate,
  joinSegments,
  languageLabel,
} from "../lib/format";
import {
  IconCopy,
  IconDownload,
  IconFolder,
  IconPause,
  IconPencil,
  IconPlay,
  IconShare,
  IconTrash,
} from "../components/icons";

const EXPORTS: { format: ExportFormat; label: string; ext: string; hint: string }[] = [
  { format: "txt", label: "纯文本", ext: "txt", hint: ".txt" },
  { format: "srt", label: "SRT 字幕", ext: "srt", hint: ".srt" },
  { format: "vtt", label: "WebVTT 字幕", ext: "vtt", hint: ".vtt" },
  { format: "md", label: "Markdown", ext: "md", hint: ".md" },
  { format: "json", label: "JSON", ext: "json", hint: ".json" },
];

export function TranscriptView({ id }: { id: string }) {
  const { setView, toast } = useStore();
  const [t, setT] = useState<Transcript | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [current, setCurrent] = useState(-1);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [editing, setEditing] = useState<number | null>(null);
  const [menu, setMenu] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const audio = useRef<HTMLAudioElement>(null);
  const segRefs = useRef<(HTMLDivElement | null)[]>([]);

  useEffect(() => {
    api.getTranscript(id).then(setT, (e) => setError(String(e)));
  }, [id]);

  const audioSrc = useMemo(() => (t?.audioPath ? convertFileSrc(t.audioPath) : null), [t?.audioPath]);

  useEffect(() => {
    if (!t) return;
    const idx = t.segments.findIndex((s) => position >= s.start && position < s.end);
    if (idx !== current) {
      setCurrent(idx);
      if (idx >= 0 && playing && editing === null) {
        segRefs.current[idx]?.scrollIntoView({ block: "nearest", behavior: "smooth" });
      }
    }
  }, [position, t, current, playing, editing]);

  useEffect(() => {
    const close = () => setMenu(false);
    if (menu) window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [menu]);

  if (error) {
    return (
      <div className="empty">
        <h2>无法打开记录</h2>
        <p>{error}</p>
      </div>
    );
  }
  if (!t) return null;

  const fullText = joinSegments(t.segments.map((s) => s.text));
  const hasWordTimestamps = t.segments.some((s) => s.words?.length);

  const seek = (seconds: number) => {
    const el = audio.current;
    if (!el) return;
    el.currentTime = seconds;
    void el.play();
  };

  const togglePlay = () => {
    const el = audio.current;
    if (!el) return;
    if (el.paused) void el.play();
    else el.pause();
  };

  const saveSegment = async (index: number, text: string) => {
    setEditing(null);
    const trimmed = text.trim();
    if (trimmed === t.segments[index].text) return;
    // Old word timings no longer match edited text.
    const segments = t.segments.map((s, i) => (i === index ? { ...s, text: trimmed, words: undefined } : s));
    try {
      setT(await api.updateTranscript(t.id, { segments }));
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const rename = async (title: string) => {
    setRenaming(false);
    if (!title.trim() || title === t.title) return;
    setT(await api.updateTranscript(t.id, { title }));
  };

  const doExport = async (format: ExportFormat, ext: string) => {
    if (exporting) return;
    setMenu(false);
    setExporting(true);
    try {
      const path = await save({
        title: "导出转写",
        defaultPath: `${t.title}.${ext}`,
        filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
      });
      if (!path) return;
      await api.exportTranscript(t.id, format, path);
      toast(`已导出 ${ext.toUpperCase()} 文件`);
    } catch (e) {
      toast(String(e), "error");
    } finally {
      setExporting(false);
    }
  };

  const remove = async () => {
    const ok = await ask("此操作无法撤销。", {
      title: `删除“${t.title}”？`,
      kind: "warning",
      okLabel: "删除",
      cancelLabel: "取消",
    });
    if (!ok) return;
    await api.deleteTranscript(t.id);
    setView({ name: "home" });
  };

  return (
    <>
      <header className="toolbar drag" data-tauri-drag-region>
        {renaming ? (
          <input
            className="title-input no-drag"
            defaultValue={t.title}
            autoFocus
            onBlur={(e) => void rename(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
              if (e.key === "Escape") setRenaming(false);
            }}
          />
        ) : (
          <div className="toolbar-title" onDoubleClick={() => setRenaming(true)} title="双击重命名">
            {t.title}
          </div>
        )}
        <div className="toolbar-spacer" />
        <div className="glass glass-group no-drag">
          <button
            className="icon-btn"
            aria-label="复制全文"
            onClick={() => void writeText(fullText).then(() => toast("已复制到剪贴板"))}
          >
            <IconCopy size={15} />
          </button>
          <div className="menu-anchor">
            <button
              className="icon-btn"
              aria-label="导出"
              disabled={exporting}
              aria-haspopup="menu"
              onClick={(e) => {
                e.stopPropagation();
                setMenu((m) => !m);
              }}
            >
              <IconShare size={15} />
            </button>
            {menu && (
              <div className="menu glass" role="menu" style={{ right: 0, top: 36 }}>
                {EXPORTS.map((x) => (
                  <button key={x.format} role="menuitem" onClick={() => void doExport(x.format, x.ext)}>
                    导出为 {x.label}
                    <span className="shortcut">{x.hint}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
          <button className="icon-btn" aria-label="删除" onClick={() => void remove()}>
            <IconTrash size={15} />
          </button>
        </div>
      </header>

      <div className="page transcript-page">
        <div className="transcript-meta muted">
          {formatRelativeDate(t.createdAt)} · {formatDuration(t.duration)}
          {t.language && ` · ${languageLabel(t.language)}`}
          {t.model && ` · ${t.model}`}
          {t.elapsedMs > 0 && t.duration > 0 && ` · 用时 ${(t.elapsedMs / 1000).toFixed(1)} 秒`}
        </div>

        <div className="transcript-downloads">
          <button className="btn primary" disabled={exporting} onClick={() => void doExport("srt", "srt")}>
            <IconDownload size={14} />
            下载 SRT 字幕文件
          </button>
          <button className="btn" disabled={exporting} onClick={() => void doExport("json", "json")}>
            <IconDownload size={14} />
            下载词级时间戳 JSON 文件
          </button>
          {!hasWordTimestamps && (
            <p className="transcript-download-note muted">
              这条记录未保存词级时间戳，JSON 将包含分段时间戳和转写文本。
            </p>
          )}
        </div>

        {t.segments.length === 0 ? (
          <div className="empty compact">
            <p>没有识别到语音内容。</p>
          </div>
        ) : (
          <div className="segments">
            {t.segments.map((s, i) => (
              <div
                key={i}
                ref={(el) => {
                  segRefs.current[i] = el;
                }}
                className={`segment${i === current && (playing || position > 0) ? " current" : ""}`}
              >
                <button
                  className="segment-time tabular"
                  onClick={() => seek(s.start)}
                  disabled={!audioSrc}
                  title={audioSrc ? "从这里播放" : undefined}
                >
                  {formatClock(s.start)}
                </button>
                {editing === i ? (
                  <textarea
                    className="segment-edit"
                    defaultValue={s.text}
                    autoFocus
                    rows={Math.max(1, Math.ceil(s.text.length / 60))}
                    onBlur={(e) => void saveSegment(i, e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && !e.shiftKey) {
                        e.preventDefault();
                        (e.target as HTMLTextAreaElement).blur();
                      }
                      if (e.key === "Escape") setEditing(null);
                    }}
                  />
                ) : (
                  <div className="segment-text selectable" onDoubleClick={() => setEditing(i)}>
                    {s.words?.length ? (
                      <SegmentText
                        segment={s}
                        time={i === current && (playing || position > 0) ? position : null}
                        onSeek={audioSrc ? seek : undefined}
                      />
                    ) : (
                      s.text
                    )}
                  </div>
                )}
                {editing !== i && (
                  <button className="icon-btn segment-edit-btn" aria-label="编辑" onClick={() => setEditing(i)}>
                    <IconPencil size={13} />
                  </button>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      {audioSrc && (
        <div className="player glass">
          <button className="player-btn" onClick={togglePlay} aria-label={playing ? "暂停" : "播放"}>
            {playing ? <IconPause size={16} /> : <IconPlay size={16} />}
          </button>
          <span className="tabular player-time">{formatClock(position)}</span>
          <input
            className="player-scrub"
            type="range"
            min={0}
            max={Math.max(1, t.duration)}
            step={0.1}
            value={position}
            aria-label="播放进度"
            onChange={(e) => {
              const v = Number(e.target.value);
              setPosition(v);
              if (audio.current) audio.current.currentTime = v;
            }}
            style={{ ["--fill" as string]: `${(position / Math.max(1, t.duration)) * 100}%` }}
          />
          <span className="tabular player-time muted">{formatClock(t.duration)}</span>
          <button
            className="icon-btn"
            aria-label="在访达中显示音频"
            onClick={() => t.audioPath && void revealItemInDir(t.audioPath)}
          >
            <IconFolder size={14} />
          </button>
          <audio
            ref={audio}
            src={audioSrc}
            preload="metadata"
            onPlay={() => setPlaying(true)}
            onPause={() => setPlaying(false)}
            onEnded={() => setPlaying(false)}
            onTimeUpdate={(e) => setPosition(e.currentTarget.currentTime)}
            onError={() => toast("无法播放音频文件（文件可能已移动或删除）", "error")}
          />
        </div>
      )}
    </>
  );
}

function SegmentText({
  segment,
  time,
  onSeek,
}: {
  segment: Segment;
  time: number | null;
  onSeek?: (seconds: number) => void;
}) {
  const words = segment.words ?? [];
  const pieces = useMemo(() => alignWords(segment.text, words), [segment.text, words]);
  const active = time === null ? -1 : activeWord(words, time);
  return (
    <>
      {pieces.map((p, k) =>
        p.word < 0 ? (
          p.text
        ) : (
          <span
            key={k}
            className={`word${p.word === active ? " active" : ""}${onSeek ? " seekable" : ""}`}
            title={formatClock(words[p.word].start)}
            onClick={onSeek && (() => onSeek(words[p.word].start))}
          >
            {p.text}
          </span>
        ),
      )}
    </>
  );
}
