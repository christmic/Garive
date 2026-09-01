import { describe, expect, it, vi } from "vitest";
import { isNearConversationTail, scrollConversationToTail } from "./conversationTail";

describe("conversation tail policy", () => {
  it("follows the tail within the bounded reading threshold", () => {
    expect(isNearConversationTail({ scrollTop: 528, scrollHeight: 1_000, clientHeight: 400 })).toBe(true);
    expect(isNearConversationTail({ scrollTop: 527, scrollHeight: 1_000, clientHeight: 400 })).toBe(false);
  });

  it("treats short content as attached and rejects invalid measurements", () => {
    expect(isNearConversationTail({ scrollTop: 0, scrollHeight: 300, clientHeight: 500 })).toBe(true);
    expect(isNearConversationTail({ scrollTop: Number.NaN, scrollHeight: 300, clientHeight: 500 })).toBe(false);
    expect(isNearConversationTail({ scrollTop: 0, scrollHeight: 300, clientHeight: 500 }, -1)).toBe(false);
  });

  it("preserves a real smooth return and honors reduced motion", () => {
    const scrollTo = vi.fn();
    const target = { scrollTop: 120, scrollHeight: 1_000, scrollTo };
    expect(scrollConversationToTail(target, false)).toBe("smooth");
    expect(scrollTo).toHaveBeenCalledWith({ top: 1_000, behavior: "smooth" });
    expect(target.scrollTop).toBe(120);
    expect(scrollConversationToTail(target, true)).toBe("instant");
    expect(target.scrollTop).toBe(1_000);
  });
});
