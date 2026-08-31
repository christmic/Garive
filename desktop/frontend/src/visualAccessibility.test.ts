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
    expect(narrow).toContain(".approval-actions { grid-column: 1 / -1;");
  });

  it("keeps normal semantic text tokens at WCAG AA contrast in light and dark", () => {
    expect(contrast("5f5f5f", "ffffff")).toBeGreaterThanOrEqual(4.5);
    expect(contrast("707070", "ffffff")).toBeGreaterThanOrEqual(4.5);
    expect(contrast("b4b4b4", "212121")).toBeGreaterThanOrEqual(4.5);
    expect(contrast("a0a0a0", "212121")).toBeGreaterThanOrEqual(4.5);
  });

  it("freezes the shared Codex-fidelity scale and wide shell geometry", () => {
    for (const token of ["--text-2xs: 11px", "--text-xs: 12px", "--text-sm: 13px",
      "--surface-canvas: #ffffff", "--surface-sidebar: #f7f7f7",
      "--surface-canvas: #212121", "--surface-sidebar: #181818"]) {
      expect(TOKENS).toContain(token);
    }
    expect(CSS).toContain("grid-template-columns: clamp(240px, 19vw, 275px) minmax(0, 1fr)");
    expect(CSS).toContain(".app-shell:has(.workspace-panel)");
    expect(CSS).toContain("minmax(390px, .82fr) minmax(500px, 1.18fr)");
    expect(CSS).toContain("grid-template-rows: 46px minmax(0, 1fr)");
    expect(CSS).toContain(".environment-panel { position: absolute");
    expect(CSS).toContain("width: min(40rem, calc(100% - 48px))");
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
