export const DESKTOP_MENU_EVENT = "desktop-menu";

export type DesktopMenuIntent =
  | "desktop.new-work"
  | "desktop.search"
  | "desktop.settings"
  | "desktop.toggle-inspector";

/** Accepts only the closed native menu intent set; payloads never carry data. */
export function decodeDesktopMenuIntent(value: unknown): DesktopMenuIntent | undefined {
  return value === "desktop.new-work" || value === "desktop.search"
    || value === "desktop.settings" || value === "desktop.toggle-inspector"
    ? value : undefined;
}
