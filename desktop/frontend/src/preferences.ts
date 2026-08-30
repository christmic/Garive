export type DesktopTheme = "system" | "light" | "dark";
export type DesktopDensity = "comfortable" | "compact";
export type DesktopLocalePreference = "system" | "en" | "zh-Hans" | "en-XA";

export interface DesktopPreferences {
  readonly schema_version: 2;
  readonly theme: DesktopTheme;
  readonly density: DesktopDensity;
  readonly locale: DesktopLocalePreference;
}

export const DEFAULT_DESKTOP_PREFERENCES: DesktopPreferences = {
  schema_version: 2, theme: "system", density: "comfortable", locale: "system",
};

const STORAGE_KEY = "garive.desktop.preferences.v1";

/** Reads only the admitted non-sensitive Desktop appearance fields. */
export function readDesktopPreferences(
  storage: Pick<Storage, "getItem"> = window.localStorage,
): DesktopPreferences {
  try {
    const encoded = storage.getItem(STORAGE_KEY);
    if (!encoded || encoded.length > 256) return DEFAULT_DESKTOP_PREFERENCES;
    const value = JSON.parse(encoded) as Record<string, unknown>;
    const keys = Object.keys(value).sort().join(",");
    if (keys === "density,schema_version,theme" && value.schema_version === 1
        && matchesTheme(value.theme) && matchesDensity(value.density)) {
      return { schema_version: 2, theme: value.theme, density: value.density, locale: "system" };
    }
    if (keys !== "density,locale,schema_version,theme" || value.schema_version !== 2
        || !matchesTheme(value.theme) || !matchesDensity(value.density)
        || !matchesLocale(value.locale)) return DEFAULT_DESKTOP_PREFERENCES;
    return { schema_version: 2, theme: value.theme, density: value.density,
      locale: value.locale };
  } catch { return DEFAULT_DESKTOP_PREFERENCES; }
}

/** Persists no value beyond the strict appearance contract. */
export function writeDesktopPreferences(
  preferences: DesktopPreferences,
  storage: Pick<Storage, "setItem"> = window.localStorage,
) {
  storage.setItem(STORAGE_KEY, JSON.stringify(preferences));
}

function matchesTheme(value: unknown): value is DesktopTheme {
  return value === "system" || value === "light" || value === "dark";
}

function matchesDensity(value: unknown): value is DesktopDensity {
  return value === "comfortable" || value === "compact";
}

function matchesLocale(value: unknown): value is DesktopLocalePreference {
  return value === "system" || value === "en" || value === "zh-Hans" || value === "en-XA";
}
