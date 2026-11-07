import { useTranslation } from "react-i18next";
import { useStore } from "../lib/store";
import { languageLabel, selectableLanguages } from "../lib/format";
import { Select } from "../components/controls";
import { IconGlobe } from "../components/icons";

/** Recognition language, offered from what the loaded model reports. */
export function LanguagePicker() {
  const { t } = useTranslation(["home", "common"]);
  const { models, settings, saveSettings } = useStore();
  const loaded = models?.loaded;
  if (!settings || !loaded || !loaded.acceptsLanguage) return null;

  const languages = selectableLanguages(loaded.languages);
  const value = languages.includes(settings.language) ? settings.language : "auto";

  return (
    <span className="language-picker">
      <IconGlobe size={14} />
      <Select
        label={t("languagePicker.label")}
        value={value}
        options={languages.map((l) => ({ value: l, label: l === "auto" ? t("common:language.auto") : languageLabel(l) }))}
        onChange={(language) => void saveSettings({ language })}
      />
    </span>
  );
}
