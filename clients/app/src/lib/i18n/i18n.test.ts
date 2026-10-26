import { describe, expect, it } from "vitest";

import { createI18n, detectLocale, formatDay, isLocale, mergeMessages, SUPPORTED_LOCALES } from ".";

const each = (messages: Record<string, unknown>) =>
  Object.fromEntries(SUPPORTED_LOCALES.map((locale) => [locale, messages])) as Record<
    (typeof SUPPORTED_LOCALES)[number],
    Record<string, unknown>
  >;

describe("the locale", () => {
  it("prefers the saved choice, then the closest browser language, then English", () => {
    expect(detectLocale("fr", ["pt-PT"])).toBe("fr");
    expect(detectLocale("de", ["pt-BR", "en"])).toBe("pt-PT");
    expect(detectLocale(null, ["es-MX"])).toBe("es-ES");
    expect(detectLocale(null, ["de-DE", "ja"])).toBe("en");
    expect(isLocale("pt")).toBe(false);
  });

  it("merges our strings over the package's, key by key", () => {
    const merged = mergeMessages(
      each({ menu: { label: "Command menu", ask: "Ask Scout" }, close: "Close" }),
      each({ menu: { ask: "Ask" } }),
    );
    expect(merged.fr).toEqual({ menu: { label: "Command menu", ask: "Ask" }, close: "Close" });
  });

  it("starts synchronously with every namespace", () => {
    const i18n = createI18n({
      locale: "pt-PT",
      namespaces: { shell: { ...each({ appName: "Applicamp" }), "pt-PT": { appName: "Olá" } } },
    });
    expect(i18n.t("appName")).toBe("Olá");
    expect(i18n.language).toBe("pt-PT");
  });

  it("says a day in the UI's language", () => {
    const day = new Date(2026, 9, 25, 12);
    expect(formatDay("en", day)).toBe("Sunday, October 25");
    expect(formatDay("pt-PT", day)).toBe("domingo, 25 de outubro");
    expect(formatDay("en")).not.toBe("");
  });
});
