import { describe, expect, it } from "vitest";
import { nextDesktopZoom } from "./zoom";

describe("Desktop zoom", () => {
  it("uses deterministic bounded native WebView zoom steps", () => {
    expect(nextDesktopZoom(1, "desktop.zoom-in")).toBe(1.2);
    expect(nextDesktopZoom(1.2, "desktop.zoom-out")).toBe(1);
    expect(nextDesktopZoom(1.5, "desktop.actual-size")).toBe(1);
    expect(nextDesktopZoom(2, "desktop.zoom-in")).toBe(2);
    expect(nextDesktopZoom(0.8, "desktop.zoom-out")).toBe(0.8);
  });

  it("normalizes an off-step current value before moving", () => {
    expect(nextDesktopZoom(1.31, "desktop.zoom-in")).toBe(1.5);
    expect(nextDesktopZoom(1.31, "desktop.zoom-out")).toBe(1.2);
  });
});
