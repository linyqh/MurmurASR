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
