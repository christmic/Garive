import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const config = JSON.parse(readFileSync(fileURLToPath(
  new URL("../../backend/tauri.conf.json", import.meta.url)), "utf8"));
const css = readFileSync(fileURLToPath(new URL("./style.css", import.meta.url)), "utf8");
const entry = readFileSync(fileURLToPath(new URL("./main.tsx", import.meta.url)), "utf8");
const app = readFileSync(fileURLToPath(new URL("./App.tsx", import.meta.url)), "utf8");
const backend = readFileSync(fileURLToPath(
  new URL("../../backend/src/main.rs", import.meta.url)), "utf8");
const manifest = readFileSync(fileURLToPath(
  new URL("../../backend/Cargo.toml", import.meta.url)), "utf8");

describe("native macOS window composition", () => {
  it("keeps content under an overlay titlebar with correctly reserved traffic lights", () => {
    expect(config.app.macOSPrivateApi).toBe(true);
    expect(manifest).toContain('tauri = { version = "2.11.5", features = ["macos-private-api"] }');
    expect(config.app.windows[0]).toMatchObject({
      decorations: true,
      hiddenTitle: true,
      titleBarStyle: "Overlay",
      trafficLightPosition: { x: 16, y: 16 },
      acceptFirstMouse: true,
      width: 1280,
      height: 820,
      minWidth: 480,
      minHeight: 600,
      transparent: true,
      windowEffects: {
        effects: ["menu"],
        state: "followsWindowActiveState",
      },
      visible: false,
    });
    expect(css).toContain('html[data-client="desktop"], html[data-client="desktop"] body, html[data-client="desktop"] #root { background: transparent; }');
    expect(css).toContain('html[data-client="desktop"] .desktop-root, html[data-client="desktop"] .app-shell { background: transparent; }');
    expect(css).toContain('html[data-client="desktop"] .sidebar { background: var(--surface-native-sidebar); }');
    expect(css).toContain('@media (prefers-reduced-transparency: reduce)');
    expect(css).toContain('html[data-client="desktop"] .sidebar { background: var(--surface-sidebar); }');
    expect(css).toContain('html[data-client="desktop"] .sidebar-window-row { padding-left: 58px; }');
    expect(css).toContain('html[data-client="desktop"] .navigation-collapsed .topbar { padding-left: 70px; }');
    expect(css).toContain('html[data-client="desktop"] .topbar { padding-left: 70px; }');
    expect(css).toContain('html[data-client="desktop"] .sidebar-window-row > button { visibility: hidden; pointer-events: none; }');
    expect(css).toContain('html[data-client="desktop"] .sidebar-window-row { display: none; }');
    expect(app).toContain('className="sidebar-window-row" data-tauri-drag-region="deep"');
    expect(app).toContain('className="topbar" data-tauri-drag-region="deep"');
    expect(app).not.toContain('className="titlebar-drag"');
    expect(entry).toContain('document.documentElement.dataset.client = "desktop";');
  });

  it("restores only the admitted main-window frame without delegating chrome", () => {
    expect(manifest).toContain('tauri-plugin-window-state = "2.4.1"');
    expect(backend).toContain("tauri_plugin_window_state::Builder::default()");
    for (const flag of ["SIZE", "POSITION", "MAXIMIZED", "FULLSCREEN", "VISIBLE"]) {
      expect(backend).toContain(`tauri_plugin_window_state::StateFlags::${flag}`);
    }
    expect(backend).not.toContain("StateFlags::DECORATIONS");
    expect(backend).toContain('.with_filter(|label| label == "main")');
  });
});
