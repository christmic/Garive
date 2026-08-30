import type { DesktopMenuIntent } from "./desktopMenu";

export const DESKTOP_ZOOM_STEPS = [0.8, 1, 1.2, 1.5, 1.75, 2] as const;
export type DesktopZoomIntent = Extract<
  DesktopMenuIntent,
  "desktop.zoom-in" | "desktop.zoom-out" | "desktop.actual-size"
>;

/** Resolves one bounded native WebView zoom step without retaining user data. */
export function nextDesktopZoom(current: number, intent: DesktopZoomIntent): number {
  if (intent === "desktop.actual-size") return 1;
  if (intent === "desktop.zoom-in") {
    return DESKTOP_ZOOM_STEPS.find((step) => step > current) ?? DESKTOP_ZOOM_STEPS.at(-1)!;
  }
  return DESKTOP_ZOOM_STEPS.findLast((step) => step < current) ?? DESKTOP_ZOOM_STEPS[0];
}
