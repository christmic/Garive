import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const CSS = readFileSync(fileURLToPath(new URL("./style.css", import.meta.url)), "utf8");
const TOKENS = readFileSync(fileURLToPath(new URL("./visual-system.css", import.meta.url)), "utf8");

describe("Desktop visual accessibility contract", () => {
  it("retains explicit focus, motion, transparency, contrast and forced-color modes", () => {
    for (const contract of [":focus-visible", "prefers-reduced-motion: reduce",
      "prefers-reduced-transparency: reduce", "prefers-contrast: more", "forced-colors: active"]) {
      expect(CSS).toContain(contract);
    }
  });

  it("reflows the complete work surface at the 320 CSS pixel boundary", () => {
    const narrow = media("max-width: 480px");
    expect(narrow).toContain("grid-template-columns: minmax(0, 1fr)");
    expect(narrow).toContain(".sidebar { display: none; }");
    expect(narrow).toContain(".main-surface, .topbar, .work-surface, .conversation, .composer-wrap, .composer, .timeline { min-width: 0; max-width: 100%; }");
    expect(narrow).toContain(".approval-foot { flex-direction: column; align-items: stretch; }");
    expect(narrow).toContain(".approval-actions { align-self: flex-end; }");
    expect(CSS).toContain(".app-shell, .app-shell:has(.environment-panel), .app-shell:has(.workspace-panel) { grid-template-columns: minmax(0, 1fr); }");
    expect(CSS).toContain(".app-shell, .app-shell:has(.environment-panel), .app-shell:has(.workspace-panel) { grid-template-columns: 72px minmax(0, 1fr); }");
  });

  it("keeps normal semantic text tokens at WCAG AA contrast in light and dark", () => {
    expect(contrast("5f5f5f", "ffffff")).toBeGreaterThanOrEqual(4.5);
    expect(contrast("707070", "ffffff")).toBeGreaterThanOrEqual(4.5);
    expect(contrast("bababa", "181818")).toBeGreaterThanOrEqual(4.5);
    expect(contrast("8c8c8c", "181818")).toBeGreaterThanOrEqual(4.5);
  });

  it("freezes the shared Codex-fidelity scale and wide shell geometry", () => {
    for (const token of ["--text-2xs: 11px", "--text-xs: 12px", "--text-sm: 13px",
      "--surface-canvas: #ffffff", "--surface-sidebar: #f7f7f7",
      "--surface-canvas: #181818", "--surface-sidebar: #211f20"]) {
      expect(TOKENS).toContain(token);
    }
    expect(TOKENS).toContain("--document-font-size: 13px");
    expect(TOKENS).toContain("--document-leading: 1.625");
    expect(TOKENS).toContain("--height-window-bar: 34px");
    expect(TOKENS).toContain("--height-file-toolbar: 30px");
    expect(TOKENS).toContain("--radius-composer: 20px");
    expect(TOKENS).toContain("--shadow-composer: 0 0 0 1px rgba(0, 0, 0, .04)");
    expect(TOKENS).toContain("--shadow-composer: inset 0 0 1px rgba(255, 255, 255, .2)");
    expect(TOKENS).toContain("--surface-composer: rgba(255, 255, 255, .03)");
    expect(TOKENS).toContain("@supports (corner-shape: superellipse(1.5))");
    expect(CSS).toContain("--sidebar-width: clamp(206px, 16.1vw, 240px)");
    expect(CSS).toContain("--conversation-split: 352px");
    expect(CSS).toContain(".app-shell:has(.workspace-panel)");
    expect(CSS).toContain("var(--conversation-split) minmax(500px, 1fr)");
    expect(CSS).toContain(".work-surface { min-width: 0; overflow: hidden; }");
    expect(CSS).toContain(".workspace-resizer { position: absolute");
    expect(CSS).toContain(".artifact-preview-content { width: min(46rem, 100%)");
    expect(CSS).toContain("font-size: calc(var(--document-font-size) * 1.5)");
    expect(CSS).toContain("font-size: calc(var(--document-font-size) * 1.25)");
    expect(CSS).toContain(".artifact-preview-content blockquote");
    expect(CSS).toContain(".artifact-preview-content pre { max-height: none;");
    expect(CSS).toContain(".artifact-preview-content .document-code-block");
    expect(CSS).toContain(".result-markdown { font-size: var(--document-font-size); line-height: var(--document-leading); }");
    expect(CSS).toContain(".result-markdown h1 { margin: 0 0 calc(var(--document-space) * 2);");
    expect(CSS).toContain(".assistant-message .result-markdown p { font-size: inherit; line-height: inherit; white-space: normal; }");
    expect(CSS).toContain(".artifact-workbench-toolbar { display: flex;");
    expect(CSS).toContain("min-height: var(--height-file-toolbar);");
    expect(CSS).toContain(".app-shell:has(.workspace-panel) .timeline { width: calc(100% - 20px); }");
    expect(CSS).toContain("grid-template-columns: minmax(0, 1fr); grid-template-rows: var(--height-window-bar) minmax(0, 1fr)");
    expect(CSS).toContain(".navigation-collapsed > .sidebar { display: none; }");
    expect(CSS).toContain(".environment-panel { position: absolute");
    expect(CSS).toContain("@keyframes environment-enter");
    expect(CSS).toContain("@keyframes workspace-content-enter");
    expect(CSS).toContain(".app-shell:has(.environment-panel) .work-surface { margin-right: 236px; }");
    expect(CSS).toContain("width: min(39rem, calc(100% - 48px))");
    expect(CSS).toContain(".settings-workbench { display: grid; grid-template-columns: 164px minmax(0, 1fr)");
    expect(CSS).toContain(".settings-panel { min-width: 0; overflow: auto;");
    expect(CSS).toContain(".settings-navigation { display: flex;");
    expect(CSS).toContain(".welcome { width: min(39rem, 100%);");
    expect(CSS).toContain(".new-work-surface .composer-wrap { top: clamp(210px, 27vh, 236px);");
    expect(CSS).toContain("grid-template-columns: 78px minmax(0, 1fr) 16px");
    expect(CSS).toContain(".new-work-surface .composer-wrap { top: 190px; }");
    expect(CSS).toContain(".setup-shell { position: relative; min-height: 100%; overflow: auto; padding: 46px 24px 64px; background: var(--surface-canvas);");
    expect(CSS).toContain(".setup-card { position: relative; width: min(39rem, 100%); margin: 0 auto;");
    expect(CSS).toContain(".setup-card::before { display: none; }");
    expect(CSS).toContain(".setup-grid { display: grid; grid-template-columns: 1fr 1fr;");
    expect(CSS).toContain(".approval-card { display: grid; grid-template-columns: 24px minmax(0,1fr);");
    expect(CSS).toContain("border-left-color: var(--state-attention)");
    expect(CSS).toContain(".approval-foot { display: flex; align-items: center; justify-content: space-between;");
    expect(CSS).toContain(".workspace-sheet { display: grid; grid-template-rows: 48px 34px minmax(220px, 1fr) auto;");
    expect(CSS).toContain("width: min(620px, 100%)");
    expect(CSS).toContain(".workspace-entry { display: grid; grid-template-columns: minmax(0, 1fr) 34px;");
    expect(CSS).toContain(".entry-icon { display: grid; place-items: center; width: 24px; height: 24px; color: var(--text-tertiary); background: transparent;");
    expect(CSS).toContain(".search-empty { padding: 52px 8px; color: var(--text-secondary); background: transparent;");
    expect(CSS).toContain(".theme-dark .search-results { background: transparent; }");
    expect(CSS).toContain(".agents-heading { display: flex; align-items: end; justify-content: space-between; gap: 24px; padding: 40px 0 18px;");
    expect(CSS).toContain(".settings-heading { padding: 40px 0 18px; }");
    expect(CSS).toContain("grid-template-columns: 18px minmax(0, 1fr) auto auto");
    expect(CSS).toContain(".progress-state { color: var(--text-tertiary); font-size: var(--text-2xs); white-space: nowrap; }");
    expect(CSS).toContain(".topbar-actions .icon-button[aria-expanded=\"true\"] { color: var(--text-primary); background: transparent; }");
    expect(CSS).toContain(".user-message > div { max-width: 70%; padding: 10px 14px; border: 0; border-radius: 22px; corner-shape: round;");
    expect(CSS).toContain(".composer { width: min(39rem, 100%); border: 0; border-radius: var(--radius-composer);");
    expect(CSS).toContain("background: var(--surface-composer)");
    expect(CSS).toContain("padding-left: 10px; padding-right: 0;");
    expect(CSS).toContain("box-shadow: var(--shadow-composer)");
  });
});

function media(query: string): string {
  const start = CSS.indexOf(`@media (${query})`);
  if (start < 0) throw new Error("missing media contract");
  const next = CSS.indexOf("\n@media ", start + 1);
  return CSS.slice(start, next < 0 ? undefined : next);
}

function contrast(foreground: string, background: string): number {
  const values = [foreground, background].map((hex) => {
    const channels = hex.match(/../g)?.map((value) => Number.parseInt(value, 16) / 255);
    if (!channels || channels.length !== 3) throw new Error("invalid color");
    const [red, green, blue] = channels.map((value) => value <= 0.04045
      ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
  });
  return (Math.max(...values) + 0.05) / (Math.min(...values) + 0.05);
}
