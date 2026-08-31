import { describe, expect, it } from "vitest";
import { isNearConversationTail } from "./conversationTail";

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
});
