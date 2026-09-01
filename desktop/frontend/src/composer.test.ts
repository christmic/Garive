import { describe, expect, it } from "vitest";
import { resolveComposerLayout, shouldSubmitComposer } from "./composer";

describe("Composer keyboard submission", () => {
  it("submits only an unmodified Enter outside CJK composition", () => {
    expect(shouldSubmitComposer({ key: "Enter", shiftKey: false, isComposing: false })).toBe(true);
    expect(shouldSubmitComposer({ key: "Enter", shiftKey: true, isComposing: false })).toBe(false);
    expect(shouldSubmitComposer({ key: "Enter", shiftKey: false, isComposing: true })).toBe(false);
    expect(shouldSubmitComposer({ key: "Process", shiftKey: false, isComposing: true })).toBe(false);
  });
});

describe("progressive Composer layout", () => {
  it("uses the source-backed 32px fit guard for a quiet one-line draft", () => {
    expect(resolveComposerLayout({ text: "Draft a launch brief", measuredTextWidth: 132,
      availableInputWidth: 164, hasExpandedCapability: false })).toBe("single-line");
    expect(resolveComposerLayout({ text: "Draft a launch brief", measuredTextWidth: 133,
      availableInputWidth: 164, hasExpandedCapability: false })).toBe("multiline");
  });

  it("expands for semantic multi-line content or attached capability UI", () => {
    expect(resolveComposerLayout({ text: "First\nSecond", measuredTextWidth: 60,
      availableInputWidth: 400, hasExpandedCapability: false })).toBe("multiline");
    expect(resolveComposerLayout({ text: "Short", measuredTextWidth: 40,
      availableInputWidth: 400, hasExpandedCapability: true })).toBe("multiline");
  });

  it("starts compact before browser measurements are available", () => {
    expect(resolveComposerLayout({ text: "", hasExpandedCapability: false })).toBe("single-line");
  });
});
