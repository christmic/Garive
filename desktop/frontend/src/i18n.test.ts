import { describe, expect, it } from "vitest";
import { createTranslator, resolveDesktopLocale } from "./i18n";

describe("Desktop localization", () => {
  it("resolves supported system languages deterministically", () => {
    expect(resolveDesktopLocale("system", ["zh-CN", "en-US"])).toBe("zh-Hans");
    expect(resolveDesktopLocale("system", ["fr-FR", "en-GB"])).toBe("en");
    expect(resolveDesktopLocale("zh-Hans", ["en-US"])).toBe("zh-Hans");
  });

  it("translates stable shell keys in English and Simplified Chinese", () => {
    expect(createTranslator("en")("nav.settings")).toBe("Settings");
    expect(createTranslator("zh-Hans")("nav.settings")).toBe("设置");
    expect(createTranslator("zh-Hans")("settings.language.description"))
      .toContain("macOS");
  });

  it("provides an expanded pseudolocale without exposing internal keys", () => {
    const translated = createTranslator("en-XA")("nav.settings");
    expect(translated).toMatch(/^\[.*\]$/);
    expect(translated.length).toBeGreaterThan("Settings".length);
    expect(translated).not.toContain("nav.settings");
  });
});
