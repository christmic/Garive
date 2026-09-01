import { describe, expect, it } from "vitest";
import {
  clampSidebarWidth, DEFAULT_DESKTOP_PREFERENCES, readDesktopPreferences,
  sourceDefaultConversationSplit, writeDesktopPreferences,
} from "./preferences";

describe("Desktop appearance preferences", () => {
  it("round-trips only the admitted bounded values", () => {
    let encoded: string | null = null;
    const storage = { getItem: () => encoded, setItem: (_: string, value: string) => {
      encoded = value;
    } };
    writeDesktopPreferences({
      schema_version: 5, theme: "dark", density: "compact", locale: "zh-Hans",
      workspaceSplitPx: 416, sidebarWidthPx: 304,
    }, storage);
    expect(readDesktopPreferences(storage)).toEqual({
      schema_version: 5, theme: "dark", density: "compact", locale: "zh-Hans",
      workspaceSplitPx: 416, sidebarWidthPx: 304,
    });
    expect(encoded).not.toContain("path");
    writeDesktopPreferences(DEFAULT_DESKTOP_PREFERENCES, storage);
    expect(readDesktopPreferences(storage)).toEqual(DEFAULT_DESKTOP_PREFERENCES);
  });

  it("fails closed on unknown, malformed or oversized storage", () => {
    const read = (value: string) => readDesktopPreferences({ getItem: () => value });
    expect(read("not-json")).toEqual(DEFAULT_DESKTOP_PREFERENCES);
    expect(read('{"schema_version":1,"theme":"neon","density":"compact"}'))
      .toEqual(DEFAULT_DESKTOP_PREFERENCES);
    expect(read('{"schema_version":1,"theme":"dark","density":"compact","path":"x"}'))
      .toEqual(DEFAULT_DESKTOP_PREFERENCES);
    expect(read('{"schema_version":2,"theme":"dark","density":"compact","locale":"fr"}'))
      .toEqual(DEFAULT_DESKTOP_PREFERENCES);
    expect(read('{"schema_version":3,"theme":"dark","density":"compact","locale":"en","workspaceSplitPx":9000}'))
      .toEqual(DEFAULT_DESKTOP_PREFERENCES);
    expect(read("x".repeat(257))).toEqual(DEFAULT_DESKTOP_PREFERENCES);
  });

  it("migrates the exact v1 appearance record without widening it", () => {
    expect(readDesktopPreferences({ getItem: () =>
      '{"schema_version":1,"theme":"light","density":"compact"}',
    })).toEqual({
      schema_version: 5, theme: "light", density: "compact", locale: "system",
      workspaceSplitPx: "adaptive", sidebarWidthPx: 275,
    });
  });

  it("migrates the exact v2 record to the reference workbench split", () => {
    expect(readDesktopPreferences({ getItem: () =>
      '{"schema_version":2,"theme":"dark","density":"comfortable","locale":"en"}',
    })).toEqual({
      schema_version: 5, theme: "dark", density: "comfortable", locale: "en",
      workspaceSplitPx: "adaptive", sidebarWidthPx: 275,
    });
  });

  it("migrates v3 and bounds the source-backed navigation width", () => {
    expect(readDesktopPreferences({ getItem: () =>
      '{"schema_version":3,"theme":"dark","density":"comfortable","locale":"en","workspaceSplitPx":416}',
    })).toEqual({
      schema_version: 5, theme: "dark", density: "comfortable", locale: "en",
      workspaceSplitPx: 416, sidebarWidthPx: 275,
    });
    expect(clampSidebarWidth(120)).toBe(240);
    expect(clampSidebarWidth(337.6)).toBe(338);
    expect(clampSidebarWidth(900)).toBe(520);
    expect(readDesktopPreferences({ getItem: () =>
      '{"schema_version":4,"theme":"dark","density":"compact","locale":"en","workspaceSplitPx":448,"sidebarWidthPx":288}',
    })).toEqual({ schema_version: 5, theme: "dark", density: "compact", locale: "en",
      workspaceSplitPx: 448, sidebarWidthPx: 288 });
  });

  it("uses Codex's adaptive content-pane default instead of its minimum width", () => {
    expect(sourceDefaultConversationSplit(1_005, 800)).toBe(365);
    expect(sourceDefaultConversationSplit(1_325, 1_000)).toBe(500);
    expect(sourceDefaultConversationSplit(749, 768)).toBe(352);
  });
});
