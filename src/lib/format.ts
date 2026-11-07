import i18n, { currentLocale } from "../i18n";

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1000)));
  const value = bytes / 1000 ** i;
  return `${value >= 100 || i === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[i]}`;
}

export function formatClock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const mm = String(m).padStart(h > 0 ? 2 : 1, "0");
  const ss = String(sec).padStart(2, "0");
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

export function formatDuration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return i18n.t("duration.seconds", { count: s });
  const m = Math.floor(s / 60);
  if (m < 60) return i18n.t("duration.minutesSeconds", { minutes: m, seconds: s % 60 });
  return i18n.t("duration.hoursMinutes", { hours: Math.floor(m / 60), minutes: m % 60 });
}

export function formatEta(remainingBytes: number, bytesPerSec: number): string {
  if (bytesPerSec <= 0) return "";
  const s = remainingBytes / bytesPerSec;
  if (s < 60) return i18n.t("duration.etaSeconds", { count: Math.ceil(s) });
  return i18n.t("duration.etaMinutes", { count: Math.ceil(s / 60) });
}

export function formatRelativeDate(ms: number, now = Date.now()): string {
  const locale = currentLocale();
  const date = new Date(ms);
  const today = new Date(now);
  const startOfToday = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();
  const time = date.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit", hour12: false });
  if (ms >= startOfToday) return i18n.t("date.todayAt", { time });
  if (ms >= startOfToday - 86_400_000) return i18n.t("date.yesterdayAt", { time });
  if (date.getFullYear() === today.getFullYear()) {
    return date.toLocaleDateString(locale, { month: "short", day: "numeric" });
  }
  return date.toLocaleDateString(locale, { year: "numeric", month: "numeric", day: "numeric" });
}

export function liveTitle(now = new Date()): string {
  const locale = currentLocale();
  return i18n.t("liveTitle", {
    date: now.toLocaleDateString(locale, { month: "short", day: "numeric" }),
    time: now.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit", hour12: false }),
  });
}

const CJK = /[぀-ヿ㐀-䶿一-鿿가-힯]/;

/** Joins segment texts, using spaces only between non-CJK neighbours. */
export function joinSegments(texts: string[]): string {
  let out = "";
  for (const t of texts) {
    if (!t) continue;
    if (out && !CJK.test(out.slice(-1)) && !CJK.test(t[0])) out += " ";
    out += t;
  }
  return out;
}

export interface TextPiece {
  text: string;
  /** Index into the segment's words, or -1 for text no word covers. */
  word: number;
}

/**
 * Splits `text` into pieces carrying each timed word, so the original
 * punctuation and spacing survive even though aligners return bare words.
 * Words that cannot be found in order are skipped.
 */
export function alignWords(text: string, words: { text: string }[]): TextPiece[] {
  const pieces: TextPiece[] = [];
  const lower = text.toLowerCase();
  let pos = 0;
  words.forEach((w, i) => {
    const needle = w.text.trim().toLowerCase();
    if (!needle) return;
    const at = lower.indexOf(needle, pos);
    if (at < 0) return;
    if (at > pos) pieces.push({ text: text.slice(pos, at), word: -1 });
    let end = at + needle.length;
    // Keep trailing punctuation with its word so highlights don't flicker on it.
    while (end < text.length && /[^\s\p{L}\p{N}]/u.test(text[end])) end++;
    pieces.push({ text: text.slice(at, end), word: i });
    pos = end;
  });
  if (pos < text.length) pieces.push({ text: text.slice(pos), word: -1 });
  return pieces;
}

/** Index of the word playing at `time`, or -1. */
export function activeWord(words: { start: number; end: number }[], time: number): number {
  let found = -1;
  for (let i = 0; i < words.length; i++) {
    if (words[i].start > time) break;
    found = i;
  }
  if (found >= 0) {
    const next = words[found + 1];
    // Hold the last spoken word through short gaps, but not past the next word.
    if (time > words[found].end + 0.6 && (!next || time < next.start)) return -1;
  }
  return found;
}

/** Engine language names (and a few codes) to BCP 47 tags understood by Intl.DisplayNames. */
const LANGUAGE_TAGS: Record<string, string> = {
  Chinese: "zh",
  English: "en",
  Cantonese: "yue",
  Japanese: "ja",
  Korean: "ko",
  German: "de",
  French: "fr",
  Spanish: "es",
  Portuguese: "pt",
  Russian: "ru",
  Italian: "it",
  Arabic: "ar",
  Thai: "th",
  Vietnamese: "vi",
  Indonesian: "id",
  Turkish: "tr",
  Hindi: "hi",
  Malay: "ms",
  Dutch: "nl",
  Swedish: "sv",
  Danish: "da",
  Finnish: "fi",
  Polish: "pl",
  Czech: "cs",
  Filipino: "fil",
  Persian: "fa",
  Greek: "el",
  Romanian: "ro",
  Hungarian: "hu",
  Macedonian: "mk",
};

let displayNames: { locale: string; names: Intl.DisplayNames } | null = null;

/** Localized language name for an engine language code or name; unknown values pass through. */
export function languageLabel(code: string): string {
  if (code === "auto") return i18n.t("language.auto");
  const locale = currentLocale();
  if (displayNames?.locale !== locale) {
    displayNames = { locale, names: new Intl.DisplayNames([locale], { type: "language", fallback: "none" }) };
  }
  try {
    return displayNames.names.of(LANGUAGE_TAGS[code] ?? code) ?? code;
  } catch {
    return code; // not a valid language tag
  }
}

/** Languages a user can pick, with "auto" first and noise entries removed. */
export function selectableLanguages(languages: string[]): string[] {
  const hidden = new Set(["nospeech", "zh dialects", "Auto", "auto"]);
  return ["auto", ...languages.filter((l) => !hidden.has(l))];
}
