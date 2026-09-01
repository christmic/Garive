export interface ComposerKeyState {
  readonly key: string;
  readonly shiftKey: boolean;
  readonly isComposing: boolean;
}

export type ComposerLayout = "single-line" | "multiline";

export interface ComposerLayoutInput {
  readonly text: string;
  readonly hasExpandedCapability: boolean;
  readonly measuredTextWidth?: number;
  readonly availableInputWidth?: number;
}

/** Codex keeps this much spare input width before admitting its compact row. */
export const COMPOSER_SINGLE_LINE_GUARD = 32;

export function resolveComposerLayout(input: ComposerLayoutInput): ComposerLayout {
  if (input.hasExpandedCapability || input.text.includes("\n")) return "multiline";
  if (input.measuredTextWidth === undefined || input.availableInputWidth === undefined) {
    return "single-line";
  }
  return input.measuredTextWidth + COMPOSER_SINGLE_LINE_GUARD <= input.availableInputWidth
    ? "single-line" : "multiline";
}

/** Prevents Enter from submitting while an IME candidate is being composed. */
export function shouldSubmitComposer(event: ComposerKeyState): boolean {
  return event.key === "Enter" && !event.shiftKey && !event.isComposing;
}
