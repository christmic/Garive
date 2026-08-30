import { describe, expect, it } from "vitest";
import {
  DEFAULT_DESKTOP_PREFERENCES, readDesktopPreferences, writeDesktopPreferences,
} from "./preferences";

describe("Desktop appearance preferences", () => {
  it("round-trips only the admitted bounded values", () => {
    let encoded: string | null = null;
    const storage = { getItem: () => encoded, setItem: (_: string, value: string) => {
      encoded = value;
    } };
    writeDesktopPreferences({ schema_version: 1, theme: "dark", density: "compact" }, storage);
    expect(readDesktopPreferences(storage)).toEqual({
      schema_version: 1, theme: "dark", density: "compact",
    });
    expect(encoded).not.toContain("path");
  });

  it("fails closed on unknown, malformed or oversized storage", () => {
    const read = (value: string) => readDesktopPreferences({ getItem: () => value });
    expect(read("not-json")).toEqual(DEFAULT_DESKTOP_PREFERENCES);
    expect(read('{"schema_version":1,"theme":"neon","density":"compact"}'))
      .toEqual(DEFAULT_DESKTOP_PREFERENCES);
    expect(read('{"schema_version":1,"theme":"dark","density":"compact","path":"x"}'))
      .toEqual(DEFAULT_DESKTOP_PREFERENCES);
    expect(read("x".repeat(257))).toEqual(DEFAULT_DESKTOP_PREFERENCES);
  });
});
