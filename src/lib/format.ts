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
  if (s < 60) return `${s} 秒`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} 分 ${s % 60} 秒`;
  return `${Math.floor(m / 60)} 小时 ${m % 60} 分`;
}

export function formatEta(remainingBytes: number, bytesPerSec: number): string {
  if (bytesPerSec <= 0) return "";
  const s = remainingBytes / bytesPerSec;
  if (s < 60) return `剩余约 ${Math.ceil(s)} 秒`;
  return `剩余约 ${Math.ceil(s / 60)} 分钟`;
}

export function formatRelativeDate(ms: number, now = Date.now()): string {
  const date = new Date(ms);
  const today = new Date(now);
  const startOfToday = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();
  const time = date.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false });
  if (ms >= startOfToday) return `今天 ${time}`;
  if (ms >= startOfToday - 86_400_000) return `昨天 ${time}`;
  if (date.getFullYear() === today.getFullYear()) {
    return `${date.getMonth() + 1}月${date.getDate()}日`;
  }
  return `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()}`;
}

export function liveTitle(now = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `实时转写 ${now.getMonth() + 1}月${now.getDate()}日 ${pad(now.getHours())}:${pad(now.getMinutes())}`;
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

export const LANGUAGE_LABELS: Record<string, string> = {
  auto: "自动检测",
  zh: "中文",
  en: "英语",
  yue: "粤语",
  ja: "日语",
  ko: "韩语",
  de: "德语",
  fr: "法语",
  es: "西班牙语",
  pt: "葡萄牙语",
  ru: "俄语",
  it: "意大利语",
  ar: "阿拉伯语",
  Chinese: "中文",
  English: "英语",
  Cantonese: "粤语",
  Japanese: "日语",
  Korean: "韩语",
  German: "德语",
  French: "法语",
  Spanish: "西班牙语",
  Portuguese: "葡萄牙语",
  Russian: "俄语",
  Italian: "意大利语",
  Arabic: "阿拉伯语",
  Thai: "泰语",
  Vietnamese: "越南语",
  Indonesian: "印尼语",
  Turkish: "土耳其语",
  Hindi: "印地语",
  Malay: "马来语",
  Dutch: "荷兰语",
  Swedish: "瑞典语",
  Danish: "丹麦语",
  Finnish: "芬兰语",
  Polish: "波兰语",
  Czech: "捷克语",
  Filipino: "菲律宾语",
  Persian: "波斯语",
  Greek: "希腊语",
  Romanian: "罗马尼亚语",
  Hungarian: "匈牙利语",
  Macedonian: "马其顿语",
  "zh-CN": "中文（普通话）",
  "en-US": "英语（美国）",
  "en-GB": "英语（英国）",
  "ja-JP": "日语",
  "ko-KR": "韩语",
};

export function languageLabel(code: string): string {
  return LANGUAGE_LABELS[code] ?? code;
}

/** Languages a user can pick, with "auto" first and noise entries removed. */
export function selectableLanguages(languages: string[]): string[] {
  const hidden = new Set(["nospeech", "zh dialects", "Auto", "auto"]);
  return ["auto", ...languages.filter((l) => !hidden.has(l))];
}
