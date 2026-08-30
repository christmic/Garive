import { describe, expect, it } from "vitest";
import { shouldSubmitComposer } from "./composer";

describe("Composer keyboard submission", () => {
  it("submits only an unmodified Enter outside CJK composition", () => {
    expect(shouldSubmitComposer({ key: "Enter", shiftKey: false, isComposing: false })).toBe(true);
    expect(shouldSubmitComposer({ key: "Enter", shiftKey: true, isComposing: false })).toBe(false);
    expect(shouldSubmitComposer({ key: "Enter", shiftKey: false, isComposing: true })).toBe(false);
    expect(shouldSubmitComposer({ key: "Process", shiftKey: false, isComposing: true })).toBe(false);
  });
});
