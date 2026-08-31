export interface ConversationScrollMetrics {
  readonly scrollTop: number;
  readonly scrollHeight: number;
  readonly clientHeight: number;
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
