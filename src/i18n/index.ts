import i18n from "i18next";
import { initReactI18next } from "react-i18next";

/**
 * Locales are discovered from `src/locales/<code>/<namespace>.json`.
 * To add a language, create a new folder with the same files as `en/`
 * and set `meta.name` in its `common.json`; nothing else needs to change.
 */
const modules = import.meta.glob<{ default: Record<string, unknown> }>("../locales/*/*.json", {
  eager: true,
});

const resources: Record<string, Record<string, Record<string, unknown>>> = {};
for (const [path, mod] of Object.entries(modules)) {
  const [, code, ns] = /\.\.\/locales\/([^/]+)\/([^/]+)\.json$/.exec(path)!;
  (resources[code] ??= {})[ns] = mod.default;
}

export const FALLBACK_LANGUAGE = "en";
export const DEFAULT_NAMESPACE = "common";
export const NAMESPACES = Object.keys(resources[FALLBACK_LANGUAGE]);

export interface LanguageOption {
  code: string;
  /** Endonym, shown in the language picker. */
  name: string;
}

export const SUPPORTED_LANGUAGES: LanguageOption[] = Object.entries(resources)
  .map(([code, ns]) => ({ code, name: (ns[DEFAULT_NAMESPACE].meta as { name: string }).name }))
  .sort((a, b) => (a.code === FALLBACK_LANGUAGE ? -1 : b.code === FALLBACK_LANGUAGE ? 1 : a.code.localeCompare(b.code)));

/** "system" follows the OS language; otherwise a code from SUPPORTED_LANGUAGES. */
export type LanguagePreference = "system" | string;

const STORAGE_KEY = "murmur.language";

/** Best supported locale for a BCP 47 tag, e.g. "zh-Hans-CN" → "zh". */
export function matchLanguage(tag: string | undefined): string | null {
  if (!tag) return null;
  const codes = SUPPORTED_LANGUAGES.map((l) => l.code);
  const lower = tag.toLowerCase();
  const exact = codes.find((c) => c.toLowerCase() === lower);
  if (exact) return exact;
  const base = lower.split("-")[0];
  return codes.find((c) => c.toLowerCase() === base) ?? null;
}

export function resolveLanguage(pref: LanguagePreference, systemLanguages: readonly string[]): string {
  if (pref !== "system") {
    const m = matchLanguage(pref);
    if (m) return m;
  }
  for (const tag of systemLanguages) {
    const m = matchLanguage(tag);
    if (m) return m;
  }
  return FALLBACK_LANGUAGE;
}

export function getLanguagePreference(): LanguagePreference {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored && matchLanguage(stored)) return stored;
  } catch {
    // Storage can be unavailable; fall back to the system language.
  }
  return "system";
}

export function setLanguagePreference(pref: LanguagePreference): Promise<unknown> {
  try {
    if (pref === "system") localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, pref);
  } catch {
    // Non-persistent preference still applies for this session.
  }
  return i18n.changeLanguage(resolveLanguage(pref, navigator.languages));
}

/** Locale code in use, for Intl APIs. */
export const currentLocale = () => i18n.resolvedLanguage ?? FALLBACK_LANGUAGE;

i18n.on("languageChanged", (lng) => {
  if (typeof document !== "undefined") document.documentElement.lang = lng;
});

void i18n.use(initReactI18next).init({
  resources,
  lng: resolveLanguage(getLanguagePreference(), navigator.languages),
  fallbackLng: FALLBACK_LANGUAGE,
  supportedLngs: Object.keys(resources),
  ns: NAMESPACES,
  defaultNS: DEFAULT_NAMESPACE,
  interpolation: { escapeValue: false }, // React already escapes
  initAsync: false,
});

export default i18n;
