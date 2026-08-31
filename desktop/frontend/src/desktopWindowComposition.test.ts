import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const config = JSON.parse(readFileSync(fileURLToPath(
  new URL("../../backend/tauri.conf.json", import.meta.url)), "utf8"));
const css = readFileSync(fileURLToPath(new URL("./style.css", import.meta.url)), "utf8");

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
  });
});
