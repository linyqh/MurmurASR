import { describe, expect, it } from "vitest";
import catalog from "../../src-tauri/catalog.json";
import i18n, { FALLBACK_LANGUAGE, SUPPORTED_LANGUAGES, NAMESPACES, resolveLanguage } from "./index";
import { errorMessage } from "../lib/errors";

const keyPaths = (o: unknown, prefix = ""): string[] =>
  o && typeof o === "object"
    ? Object.entries(o).flatMap(([k, v]) => keyPaths(v, prefix ? `${prefix}.${k}` : k))
    : [prefix];

// Plural suffixes legitimately differ between languages.
const base = (path: string) => path.replace(/_(zero|one|two|few|many|other)$/, "");

describe("locales", () => {
  const sets = (lng: string, ns: string) =>
    new Set(keyPaths(i18n.getResourceBundle(lng, ns)).map(base));

  it.each(SUPPORTED_LANGUAGES.map((l) => l.code))("%s has every key of the fallback language", (lng) => {
    for (const ns of NAMESPACES) {
      if (ns === "catalog") continue;
      expect([...sets(FALLBACK_LANGUAGE, ns)].filter((k) => !sets(lng, ns).has(k)), `${lng}/${ns}`).toEqual([]);
      expect([...sets(lng, ns)].filter((k) => !sets(FALLBACK_LANGUAGE, ns).has(k)), `${lng}/${ns} extra`).toEqual([]);
    }
  });

  it.each(SUPPORTED_LANGUAGES.map((l) => l.code))("%s translates the whole model catalog", (lng) => {
    const bundle = i18n.getResourceBundle(lng, "catalog");
    for (const m of catalog.models) {
      for (const f of ["summary", "description", "languages"]) expect(bundle.models[m.id]?.[f], `${m.id}.${f}`).toBeTruthy();
      expect(bundle.groups[m.group], m.group).toBeTruthy();
      for (const t of m.tags) expect(bundle.tags[t], t).toBeTruthy();
    }
  });

  it("keeps interpolation placeholders identical across languages", () => {
    const placeholders = (s: unknown) => String(s).match(/{{\w+}}/g)?.sort().join() ?? "";
    const at = (o: unknown, path: string) => path.split(".").reduce<any>((x, k) => x?.[k], o);
    for (const ns of NAMESPACES.filter((n) => n !== "catalog")) {
      const en = i18n.getResourceBundle(FALLBACK_LANGUAGE, ns);
      for (const { code } of SUPPORTED_LANGUAGES) {
        const other = i18n.getResourceBundle(code, ns);
        for (const path of keyPaths(en)) {
          const translated = at(other, path);
          if (translated !== undefined) expect(placeholders(translated), `${code}/${ns}/${path}`).toBe(placeholders(at(en, path)));
        }
      }
    }
  });
});

describe("resolveLanguage", () => {
  it("honours an explicit choice, then the system language, then English", () => {
    expect(resolveLanguage("zh", ["en-US"])).toBe("zh");
    expect(resolveLanguage("system", ["zh-Hans-CN", "en"])).toBe("zh");
    expect(resolveLanguage("system", ["fr-FR", "en-GB"])).toBe("en");
    expect(resolveLanguage("system", ["fr-FR"])).toBe("en");
    expect(resolveLanguage("xx", [])).toBe("en");
  });
});

describe("errorMessage", () => {
  it("translates known backend errors, including nested details", async () => {
    await i18n.changeLanguage("zh");
    expect(errorMessage(new Error("Load a model first"))).toBe("请先加载一个模型");
    expect(errorMessage("Unsupported audio format: No audio track")).toBe("不支持的音频格式：没有音轨");
    expect(errorMessage("something unexpected")).toBe("something unexpected");
    await i18n.changeLanguage("en");
    expect(errorMessage("Port 8080 is unavailable: in use")).toBe("Port 8080 is unavailable: in use");
  });
});
