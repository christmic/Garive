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

/** Keeps live output attached only while the reader remains near the tail. */
export function isNearConversationTail(
  metrics: ConversationScrollMetrics,
  threshold = 72,
): boolean {
  if (![metrics.scrollTop, metrics.scrollHeight, metrics.clientHeight, threshold]
    .every(Number.isFinite) || threshold < 0) return false;
  const remaining = Math.max(0,
    metrics.scrollHeight - metrics.clientHeight - Math.max(0, metrics.scrollTop));
  return remaining <= threshold;
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
