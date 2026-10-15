import { useEffect, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useStore } from "../lib/store";
import { Segmented, Switch } from "../components/controls";
import { IconCopy, IconFolder, IconWarning } from "../components/icons";

type Lang = "curl" | "python" | "js";

const ENDPOINTS: { method: "GET" | "POST"; path: string; desc: string; localOnly?: boolean }[] = [
  { method: "GET", path: "/health", desc: "服务状态、已加载模型及其支持的语言" },
  { method: "POST", path: "/transcribe", desc: "上传音频/视频文件转写，可选 json / txt / srt / vtt / md 输出" },
  { method: "GET", path: "/jobs/{id}", desc: "查询异步任务进度，完成后带结果" },
  { method: "POST", path: "/jobs/{id}/cancel", desc: "取消任务" },
  { method: "GET", path: "/transcripts/{id}?format=srt", desc: "按格式导出某次转写" },
  { method: "GET", path: "/models", desc: "模型列表与安装状态" },
  { method: "POST", path: "/models/load", desc: "加载已下载的模型" },
  { method: "GET", path: "/transcripts", desc: "历史记录列表", localOnly: true },
];

function examples(base: string): Record<Lang, string> {
  return {
    curl: `# 检查服务
curl ${base}/health

# 上传文件，直接得到 SRT 字幕
curl -F file=@meeting.m4a -F format=srt ${base}/transcribe

# 得到 JSON（含分段与词级时间戳）
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
  const { settings, saveSettings, apiStatus, toast } = useStore();
  const [lang, setLang] = useState<Lang>("curl");
  const [portDraft, setPortDraft] = useState("");

  useEffect(() => {
    if (settings) setPortDraft(String(settings.apiPort));
  }, [settings?.apiPort]);

  if (!settings) return null;

  const copy = (text: string) => void writeText(text).then(() => toast("已复制到剪贴板"));

  const commitPort = () => {
    const port = Number(portDraft);
    if (Number.isInteger(port) && port >= 1024 && port <= 65535) {
      if (port !== settings.apiPort) void saveSettings({ apiPort: port });
    } else {
      setPortDraft(String(settings.apiPort));
      toast("端口需为 1024–65535 之间的整数", "error");
    }
  };

  const running = settings.apiEnabled && apiStatus?.running;
  const base = (settings.apiLan && apiStatus?.lanUrl) || apiStatus?.url || `http://127.0.0.1:${settings.apiPort}`;
  const code = examples(base)[lang];

  return (
    <>
      <header className="toolbar drag" data-tauri-drag-region>
        <div className="toolbar-title">API 接口</div>
        <div className="toolbar-subtitle">
          {running ? (settings.apiLan ? "运行中 · 局域网可访问" : "运行中 · 仅本机") : "已停止"}
        </div>
      </header>
      <div className="page api-page">
        <p className="page-intro muted">
          Murmur 打开时提供 HTTP 接口，其他程序、局域网内的设备和 Agent 都可以调用它转写音频。
          接口任务与界面共用同一个推理队列，结果也会出现在左侧记录中。
        </p>

        <div className="form-group">
          <div className="form-row">
            <div className="label">
              启用 API 服务
              {apiStatus?.error && <div className="hint error-text">{apiStatus.error}</div>}
            </div>
            <Switch
              label="启用 API 服务"
              checked={settings.apiEnabled}
              onChange={(apiEnabled) => void saveSettings({ apiEnabled })}
            />
          </div>
          <div className="form-row">
            <div className="label">
              允许局域网访问
              <div className="hint">同一网络下的其他电脑、手机可以调用；在公共网络中建议关闭</div>
            </div>
            <Switch
              label="允许局域网访问"
              checked={settings.apiLan}
              disabled={!settings.apiEnabled}
              onChange={(apiLan) => void saveSettings({ apiLan })}
            />
          </div>
          <div className="form-row">
            <div className="label">端口</div>
            <input
              className="port-input tabular"
              aria-label="端口"
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
            <h3 className="form-group-title">服务地址</h3>
            <div className="form-group">
              {settings.apiLan && (
                <AddressRow
                  label="局域网"
                  hint="在其他设备上使用这个地址"
                  url={apiStatus?.lanUrl ?? null}
                  missing="未检测到局域网连接"
                  onCopy={copy}
                />
              )}
              <AddressRow label="本机" url={apiStatus?.url ?? null} onCopy={copy} />
            </div>
            {settings.apiLan && (
              <p className="api-note muted">
                <IconWarning size={13} />
                局域网接口没有密码，同一网络中的任何设备都能提交转写。其他设备只能上传文件，不能读取本机文件或浏览历史记录。
              </p>
            )}
          </>
        )}

        <h3 className="form-group-title">接口</h3>
        <div className="form-group">
          {ENDPOINTS.map((e) => (
            <div className="form-row api-endpoint" key={e.method + e.path}>
              <span className={`api-method ${e.method.toLowerCase()}`}>{e.method}</span>
              <code className="selectable">{e.path}</code>
              <span className="label muted">{e.desc}</span>
              {e.localOnly && <span className="badge">仅本机</span>}
            </div>
          ))}
        </div>

        <div className="api-code-head">
          <h3 className="form-group-title">调用示例</h3>
          <Segmented
            label="示例语言"
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
          <button className="btn plain api-code-copy" onClick={() => copy(code)} aria-label="复制代码">
            <IconCopy size={14} />
          </button>
          <pre className="selectable">{code}</pre>
        </div>

        {apiStatus && (
          <>
            <h3 className="form-group-title">Agent Skill</h3>
            <div className="form-group">
              <div className="form-row">
                <div className="label">
                  murmur-asr
                  <div className="hint">复制到 Agent 的 skills 目录（如 ~/.claude/skills/），Agent 就会用 Murmur 转写音频</div>
                </div>
                <button className="btn" onClick={() => void revealItemInDir(apiStatus.skillDir)}>
                  <IconFolder size={14} />
                  显示
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
            复制
          </button>
        </>
      ) : (
        <span className="muted">{missing}</span>
      )}
    </div>
  );
}
