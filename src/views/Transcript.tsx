import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { convertFileSrc } from "@tauri-apps/api/core";
import { save, ask } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, type ExportFormat, type Segment, type Transcript } from "../lib/api";
import { useStore } from "../lib/store";
import { errorMessage } from "../lib/errors";
import { Segmented } from "../components/controls";
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

const EXPORTS: { format: ExportFormat; ext: string; hint: string }[] = [
  { format: "txt", ext: "txt", hint: ".txt" },
  { format: "srt", ext: "srt", hint: ".srt" },
  { format: "vtt", ext: "vtt", hint: ".vtt" },
  { format: "md", ext: "md", hint: ".md" },
  { format: "json", ext: "json", hint: ".json" },
];

/** Issue codes reported by the backend that have a translated label. */
const SUBTITLE_ISSUES = ["segment_timing", "reading_speed", "line_length", "short_duration", "long_duration"] as const;
type SubtitleIssue = (typeof SUBTITLE_ISSUES)[number];

function isSubtitleIssue(issue: string): issue is SubtitleIssue {
  return (SUBTITLE_ISSUES as readonly string[]).includes(issue);
}

export function TranscriptView({ id }: { id: string }) {
  // `t` is the transcript state below, so the translate function is `tr`.
  const { t: tr } = useTranslation(["transcript", "common"]);
  const { setView, toast, settings, saveSettings } = useStore();
  const [t, setT] = useState<Transcript | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [current, setCurrent] = useState(-1);
  const [playing, setPlaying] = useState(false);
  const [position, setPosition] = useState(0);
  const [editing, setEditing] = useState<number | null>(null);
  const [display, setDisplay] = useState<"subtitles" | "original">("subtitles");
  const [menu, setMenu] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const audio = useRef<HTMLAudioElement>(null);
  const segRefs = useRef<(HTMLDivElement | null)[]>([]);

  useEffect(() => {
    let active = true;
    api.getTranscript(id).then(
      (transcript) => { if (active) { setT(transcript); setError(null); } },
      (e) => { if (active) setError(errorMessage(e)); },
    );
    return () => { active = false; };
  }, [id, settings?.subtitleMaxLines]);

  const audioSrc = useMemo(() => (t?.audioPath ? convertFileSrc(t.audioPath) : null), [t?.audioPath]);
  const visibleSegments = useMemo(() => t ? (display === "subtitles" ? t.subtitles : t.segments) : [], [t, display]);

  useEffect(() => {
    if (!t) return;
    const idx = visibleSegments.findIndex((s) => position >= s.start && position < s.end);
    if (idx !== current) {
      setCurrent(idx);
      if (idx >= 0 && playing && editing === null) {
        segRefs.current[idx]?.scrollIntoView({ block: "nearest", behavior: "smooth" });
      }
    }
  }, [position, t, visibleSegments, current, playing, editing]);

  useEffect(() => {
    const close = () => setMenu(false);
    if (menu) window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [menu]);

  if (error) {
    return (
      <div className="empty">
        <h2>{tr("transcript:error.title")}</h2>
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
      toast(errorMessage(e), "error");
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
        title: tr("transcript:export.dialogTitle"),
        defaultPath: `${t.title}.${ext}`,
        filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
      });
      if (!path) return;
      await api.exportTranscript(t.id, format, path);
      toast(tr("transcript:export.done", { format: ext.toUpperCase() }));
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      setExporting(false);
    }
  };

  const remove = async () => {
    const ok = await ask(tr("transcript:deleteDialog.message"), {
      title: tr("transcript:deleteDialog.title", { title: t.title }),
      kind: "warning",
      okLabel: tr("common:actions.delete"),
      cancelLabel: tr("common:actions.cancel"),
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
          <div className="toolbar-title" onDoubleClick={() => setRenaming(true)} title={tr("transcript:rename.hint")}>
            {t.title}
          </div>
        )}
        <div className="toolbar-spacer" />
        <div className="glass glass-group no-drag">
          <button
            className="icon-btn"
            aria-label={tr("transcript:toolbar.copyAll")}
            onClick={() => void writeText(fullText).then(() => toast(tr("common:actions.copied")))}
          >
            <IconCopy size={15} />
          </button>
          <div className="menu-anchor">
            <button
              className="icon-btn"
              aria-label={tr("transcript:toolbar.export")}
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
                    {tr("transcript:export.menuItem", { format: tr(`transcript:export.formats.${x.format}`) })}
                    <span className="shortcut">{x.hint}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
          <button className="icon-btn" aria-label={tr("transcript:toolbar.delete")} onClick={() => void remove()}>
            <IconTrash size={15} />
          </button>
        </div>
      </header>

      <div className="page transcript-page">
        <div className="transcript-meta muted">
          {formatRelativeDate(t.createdAt)} · {formatDuration(t.duration)}
          {t.language && ` · ${languageLabel(t.language)}`}
          {t.model && ` · ${t.model}`}
          {t.elapsedMs > 0 && t.duration > 0 && ` · ${tr("transcript:meta.elapsed", { seconds: (t.elapsedMs / 1000).toFixed(1) })}`}
        </div>

        <div className="transcript-downloads">
          <button className="btn primary" disabled={exporting} onClick={() => void doExport("srt", "srt")}>
            <IconDownload size={14} />
            {tr("transcript:downloads.srt")}
          </button>
          <button className="btn" disabled={exporting} onClick={() => void doExport("json", "json")}>
            <IconDownload size={14} />
            {tr("transcript:downloads.json")}
          </button>
          {!hasWordTimestamps && (
            <p className="transcript-download-note muted">
              {tr("transcript:downloads.noWordTimestamps")}
            </p>
          )}
        </div>

        <div className="transcript-view-controls">
          <div className="transcript-view-switch" role="group" aria-label={tr("transcript:view.label")}>
            <button
              className={`btn${display === "subtitles" ? " primary" : ""}`}
              aria-pressed={display === "subtitles"}
              onClick={() => { setEditing(null); setDisplay("subtitles"); }}
            >
              {tr("transcript:view.subtitles")}
            </button>
            <button
              className={`btn${display === "original" ? " primary" : ""}`}
              aria-pressed={display === "original"}
              onClick={() => { setEditing(null); setDisplay("original"); }}
            >
              {tr("transcript:view.original")}
            </button>
          </div>
          {display === "subtitles" && settings && (
            <div className="subtitle-line-setting">
              <span className="muted">{tr("transcript:view.subtitleLines")}</span>
              <Segmented
                label={tr("transcript:view.subtitleLines")}
                value={String(settings.subtitleMaxLines)}
                options={[
                  { value: "1", label: tr("transcript:view.oneLine") },
                  { value: "2", label: tr("transcript:view.twoLines") },
                ]}
                onChange={(value) => void saveSettings({ subtitleMaxLines: value === "2" ? 2 : 1 })}
              />
            </div>
          )}
          <p className="muted">
            {display === "subtitles" ? tr("transcript:view.subtitlesNote", { count: t.subtitles.length }) : tr("transcript:view.originalNote")}
          </p>
        </div>

        {visibleSegments.length === 0 ? (
          <div className="empty compact">
            <p>{tr("transcript:empty")}</p>
          </div>
        ) : (
          <div className="segments">
            {visibleSegments.map((s, i) => (
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
                  title={audioSrc ? tr("transcript:segment.playFromHere") : undefined}
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
                  <div className={`segment-text selectable${display === "subtitles" ? " subtitle-text" : ""}`} onDoubleClick={display === "original" ? () => setEditing(i) : undefined}>
                    {s.words?.length ? (
                      <SegmentText
                        segment={s}
                        time={i === current && (playing || position > 0) ? position : null}
                        onSeek={audioSrc ? seek : undefined}
                      />
                    ) : (
                      s.text
                    )}
                    {display === "subtitles" && t.subtitles[i].issues.length > 0 && (
                      <div className="subtitle-issues">
                        {t.subtitles[i].issues
                          .map((issue) => (isSubtitleIssue(issue) ? tr(`transcript:issues.${issue}`) : issue))
                          .join(" · ")}
                      </div>
                    )}
                  </div>
                )}
                {display === "original" && editing !== i && (
                  <button className="icon-btn segment-edit-btn" aria-label={tr("transcript:segment.edit")} onClick={() => setEditing(i)}>
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
          <button className="player-btn" onClick={togglePlay} aria-label={playing ? tr("transcript:player.pause") : tr("transcript:player.play")}>
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
            aria-label={tr("transcript:player.seek")}
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
            aria-label={tr("transcript:player.revealAudio")}
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
            onError={() => toast(tr("transcript:player.playbackError"), "error")}
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
