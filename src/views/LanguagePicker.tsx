import { useStore } from "../lib/store";
import { languageLabel, selectableLanguages } from "../lib/format";
import { Select } from "../components/controls";
import { IconGlobe } from "../components/icons";

/** Recognition language, offered from what the loaded model reports. */
export function LanguagePicker() {
  const { models, settings, saveSettings } = useStore();
  const loaded = models?.loaded;
  if (!settings || !loaded || !loaded.acceptsLanguage) return null;

  const languages = selectableLanguages(loaded.languages);
  const value = languages.includes(settings.language) ? settings.language : "auto";

  return (
    <span className="language-picker">
      <IconGlobe size={14} />
      <Select
        label="识别语言"
        value={value}
        options={languages.map((l) => ({ value: l, label: languageLabel(l) }))}
        onChange={(language) => void saveSettings({ language })}
      />
    </span>
  );
}
