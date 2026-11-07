import { useTranslation } from "react-i18next";
import { api, modelSize } from "../lib/api";
import { useStore } from "../lib/store";
import { formatBytes, formatEta } from "../lib/format";
import { errorMessage } from "../lib/errors";
import { useCatalogText } from "../lib/catalogText";
import { Progress } from "../components/controls";
import { IconBolt, IconCheckCircle, IconDownload, IconWaveform } from "../components/icons";

/** First-run sheet: explains that models are downloaded, offers the recommended one. */
export function Onboarding() {
  const { t } = useTranslation("onboarding");
  const c = useCatalogText();
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
        <h1 id="onb-title">{t("title")}</h1>
        <p className="muted">
          {t("intro")}
          <br />
          {t("needModel")}
        </p>

        <div className="onboarding-model card">
          <div className="onboarding-model-head">
            <div>
              <div className="onboarding-model-name">
                {recommended.name} <span className="badge accent">{t("recommended")}</span>
              </div>
              <div className="muted small">{c.languages(recommended)}</div>
            </div>
            <div className="muted tabular">{formatBytes(modelSize(recommended))}</div>
          </div>
          <ul className="onboarding-points">
            <li>
              <IconCheckCircle size={14} /> {t("points.languages")}
            </li>
            <li>
              <IconBolt size={14} /> {t("points.speed")}
            </li>
          </ul>
          {busy && (
            <div className="onboarding-progress">
              <Progress value={fraction} indeterminate={!download || download.verifying} />
              <div className="muted small tabular">
                {download?.verifying
                  ? t("progress.verifying")
                  : download
                    ? `${formatBytes(download.downloaded)} / ${formatBytes(download.total)} · ${formatEta(download.total - download.downloaded, download.bytesPerSec)}`
                    : t("progress.connecting")}
              </div>
            </div>
          )}
        </div>

        <div className="onboarding-actions">
          <button className="btn plain" onClick={() => void saveSettings({ onboarded: true })}>
            {busy ? t("actions.background") : t("actions.later")}
          </button>
          {!busy && (
            <button
              className="btn primary large"
              onClick={() => void api.downloadModel(recommended.id).catch((e) => toast(errorMessage(e), "error"))}
            >
              <IconDownload size={15} /> {t("actions.download")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
