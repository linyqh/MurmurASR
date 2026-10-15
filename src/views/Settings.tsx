import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, type InputDevice } from "../lib/api";
import { useStore } from "../lib/store";
import { languageLabel, selectableLanguages } from "../lib/format";
import { Segmented, Select, Switch } from "../components/controls";

export function SettingsView() {
  const { settings, saveSettings, info, models, toast, refreshModels } = useStore();
  const [devices, setDevices] = useState<InputDevice[]>([]);

  useEffect(() => {
    void api.listInputDevices().then(setDevices);
  }, []);

  if (!settings || !info) return null;

  const loaded = models?.loaded;
  const languages = loaded?.acceptsLanguage ? selectableLanguages(loaded.languages) : ["auto"];
  const reloadHint = loaded ? "（重新加载模型后生效）" : "";

  const reload = async (patch: Partial<typeof settings>) => {
    await saveSettings(patch);
    if (loaded) await api.loadModel(loaded.id).catch((e) => toast(String(e), "error"));
  };

  const chooseModelsDir = async () => {
    const dir = await open({ directory: true, title: "选择模型存放位置" });
    if (typeof dir === "string") {
      await saveSettings({ modelsDir: dir });
      await refreshModels();
      toast("模型位置已更改，已有模型需手动移动到新位置");
    }
  };

  return (
    <>
      <header className="toolbar drag" data-tauri-drag-region>
        <div className="toolbar-title">设置</div>
      </header>
      <div className="page settings-page">
        <h3 className="form-group-title">识别</h3>
        <div className="form-group">
          <div className="form-row">
            <div className="label">
              识别语言
              <div className="hint">
                {loaded?.acceptsLanguage ? "自动检测适合多语言混合的内容" : "当前模型会自动处理语言"}
              </div>
            </div>
            <Select
              label="识别语言"
              value={languages.includes(settings.language) ? settings.language : "auto"}
              disabled={!loaded?.acceptsLanguage}
              options={languages.map((l) => ({ value: l, label: languageLabel(l) }))}
              onChange={(language) => void saveSettings({ language })}
            />
          </div>
          <div className="form-row">
            <div className="label">
              数字与标点规整
              <div className="hint">把“一百二十三”写成“123”等（SenseVoice、Fun-ASR 支持）</div>
            </div>
            <Switch label="数字与标点规整" checked={settings.itn} onChange={(itn) => void saveSettings({ itn })} />
          </div>
        </div>

        <h3 className="form-group-title">录音</h3>
        <div className="form-group">
          <div className="form-row">
            <div className="label">麦克风</div>
            <Select
              label="麦克风"
              value={settings.inputDevice ?? ""}
              options={[
                { value: "", label: "系统默认" },
                ...devices.map((d) => ({ value: d.id, label: d.name })),
              ]}
              onChange={(v) => void saveSettings({ inputDevice: v || null })}
            />
          </div>
          <div className="form-row">
            <div className="label">
              保存实时听写的录音
              <div className="hint">保存后可在记录中回放并对照文字</div>
            </div>
            <Switch
              label="保存录音"
              checked={settings.saveRecordings}
              onChange={(saveRecordings) => void saveSettings({ saveRecordings })}
            />
          </div>
        </div>

        <h3 className="form-group-title">性能</h3>
        <div className="form-group">
          <div className="form-row">
            <div className="label">
              计算后端
              <div className="hint">
                {info.platform === "macos" ? "Metal 使用 Apple 芯片的 GPU 加速" : "“自动”会选择可用的最快设备"}
                {reloadHint}
              </div>
            </div>
            <Segmented
              label="计算后端"
              value={settings.backend}
              onChange={(backend) => void reload({ backend })}
              options={info.backends.map((b) => ({
                value: b,
                label: b === "best" ? "自动" : b === "cpu" ? "CPU" : b.charAt(0).toUpperCase() + b.slice(1),
              }))}
            />
          </div>
          <div className="form-row">
            <div className="label">
              CPU 线程数
              <div className="hint">仅影响 CPU 计算部分{reloadHint}</div>
            </div>
            <Select
              label="CPU 线程数"
              value={String(settings.threads)}
              options={[1, 2, 4, 6, 8, 12, 16].map((n) => ({ value: String(n), label: `${n}` }))}
              onChange={(v) => void reload({ threads: Number(v) })}
            />
          </div>
        </div>

        <h3 className="form-group-title">存储</h3>
        <div className="form-group">
          <div className="form-row">
            <div className="label">模型位置</div>
            <span className="value selectable" title={info.modelsDir}>
              {settings.modelsDir ?? info.modelsDir}
            </span>
            <button className="btn" onClick={() => void chooseModelsDir()}>
              更改…
            </button>
          </div>
          <div className="form-row">
            <div className="label">数据位置</div>
            <span className="value selectable" title={info.dataDir}>
              {info.dataDir}
            </span>
            <button className="btn" onClick={() => void revealItemInDir(info.dataDir)}>
              显示
            </button>
          </div>
        </div>

        <h3 className="form-group-title">关于</h3>
        <div className="form-group">
          <div className="form-row">
            <div className="label">Murmur</div>
            <span className="muted tabular">{info.version}</span>
          </div>
          <div className="form-row">
            <div className="label">
              推理引擎
              {info.engineError && <div className="hint error-text">{info.engineError}</div>}
            </div>
            <span className="muted tabular">
              {info.engineVersion ? `audio.cpp ${info.engineVersion}` : "不可用"}
            </span>
          </div>
          <div className="form-row">
            <div className="label">系统</div>
            <span className="muted">
              {info.platform} · {info.arch}
            </span>
          </div>
        </div>
      </div>
    </>
  );
}
