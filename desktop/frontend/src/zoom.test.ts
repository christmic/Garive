import { describe, expect, it } from "vitest";
import { nextDesktopZoom } from "./zoom";

describe("Desktop zoom", () => {
  it("uses the installed Codex ten-percent bounded WebView ramp", () => {
    expect(nextDesktopZoom(1, "desktop.zoom-in")).toBe(1.1);
    expect(nextDesktopZoom(1.1, "desktop.zoom-out")).toBe(1);
    expect(nextDesktopZoom(1.5, "desktop.actual-size")).toBe(1);
    expect(nextDesktopZoom(3, "desktop.zoom-in")).toBe(3);
    expect(nextDesktopZoom(0.5, "desktop.zoom-out")).toBe(0.5);
  });

  it("rounds every relative step to two decimal places", () => {
    expect(nextDesktopZoom(1.31, "desktop.zoom-in")).toBe(1.41);
    expect(nextDesktopZoom(1.31, "desktop.zoom-out")).toBe(1.21);
  });
});
