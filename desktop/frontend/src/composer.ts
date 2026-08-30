export interface ComposerKeyState {
  readonly key: string;
  readonly shiftKey: boolean;
  readonly isComposing: boolean;
}

/** Prevents Enter from submitting while an IME candidate is being composed. */
export function shouldSubmitComposer(event: ComposerKeyState): boolean {
  return event.key === "Enter" && !event.shiftKey && !event.isComposing;
}
