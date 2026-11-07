import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { FALLBACK_LANGUAGE } from "../i18n";
import type { CatalogModel } from "./api";

type Bundle = {
  groups?: Record<string, string>;
  tags?: Record<string, string>;
  models?: Record<string, Partial<Record<"summary" | "description" | "languages", string>>>;
};

/**
 * Localized text for catalog.json entries. Model ids contain dots, so the
 * `catalog` namespace is read directly instead of through `t()` key paths.
 * Falls back to the fallback language, then to the text shipped in the catalog.
 */
export function useCatalogText() {
  const { i18n } = useTranslation("catalog");
  const lng = i18n.resolvedLanguage ?? FALLBACK_LANGUAGE;
  return useMemo(() => {
    const bundles = [lng, FALLBACK_LANGUAGE].map(
      (l) => (i18n.getResourceBundle(l, "catalog") ?? {}) as Bundle,
    );
    const pick = (read: (b: Bundle) => string | undefined, original: string) =>
      bundles.reduce<string | undefined>((found, b) => found ?? read(b), undefined) ?? original;
    const field = (m: CatalogModel, key: "summary" | "description" | "languages") =>
      pick((b) => b.models?.[m.id]?.[key], m[key]);
    return {
      summary: (m: CatalogModel) => field(m, "summary"),
      description: (m: CatalogModel) => field(m, "description"),
      languages: (m: CatalogModel) => field(m, "languages"),
      group: (name: string) => pick((b) => b.groups?.[name], name),
      tag: (name: string) => pick((b) => b.tags?.[name], name),
    };
  }, [i18n, lng]);
}
