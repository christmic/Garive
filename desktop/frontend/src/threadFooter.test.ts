import { describe, expect, it } from "vitest";
import { threadScrollPaddingBottom } from "./threadFooter";

describe("thread footer layout", () => {
  it("adds the source safe gap to each live footer height", () => {
    expect(threadScrollPaddingBottom(80)).toBe(96);
    expect(threadScrollPaddingBottom(234)).toBe(250);
  });

  it("never admits a negative scroll reserve", () => {
    expect(threadScrollPaddingBottom(-40)).toBe(16);
  });
});
