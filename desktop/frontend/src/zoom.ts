import type { DesktopMenuIntent } from "./desktopMenu";

export const DESKTOP_ZOOM_MIN = 0.5;
export const DESKTOP_ZOOM_MAX = 3;
export const DESKTOP_ZOOM_STEP = 0.1;
export type DesktopZoomIntent = Extract<
  DesktopMenuIntent,
  "desktop.zoom-in" | "desktop.zoom-out" | "desktop.actual-size"
>;

/** Resolves one bounded native WebView zoom step without retaining user data. */
export function nextDesktopZoom(current: number, intent: DesktopZoomIntent): number {
  if (intent === "desktop.actual-size") return 1;
  const delta = intent === "desktop.zoom-in" ? DESKTOP_ZOOM_STEP : -DESKTOP_ZOOM_STEP;
  return Math.round(Math.min(DESKTOP_ZOOM_MAX,
    Math.max(DESKTOP_ZOOM_MIN, current + delta)) * 100) / 100;
}
