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

  it("covers the primary Work, Search, and Agents journeys", () => {
    const t = createTranslator("zh-Hans");
    expect(t("work.welcome.title")).toBe("这次要完成什么？");
    expect(t("work.composer.commitNote")).toContain("本地运行时提交");
    expect(t("search.description")).toContain("不会创建云端索引");
    expect(t("agents.description")).toContain("新工作");
  });

  it("preserves approval and Artifact trust semantics in Chinese", () => {
    const t = createTranslator("zh-Hans");
    expect(t("approval.durationValue")).toContain("仅限当前已准备调用");
    expect(t("approval.overwriteValue")).toBe("绝不覆盖");
    expect(t("artifact.overwriteError")).toContain("绝不覆盖");
    expect(t("artifact.previewVerified")).toBe("已验证预览");
  });
});
