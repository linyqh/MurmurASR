import { useEffect, useMemo, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useStore, AUDIO_EXTENSIONS } from "./lib/store";
import { Sidebar } from "./views/Sidebar";
import { HomeView } from "./views/Home";
import { LiveView } from "./views/Live";
import { ModelsView } from "./views/Models";
import { SettingsView } from "./views/Settings";
import { TranscriptView } from "./views/Transcript";
import { Onboarding } from "./views/Onboarding";
import { IconUpload } from "./components/icons";

export function App() {
  const { view, toasts, models, settings, transcribeFiles, setView, toast } = useStore();
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "enter" || payload.type === "over") setDragging(true);
      else if (payload.type === "leave") setDragging(false);
      else if (payload.type === "drop") {
        setDragging(false);
        const audio = payload.paths.filter((p) =>
          AUDIO_EXTENSIONS.includes(p.split(".").pop()?.toLowerCase() ?? ""),
        );
        if (audio.length === 0) {
          toast("不支持的文件类型，请拖入音频或视频文件", "error");
          return;
        }
        if (!models?.loaded) {
          toast("请先下载并加载一个模型", "error");
          setView({ name: "models" });
          return;
        }
        setView({ name: "home" });
        void transcribeFiles(audio);
      }
    });
    return () => void unlisten.then((un) => un());
  }, [models?.loaded, setView, toast, transcribeFiles]);

  const needsOnboarding = useMemo(
    () => !!models && !!settings && !settings.onboarded && models.installed.length === 0,
    [models, settings],
  );

  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        {view.name === "home" && <HomeView />}
        {view.name === "live" && <LiveView />}
        {view.name === "models" && <ModelsView />}
        {view.name === "settings" && <SettingsView />}
        {view.name === "transcript" && <TranscriptView key={view.id} id={view.id} />}
        {dragging && (
          <div className="drop-overlay">
            <div className="drop-card glass">
              <IconUpload size={30} />
              <div>松开以转写</div>
            </div>
          </div>
        )}
      </main>
      {needsOnboarding && <Onboarding />}
      <div className="toasts" aria-live="polite">
        {toasts.map((t) => (
          <div key={t.id} className={`toast glass ${t.tone}`}>
            <span className="dot" />
            {t.text}
          </div>
        ))}
      </div>
    </div>
  );
}
