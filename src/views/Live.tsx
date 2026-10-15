import { useEffect, useRef, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useStore } from "../lib/store";
import { formatClock, joinSegments, liveTitle } from "../lib/format";
import { IconCopy, IconCube, IconMic, IconStop } from "../components/icons";
import { LanguagePicker } from "./LanguagePicker";

const BARS = 28;

export function LiveView() {
  const { models, live, startLive, stopLive, setView, toast } = useStore();
  const [now, setNow] = useState(Date.now());
  const [levels, setLevels] = useState<number[]>(() => Array(BARS).fill(0));
  const scroller = useRef<HTMLDivElement>(null);
  const recording = !!live.id && !live.starting;
  const ready = !!models?.loaded;

  useEffect(() => {
    if (!recording) return;
    const t = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(t);
  }, [recording]);

  useEffect(() => {
    if (!recording) {
      setLevels(Array(BARS).fill(0));
      return;
    }
    // Perceptual scaling: speech RMS is small, so map it through a log curve.
    const db = 20 * Math.log10(Math.max(live.level, 1e-5));
    const v = Math.max(0, Math.min(1, (db + 55) / 45));
    setLevels((l) => [...l.slice(1), v]);
  }, [live.level, recording]);

  useEffect(() => {
    scroller.current?.scrollTo({ top: scroller.current.scrollHeight, behavior: "smooth" });
  }, [live.segments.length, live.partial]);

  const text = joinSegments(live.segments.map((s) => s.text));
  const elapsed = recording ? (now - live.startedAt) / 1000 : 0;

  const toggle = () => {
    if (live.id) void stopLive();
    else void startLive(liveTitle());
  };

  return (
    <>
      <header className="toolbar drag" data-tauri-drag-region>
        <div className="toolbar-title">实时听写</div>
        <div className="toolbar-spacer" />
        <div className="no-drag toolbar-actions">
          <LanguagePicker />
          <button
            className="icon-btn"
            aria-label="复制全部"
            disabled={!text}
            onClick={() => void writeText(text).then(() => toast("已复制到剪贴板"))}
          >
            <IconCopy size={15} />
          </button>
        </div>
      </header>

      {!ready ? (
        <div className="page">
          <div className="empty">
            <div className="empty-icon">
              <IconCube size={30} />
            </div>
            <h2>需要先加载模型</h2>
            <p>下载并加载一个语音识别模型后，即可实时听写。</p>
            <button className="btn primary large" onClick={() => setView({ name: "models" })}>
              前往下载模型
            </button>
          </div>
        </div>
      ) : (
        <>
          <div className="page live-page" ref={scroller}>
            {live.segments.length === 0 && !live.partial ? (
              <div className="live-placeholder">
                {recording ? "正在聆听，请开始说话…" : "点按下方按钮开始听写，说话内容会实时出现在这里。"}
              </div>
            ) : (
              <div className="live-text selectable">
                {live.segments.map((s) => (
                  <span key={`${s.start}-${s.index}`} className="live-seg">
                    {s.text}{" "}
                  </span>
                ))}
                {live.partial && <span className="live-partial">{live.partial}</span>}
              </div>
            )}
          </div>

          <div className="live-dock-wrap">
            <div className={`live-dock glass${recording ? " recording" : ""}`}>
              {/* Newest level sits next to the button and flows outward on both sides. */}
              <div className={`live-wave left${recording ? "" : " idle"}`} aria-hidden>
                {levels.map((v, i) => (
                  <i key={i} style={{ transform: `scaleY(${0.1 + v * 0.9})` }} />
                ))}
              </div>
              <button
                className={`record-btn${recording ? " recording" : ""}`}
                onClick={toggle}
                disabled={live.starting}
                aria-label={recording ? "停止" : "开始听写"}
              >
                {recording ? <IconStop size={20} /> : <IconMic size={22} />}
              </button>
              <div className={`live-wave right${recording ? "" : " idle"}`} aria-hidden>
                {levels.map((v, i) => (
                  <i key={i} style={{ transform: `scaleY(${0.1 + v * 0.9})` }} />
                ))}
              </div>
            </div>
            <div className="live-status tabular">
              {recording ? (
                <>
                  <span className="live-status-dot" />
                  {formatClock(elapsed)}
                </>
              ) : live.starting ? (
                "正在启动麦克风…"
              ) : (
                "点按开始听写"
              )}
            </div>
          </div>
        </>
      )}
    </>
  );
}
