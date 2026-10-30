import { api, modelSize } from "../lib/api";
import { useStore } from "../lib/store";
import { formatBytes, formatEta } from "../lib/format";
import { Progress } from "../components/controls";
import { IconBolt, IconCheckCircle, IconDownload, IconWaveform } from "../components/icons";

/** First-run sheet: explains that models are downloaded, offers the recommended one. */
export function Onboarding() {
  const { models, downloads, saveSettings, toast } = useStore();
  const recommended = models?.catalog.models.find((m) => m.id === models.catalog.recommended);
  const download = recommended ? downloads[recommended.id] : undefined;
  const busy = !!download || (!!recommended && !!models?.downloading.includes(recommended.id));

  if (!recommended) return null;
  const fraction = download ? download.downloaded / Math.max(1, download.total) : 0;

  return (
    <div className="sheet-backdrop">
      <div className="sheet glass onboarding" role="dialog" aria-modal="true" aria-labelledby="onb-title">
        <div className="onboarding-icon">
          <IconWaveform size={34} />
        </div>
        <h1 id="onb-title">欢迎使用 Murmur</h1>
        <p className="muted">
          在你的 Mac 上离线完成语音转文字。音频不会离开这台设备。
          <br />
          开始之前，需要下载一个语音识别模型。
        </p>

        <div className="onboarding-model card">
          <div className="onboarding-model-head">
            <div>
              <div className="onboarding-model-name">
                {recommended.name} <span className="badge accent">推荐</span>
              </div>
              <div className="muted small">{recommended.languages}</div>
            </div>
            <div className="muted tabular">{formatBytes(modelSize(recommended))}</div>
          </div>
          <ul className="onboarding-points">
            <li>
              <IconCheckCircle size={14} /> 支持中英日韩等 30 种语言与多种方言
            </li>
            <li>
              <IconBolt size={14} /> Apple 芯片 Metal 加速，转写速度可达实时的 20 倍
            </li>
          </ul>
          {busy && (
            <div className="onboarding-progress">
              <Progress value={fraction} indeterminate={!download || download.verifying} />
              <div className="muted small tabular">
                {download?.verifying
                  ? "正在校验…"
                  : download
                    ? `${formatBytes(download.downloaded)} / ${formatBytes(download.total)} · ${formatEta(download.total - download.downloaded, download.bytesPerSec)}`
                    : "正在连接…"}
              </div>
            </div>
          )}
        </div>

        <div className="onboarding-actions">
          <button className="btn plain" onClick={() => void saveSettings({ onboarded: true })}>
            {busy ? "后台下载" : "稍后再说"}
          </button>
          {!busy && (
            <button
              className="btn primary large"
              onClick={() => void api.downloadModel(recommended.id).catch((e) => toast(String(e), "error"))}
            >
              <IconDownload size={15} /> 下载推荐模型
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
