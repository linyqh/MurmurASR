import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, type InputDevice } from "../lib/api";
import { useStore } from "../lib/store";
import { errorMessage } from "../lib/errors";
import { languageLabel, selectableLanguages } from "../lib/format";
import { SUPPORTED_LANGUAGES, getLanguagePreference, setLanguagePreference } from "../i18n";
import { Segmented, Select, Switch } from "../components/controls";

export function SettingsView() {
  const { t } = useTranslation(["settings", "common"]);
  const { settings, saveSettings, info, models, toast, refreshModels } = useStore();
  const [devices, setDevices] = useState<InputDevice[]>([]);
  const [uiLanguage, setUiLanguage] = useState(getLanguagePreference);

  useEffect(() => {
    void api.listInputDevices().then(setDevices);
  }, []);

  if (!settings || !info) return null;

  const loaded = models?.loaded;
  const languages = loaded?.acceptsLanguage ? selectableLanguages(loaded.languages) : ["auto"];
  const reloadHint = loaded ? t("settings:performance.reloadHint") : "";

  const reload = async (patch: Partial<typeof settings>) => {
    await saveSettings(patch);
    if (loaded) await api.loadModel(loaded.id).catch((e) => toast(errorMessage(e), "error"));
  };

  const chooseModelsDir = async () => {
    const dir = await open({ directory: true, title: t("settings:storage.chooseModelsDir") });
    if (typeof dir === "string") {
      await saveSettings({ modelsDir: dir });
      await refreshModels();
      toast(t("settings:storage.modelsDirChanged"));
    }
  };

  return (
    <>
      <header className="toolbar drag" data-tauri-drag-region>
        <div className="toolbar-title">{t("settings:toolbar.title")}</div>
      </header>
      <div className="page settings-page">
        <h3 className="form-group-title">{t("settings:general.title")}</h3>
        <div className="form-group">
          <div className="form-row">
            <div className="label">
              {t("common:language.interface")}
              <div className="hint">{t("settings:general.languageHint")}</div>
            </div>
            <Select
              label={t("common:language.interface")}
              value={uiLanguage}
              options={[
                { value: "system", label: t("common:language.system") },
                ...SUPPORTED_LANGUAGES.map((l) => ({ value: l.code, label: l.name })),
              ]}
              onChange={(language) => {
                void setLanguagePreference(language);
                setUiLanguage(language);
              }}
            />
          </div>
        </div>

        <h3 className="form-group-title">{t("settings:recognition.title")}</h3>
        <div className="form-group">
          <div className="form-row">
            <div className="label">
              {t("settings:recognition.language")}
              <div className="hint">
                {loaded?.acceptsLanguage ? t("settings:recognition.languageHintMulti") : t("settings:recognition.languageHintSingle")}
              </div>
            </div>
            <Select
              label={t("settings:recognition.language")}
              value={languages.includes(settings.language) ? settings.language : "auto"}
              disabled={!loaded?.acceptsLanguage}
              options={languages.map((l) => ({ value: l, label: languageLabel(l) }))}
              onChange={(language) => void saveSettings({ language })}
            />
          </div>
          <div className="form-row">
            <div className="label">
              {t("settings:recognition.itn")}
              <div className="hint">{t("settings:recognition.itnHint")}</div>
            </div>
            <Switch label={t("settings:recognition.itn")} checked={settings.itn} onChange={(itn) => void saveSettings({ itn })} />
          </div>
        </div>

        <h3 className="form-group-title">{t("settings:subtitles.title")}</h3>
        <div className="form-group">
          <div className="form-row">
            <div className="label">
              {t("settings:subtitles.maxLines")}
              <div className="hint">{t("settings:subtitles.maxLinesHint")}</div>
            </div>
            <Segmented
              label={t("settings:subtitles.maxLines")}
              value={String(settings.subtitleMaxLines)}
              options={[
                { value: "1", label: t("settings:subtitles.oneLine") },
                { value: "2", label: t("settings:subtitles.twoLines") },
              ]}
              onChange={(value) => void saveSettings({ subtitleMaxLines: value === "2" ? 2 : 1 })}
            />
          </div>
        </div>

        <h3 className="form-group-title">{t("settings:recording.title")}</h3>
        <div className="form-group">
          <div className="form-row">
            <div className="label">{t("settings:recording.microphone")}</div>
            <Select
              label={t("settings:recording.microphone")}
              value={settings.inputDevice ?? ""}
              options={[
                { value: "", label: t("settings:recording.systemDefault") },
                ...devices.map((d) => ({ value: d.id, label: d.name })),
              ]}
              onChange={(v) => void saveSettings({ inputDevice: v || null })}
            />
          </div>
          <div className="form-row">
            <div className="label">
              {t("settings:recording.saveRecordings")}
              <div className="hint">{t("settings:recording.saveRecordingsHint")}</div>
            </div>
            <Switch
              label={t("settings:recording.saveRecordingsLabel")}
              checked={settings.saveRecordings}
              onChange={(saveRecordings) => void saveSettings({ saveRecordings })}
            />
          </div>
        </div>

        <h3 className="form-group-title">{t("settings:performance.title")}</h3>
        <div className="form-group">
          <div className="form-row">
            <div className="label">
              {t("settings:performance.backend")}
              <div className="hint">
                {info.platform === "macos" ? t("settings:performance.backendHintMac") : t("settings:performance.backendHintOther")}
                {reloadHint}
              </div>
            </div>
            <Segmented
              label={t("settings:performance.backend")}
              value={settings.backend}
              onChange={(backend) => void reload({ backend })}
              options={info.backends.map((b) => ({
                value: b,
                label: b === "best" ? t("settings:performance.auto") : b === "cpu" ? "CPU" : b.charAt(0).toUpperCase() + b.slice(1),
              }))}
            />
          </div>
          <div className="form-row">
            <div className="label">
              {t("settings:performance.threads")}
              <div className="hint">{t("settings:performance.threadsHint")}{reloadHint}</div>
            </div>
            <Select
              label={t("settings:performance.threads")}
              value={String(settings.threads)}
              options={[1, 2, 4, 6, 8, 12, 16].map((n) => ({ value: String(n), label: `${n}` }))}
              onChange={(v) => void reload({ threads: Number(v) })}
            />
          </div>
        </div>

        <h3 className="form-group-title">{t("settings:storage.title")}</h3>
        <div className="form-group">
          <div className="form-row">
            <div className="label">{t("settings:storage.modelsDir")}</div>
            <span className="value selectable" title={info.modelsDir}>
              {settings.modelsDir ?? info.modelsDir}
            </span>
            <button className="btn" onClick={() => void chooseModelsDir()}>
              {t("settings:storage.change")}
            </button>
          </div>
          <div className="form-row">
            <div className="label">{t("settings:storage.dataDir")}</div>
            <span className="value selectable" title={info.dataDir}>
              {info.dataDir}
            </span>
            <button className="btn" onClick={() => void revealItemInDir(info.dataDir)}>
              {t("common:actions.show")}
            </button>
          </div>
        </div>

        <h3 className="form-group-title">{t("settings:about.title")}</h3>
        <div className="form-group">
          <div className="form-row">
            <div className="label">Murmur</div>
            <span className="muted tabular">{info.version}</span>
          </div>
          <div className="form-row">
            <div className="label">
              {t("settings:about.engine")}
              {info.engineError && <div className="hint error-text">{info.engineError}</div>}
            </div>
            <span className="muted tabular">
              {info.engineVersion ? `audio.cpp ${info.engineVersion}` : t("settings:about.engineUnavailable")}
            </span>
          </div>
          <div className="form-row">
            <div className="label">{t("settings:about.system")}</div>
            <span className="muted">
              {info.platform} · {info.arch}
            </span>
          </div>
        </div>
      </div>
    </>
  );
}
