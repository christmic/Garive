/** Codex ThreadScrollLayout keeps one 16px safe gap above its measured footer. */
export const THREAD_FOOTER_SAFE_GAP_PX = 16;

export function threadScrollPaddingBottom(footerHeight: number): number {
  return Math.max(0, footerHeight) + THREAD_FOOTER_SAFE_GAP_PX;
}
