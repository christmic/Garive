import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const config = JSON.parse(readFileSync(fileURLToPath(
  new URL("../../backend/tauri.conf.json", import.meta.url)), "utf8"));
const css = readFileSync(fileURLToPath(new URL("./style.css", import.meta.url)), "utf8");
const entry = readFileSync(fileURLToPath(new URL("./main.tsx", import.meta.url)), "utf8");

describe("native macOS window composition", () => {
  it("keeps content under an overlay titlebar with correctly reserved traffic lights", () => {
    expect(config.app.windows[0]).toMatchObject({
      decorations: true,
      hiddenTitle: true,
      titleBarStyle: "Overlay",
      trafficLightPosition: { x: 16, y: 16 },
      acceptFirstMouse: true,
    });
    expect(css).toContain('html[data-client="desktop"] .sidebar-window-row { padding-left: 58px; }');
    expect(css).toContain('html[data-client="desktop"] .navigation-collapsed .topbar { padding-left: 70px; }');
    expect(css).toContain('html[data-client="desktop"] .topbar { padding-left: 70px; }');
    expect(css).toContain('html[data-client="desktop"] .sidebar-window-row > button { visibility: hidden; pointer-events: none; }');
    expect(css).toContain('html[data-client="desktop"] .sidebar-window-row { display: none; }');
    expect(entry).toContain('document.documentElement.dataset.client = "desktop";');
  });
});
