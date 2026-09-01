import { describe, expect, it } from "vitest";
import { visibleScrollEdges } from "./scrollEdges";

describe("visibleScrollEdges", () => {
  it("discloses only edges that still contain scrollable content", () => {
    expect(visibleScrollEdges({ scrollTop: 0, clientHeight: 160, scrollHeight: 208 }))
      .toEqual({ top: false, bottom: true });
    expect(visibleScrollEdges({ scrollTop: 24, clientHeight: 160, scrollHeight: 208 }))
      .toEqual({ top: true, bottom: true });
    expect(visibleScrollEdges({ scrollTop: 48, clientHeight: 160, scrollHeight: 208 }))
      .toEqual({ top: true, bottom: false });
  });

  it("does not attenuate a list that fits or subpixel attachment noise", () => {
    expect(visibleScrollEdges({ scrollTop: 0, clientHeight: 208, scrollHeight: 208 }))
      .toEqual({ top: false, bottom: false });
    expect(visibleScrollEdges({ scrollTop: 1, clientHeight: 160, scrollHeight: 161 }))
      .toEqual({ top: false, bottom: false });
  });
});
