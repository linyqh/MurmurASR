import { describe, expect, it } from "vitest";
import {
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
