import { describe, expect, it, vi } from "vitest";
import { conversationScrollDirectionForKey, isNearConversationTail,
  preserveConversationDistanceFromTail, scrollConversationToTail } from "./conversationTail";

describe("conversation tail policy", () => {
  it("follows the tail within the bounded reading threshold", () => {
    expect(isNearConversationTail({ scrollTop: 576, scrollHeight: 1_000, clientHeight: 400 })).toBe(true);
    expect(isNearConversationTail({ scrollTop: 575, scrollHeight: 1_000, clientHeight: 400 })).toBe(false);
  });

  it("preserves distance from the tail across programmatic layout changes", () => {
    expect(preserveConversationDistanceFromTail({ scrollTop: 600,
      scrollHeight: 1_200, clientHeight: 400 }, 900)).toBe(300);
    expect(preserveConversationDistanceFromTail({ scrollTop: 800,
      scrollHeight: 1_200, clientHeight: 400 }, 900)).toBe(500);
    expect(preserveConversationDistanceFromTail({ scrollTop: 0,
      scrollHeight: 300, clientHeight: 400 }, 200)).toBe(0);
    expect(preserveConversationDistanceFromTail({ scrollTop: 800,
      scrollHeight: 1_200, clientHeight: 400 }, 1_200, 300)).toBe(900);
  });

  it("maps only platform scrolling keys to reader intent", () => {
    expect(conversationScrollDirectionForKey("ArrowUp")).toBe("away");
    expect(conversationScrollDirectionForKey("PageDown")).toBe("toward");
    expect(conversationScrollDirectionForKey(" ", true)).toBe("away");
    expect(conversationScrollDirectionForKey(" ", false)).toBe("toward");
    expect(conversationScrollDirectionForKey("Enter")).toBeUndefined();
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
