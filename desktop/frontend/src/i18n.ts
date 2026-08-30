import type { DesktopLocalePreference } from "./preferences";

const english = {
  "nav.newWork": "New work",
  "nav.work": "Work",
  "nav.search": "Search",
  "nav.agents": "Agents",
  "nav.settings": "Settings",
  "nav.recents": "Recents",
  "nav.library": "Library",
  "settings.eyebrow": "DESKTOP",
  "settings.title": "Settings",
  "settings.appearance.title": "Appearance",
  "settings.appearance.description":
    "Match macOS automatically or keep an explicit theme and information density.",
  "settings.theme": "Theme",
  "settings.theme.system": "System",
  "settings.theme.light": "Light",
  "settings.theme.dark": "Dark",
  "settings.density": "Density",
  "settings.density.comfortable": "Comfortable",
  "settings.density.compact": "Compact",
  "settings.language.title": "Language",
  "settings.language.description":
    "Follow macOS automatically or choose the language used by Garive.",
  "settings.language.label": "App language",
  "settings.language.system": "System",
  "settings.language.en": "English",
  "settings.language.zh-Hans": "简体中文",
  "settings.language.en-XA": "Pseudolocale",
} as const;

export type MessageKey = keyof typeof english;
export type ResolvedDesktopLocale = "en" | "zh-Hans" | "en-XA";

const simplifiedChinese: Record<MessageKey, string> = {
  "nav.newWork": "新建工作",
  "nav.work": "工作",
  "nav.search": "搜索",
  "nav.agents": "智能体",
  "nav.settings": "设置",
  "nav.recents": "最近工作",
  "nav.library": "资料库",
  "settings.eyebrow": "桌面端",
  "settings.title": "设置",
  "settings.appearance.title": "外观",
  "settings.appearance.description": "自动跟随 macOS，或单独设置主题与信息密度。",
  "settings.theme": "主题",
  "settings.theme.system": "跟随系统",
  "settings.theme.light": "浅色",
  "settings.theme.dark": "深色",
  "settings.density": "密度",
  "settings.density.comfortable": "舒适",
  "settings.density.compact": "紧凑",
  "settings.language.title": "语言",
  "settings.language.description": "自动跟随 macOS，或选择 Garive 使用的语言。",
  "settings.language.label": "应用语言",
  "settings.language.system": "跟随系统",
  "settings.language.en": "English",
  "settings.language.zh-Hans": "简体中文",
  "settings.language.en-XA": "伪本地化",
};

export function resolveDesktopLocale(
  preference: DesktopLocalePreference,
  systemLanguages: readonly string[] = navigator.languages,
): ResolvedDesktopLocale {
  if (preference !== "system") return preference;
  return systemLanguages.some((language) => /^zh(?:-|$)/i.test(language)) ? "zh-Hans" : "en";
}

export function createTranslator(locale: ResolvedDesktopLocale) {
  return (key: MessageKey): string => {
    const message = locale === "zh-Hans" ? simplifiedChinese[key] : english[key];
    return locale === "en-XA" ? pseudolocalize(message) : message;
  };
}

function pseudolocalize(message: string): string {
  const accents: Record<string, string> = {
    a: "á", e: "ë", i: "ï", o: "ô", u: "ü", A: "Á", E: "Ë", I: "Ï", O: "Ô", U: "Ü",
  };
  const expanded = [...message].map((character) => accents[character] ?? character).join("");
  return `[${expanded}··]`;
}
