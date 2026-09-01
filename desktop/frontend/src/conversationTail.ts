export interface ConversationScrollMetrics {
  readonly scrollTop: number;
  readonly scrollHeight: number;
  readonly clientHeight: number;
}

export interface ConversationTailTarget {
  scrollTop: number;
  readonly scrollHeight: number;
  scrollTo?: (options: ScrollToOptions) => void;
}

export type ConversationScrollDirection = "away" | "toward";

/** Normalizes the reader's distance from the newest content for positive scroll coordinates. */
export function conversationDistanceFromTail(metrics: ConversationScrollMetrics): number {
  if (![metrics.scrollTop, metrics.scrollHeight, metrics.clientHeight].every(Number.isFinite)) {
    return Number.POSITIVE_INFINITY;
  }
  return Math.max(0,
    metrics.scrollHeight - metrics.clientHeight - Math.max(0, metrics.scrollTop));
}

/** Restores the same reading position after measured content changes height. */
export function preserveConversationDistanceFromTail(
  previous: ConversationScrollMetrics,
  nextScrollHeight: number,
  nextClientHeight = previous.clientHeight,
): number {
  const distance = conversationDistanceFromTail(previous);
  if (!Number.isFinite(distance) || !Number.isFinite(nextScrollHeight)
    || !Number.isFinite(nextClientHeight)) {
    return Math.max(0, previous.scrollTop);
  }
  return Math.max(0, nextScrollHeight - nextClientHeight - distance);
}

/** Maps only keys with platform scrolling semantics to an explicit reading direction. */
export function conversationScrollDirectionForKey(
  key: string,
  shiftKey = false,
): ConversationScrollDirection | undefined {
  switch (key) {
    case "ArrowUp": case "Home": case "PageUp": return "away";
    case "ArrowDown": case "End": case "PageDown": return "toward";
    case " ": case "Spacebar": return shiftKey ? "away" : "toward";
    default: return undefined;
  }
}

/** Keeps live output attached only while the reader remains near the tail. */
export function isNearConversationTail(
  metrics: ConversationScrollMetrics,
  threshold = 24,
): boolean {
  if (![metrics.scrollTop, metrics.scrollHeight, metrics.clientHeight, threshold]
    .every(Number.isFinite) || threshold < 0) return false;
  return conversationDistanceFromTail(metrics) <= threshold;
}

/** Returns smoothly when motion is allowed; reduced motion takes the exact tail immediately. */
export function scrollConversationToTail(
  target: ConversationTailTarget,
  reducedMotion: boolean,
): "smooth" | "instant" {
  if (!reducedMotion && typeof target.scrollTo === "function") {
    target.scrollTo({ top: target.scrollHeight, behavior: "smooth" });
    return "smooth";
  }
  target.scrollTop = target.scrollHeight;
  return "instant";
}
