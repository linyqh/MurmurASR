import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useStore } from "../lib/store";
import { Segmented, Switch } from "../components/controls";
import { IconCopy, IconFolder, IconWarning } from "../components/icons";

type Lang = "curl" | "python" | "js";

/** `desc` is a key under `api:endpoints`. */
const ENDPOINTS = [
  { method: "GET", path: "/health", desc: "health" },
  { method: "POST", path: "/transcribe", desc: "transcribe" },
  { method: "GET", path: "/jobs/{id}", desc: "job" },
  { method: "POST", path: "/jobs/{id}/cancel", desc: "cancel" },
  { method: "GET", path: "/transcripts/{id}?format=srt", desc: "export" },
  { method: "GET", path: "/models", desc: "models" },
  { method: "POST", path: "/models/load", desc: "load" },
  { method: "GET", path: "/transcripts", desc: "history", localOnly: true },
] as const;

interface ExampleComments {
  checkService: string;
  uploadSrt: string;
  getJson: string;
}

function examples(base: string, c: ExampleComments): Record<Lang, string> {
  return {
    curl: `# ${c.checkService}
curl ${base}/health

# ${c.uploadSrt}
curl -F file=@meeting.m4a -F format=srt ${base}/transcribe

# ${c.getJson}
curl -F file=@meeting.m4a ${base}/transcribe`,
    python: `import requests

with open("meeting.m4a", "rb") as f:
    r = requests.post(
        "${base}/transcribe",
        files={"file": f},
        data={"format": "json"},  # json / txt / srt / vtt / md
    )
r.raise_for_status()
result = r.json()
print(result["text"])
for seg in result["segments"]:
    print(seg["start"], seg["end"], seg["text"])`,
    js: `import { readFile } from "node:fs/promises";

const form = new FormData();
form.append("file", new Blob([await readFile("meeting.m4a")]), "meeting.m4a");
form.append("format", "json");

const res = await fetch("${base}/transcribe", { method: "POST", body: form });
if (!res.ok) throw new Error((await res.json()).error);
const result = await res.json();
console.log(result.text);`,
  };
}

export function ApiView() {
  const { t } = useTranslation(["api", "common"]);
  const { settings, saveSettings, apiStatus, toast } = useStore();
  const [lang, setLang] = useState<Lang>("curl");
  const [portDraft, setPortDraft] = useState("");

  useEffect(() => {
    if (settings) setPortDraft(String(settings.apiPort));
  }, [settings?.apiPort]);

  if (!settings) return null;

  const copy = (text: string) => void writeText(text).then(() => toast(t("common:actions.copied")));

  const commitPort = () => {
    const port = Number(portDraft);
    if (Number.isInteger(port) && port >= 1024 && port <= 65535) {
      if (port !== settings.apiPort) void saveSettings({ apiPort: port });
    } else {
      setPortDraft(String(settings.apiPort));
      toast(t("api:port.invalid"), "error");
    }
  };

  const running = settings.apiEnabled && apiStatus?.running;
  const base = (settings.apiLan && apiStatus?.lanUrl) || apiStatus?.url || `http://127.0.0.1:${settings.apiPort}`;
  const code = examples(base, {
    checkService: t("api:examples.checkService"),
    uploadSrt: t("api:examples.uploadSrt"),
    getJson: t("api:examples.getJson"),
  })[lang];

  return (
    <>
      <header className="toolbar drag" data-tauri-drag-region>
        <div className="toolbar-title">{t("api:toolbar.title")}</div>
        <div className="toolbar-subtitle">
          {running ? (settings.apiLan ? t("api:toolbar.runningLan") : t("api:toolbar.runningLocal")) : t("api:toolbar.stopped")}
        </div>
      </header>
      <div className="page api-page">
        <p className="page-intro muted">{t("api:intro")}</p>

        <div className="form-group">
          <div className="form-row">
            <div className="label">
              {t("api:enable")}
              {apiStatus?.error && <div className="hint error-text">{apiStatus.error}</div>}
            </div>
            <Switch
              label={t("api:enable")}
              checked={settings.apiEnabled}
              onChange={(apiEnabled) => void saveSettings({ apiEnabled })}
            />
          </div>
          <div className="form-row">
            <div className="label">
              {t("api:lan.label")}
              <div className="hint">{t("api:lan.hint")}</div>
            </div>
            <Switch
              label={t("api:lan.label")}
              checked={settings.apiLan}
              disabled={!settings.apiEnabled}
              onChange={(apiLan) => void saveSettings({ apiLan })}
            />
          </div>
          <div className="form-row">
            <div className="label">{t("api:port.label")}</div>
            <input
              className="port-input tabular"
              aria-label={t("api:port.label")}
              inputMode="numeric"
              value={portDraft}
              disabled={!settings.apiEnabled}
              onChange={(e) => setPortDraft(e.target.value.replace(/\D/g, ""))}
              onBlur={commitPort}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
            />
          </div>
        </div>

        {running && (
          <>
            <h3 className="form-group-title">{t("api:addresses.title")}</h3>
            <div className="form-group">
              {settings.apiLan && (
                <AddressRow
                  label={t("api:addresses.lan")}
                  hint={t("api:addresses.lanHint")}
                  url={apiStatus?.lanUrl ?? null}
                  missing={t("api:addresses.lanMissing")}
                  onCopy={copy}
                />
              )}
              <AddressRow label={t("api:addresses.local")} url={apiStatus?.url ?? null} onCopy={copy} />
            </div>
            {settings.apiLan && (
              <p className="api-note muted">
                <IconWarning size={13} />
                {t("api:addresses.lanWarning")}
              </p>
            )}
          </>
        )}

        <h3 className="form-group-title">{t("api:endpoints.title")}</h3>
        <div className="form-group">
          {ENDPOINTS.map((e) => (
            <div className="form-row api-endpoint" key={e.method + e.path}>
              <span className={`api-method ${e.method.toLowerCase()}`}>{e.method}</span>
              <code className="selectable">{e.path}</code>
              <span className="label muted">{t(`api:endpoints.${e.desc}`)}</span>
              {"localOnly" in e && <span className="badge">{t("api:endpoints.localOnly")}</span>}
            </div>
          ))}
        </div>

        <div className="api-code-head">
          <h3 className="form-group-title">{t("api:examples.title")}</h3>
          <Segmented
            label={t("api:examples.language")}
            value={lang}
            onChange={setLang}
            options={[
              { value: "curl", label: "curl" },
              { value: "python", label: "Python" },
              { value: "js", label: "JavaScript" },
            ]}
          />
        </div>
        <div className="api-code card">
          <button className="btn plain api-code-copy" onClick={() => copy(code)} aria-label={t("api:examples.copyLabel")}>
            <IconCopy size={14} />
          </button>
          <pre className="selectable">{code}</pre>
        </div>

        {apiStatus && (
          <>
            <h3 className="form-group-title">{t("api:skill.title")}</h3>
            <div className="form-group">
              <div className="form-row">
                <div className="label">
                  murmur-asr
                  <div className="hint">{t("api:skill.hint")}</div>
                </div>
                <button className="btn" onClick={() => void revealItemInDir(apiStatus.skillDir)}>
                  <IconFolder size={14} />
                  {t("common:actions.show")}
                </button>
              </div>
            </div>
          </>
        )}
      </div>
    </>
  );
}

function AddressRow({
  label,
  hint,
  url,
  missing,
  onCopy,
}: {
  label: string;
  hint?: string;
  url: string | null;
  missing?: string;
  onCopy: (text: string) => void;
}) {
  const { t } = useTranslation("common");
  return (
    <div className="form-row">
      <div className="label">
        {label}
        {hint && <div className="hint">{hint}</div>}
      </div>
      {url ? (
        <>
          <code className="api-url selectable">{url}</code>
          <button className="btn" onClick={() => onCopy(url)}>
            {t("actions.copy")}
          </button>
        </>
      ) : (
        <span className="muted">{missing}</span>
      )}
    </div>
  );
}
