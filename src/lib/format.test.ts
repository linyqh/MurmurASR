import { describe, expect, it } from "vitest";
import {
  activeWord,
  alignWords,
  formatBytes,
  formatClock,
  formatRelativeDate,
  joinSegments,
  selectableLanguages,
} from "./format";

describe("formatBytes", () => {
  it("uses decimal units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1_151_272_416)).toBe("1.2 GB");
    expect(formatBytes(254_211_200)).toBe("254 MB");
  });
});

describe("formatClock", () => {
  it("formats minutes and hours", () => {
    expect(formatClock(5)).toBe("0:05");
    expect(formatClock(65.9)).toBe("1:05");
    expect(formatClock(3661)).toBe("1:01:01");
  });
});

describe("joinSegments", () => {
  it("adds spaces only between latin text", () => {
    expect(joinSegments(["Hello", "world"])).toBe("Hello world");
    expect(joinSegments(["你好。", "世界"])).toBe("你好。世界");
    expect(joinSegments(["你好", "world"])).toBe("你好world");
  });
});

describe("selectableLanguages", () => {
  it("puts auto first and hides internal entries", () => {
    expect(selectableLanguages(["Auto", "Chinese", "English", "zh dialects"])).toEqual([
      "auto",
      "Chinese",
      "English",
    ]);
    expect(selectableLanguages(["auto", "zh", "nospeech"])).toEqual(["auto", "zh"]);
  });
});

describe("formatRelativeDate", () => {
  it("labels today and yesterday", () => {
    const now = new Date(2026, 9, 15, 15, 0).getTime();
    expect(formatRelativeDate(new Date(2026, 9, 15, 9, 5).getTime(), now)).toBe("今天 09:05");
    expect(formatRelativeDate(new Date(2026, 9, 14, 22, 30).getTime(), now)).toBe("昨天 22:30");
    expect(formatRelativeDate(new Date(2026, 2, 1).getTime(), now)).toBe("3月1日");
  });
});

describe("alignWords", () => {
  it("keeps punctuation and spacing from the original text", () => {
    const pieces = alignWords("Hello, world.", [{ text: "hello" }, { text: "world" }]);
    expect(pieces).toEqual([
      { text: "Hello,", word: 0 },
      { text: " ", word: -1 },
      { text: "world.", word: 1 },
    ]);
    expect(pieces.map((p) => p.text).join("")).toBe("Hello, world.");
  });

  it("aligns CJK characters and skips words it cannot find", () => {
    const pieces = alignWords("你好，AI。", [{ text: "你" }, { text: "好" }, { text: "zz" }, { text: "AI" }]);
    expect(pieces.map((p) => [p.text, p.word])).toEqual([
      ["你", 0],
      ["好，", 1],
      ["AI。", 3],
    ]);
  });
});

describe("activeWord", () => {
  const words = [
    { start: 1, end: 1.4 },
    { start: 1.5, end: 2 },
    { start: 5, end: 5.5 },
  ];
  it("tracks the word under the playhead", () => {
    expect(activeWord(words, 0.5)).toBe(-1);
    expect(activeWord(words, 1.45)).toBe(0);
    expect(activeWord(words, 1.7)).toBe(1);
    expect(activeWord(words, 3)).toBe(-1);
    expect(activeWord(words, 5.2)).toBe(2);
  });
});

describe("alignWords with aligner-merged tokens", () => {
  it("skips words the aligner glued across punctuation without losing text", () => {
    const text = "the common life—we meet";
    const pieces = alignWords(text, [{ text: "the" }, { text: "common" }, { text: "lifewe" }, { text: "meet" }]);
    expect(pieces.map((p) => p.text).join("")).toBe(text);
    expect(pieces.filter((p) => p.word >= 0).map((p) => p.text.trim())).toEqual(["the", "common", "meet"]);
  });
});
