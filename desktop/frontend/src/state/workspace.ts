import type { DesktopCapabilities, HostResult } from "../ipc/host";

export type BootState = "loading" | "ready" | "unavailable";
export type WorkPhase = "idle" | "submitting";
export type InspectorTab = "activity" | "artifacts";

export interface WorkMessage {
  readonly id: string;
  readonly role: "user" | "assistant";
  readonly text: string;
  readonly terminal?: HostResult["terminal"];
}

export interface WorkState {
  readonly boot: BootState;
  readonly capabilities?: DesktopCapabilities;
  readonly phase: WorkPhase;
  readonly sessionId?: string;
  readonly messages: readonly WorkMessage[];
  readonly draft: string;
  readonly error?: string;
  readonly inspectorOpen: boolean;
  readonly inspectorTab: InspectorTab;
}

export type WorkEvent =
  | { readonly type: "capabilities_loaded"; readonly capabilities: DesktopCapabilities }
  | { readonly type: "capabilities_failed" }
  | { readonly type: "draft_changed"; readonly value: string }
  | { readonly type: "submission_started" }
  | { readonly type: "submission_succeeded"; readonly input: string; readonly result: HostResult }
  | { readonly type: "submission_failed"; readonly code: string }
  | { readonly type: "new_work" }
  | { readonly type: "inspector_toggled" }
  | { readonly type: "inspector_selected"; readonly tab: InspectorTab }
  | { readonly type: "error_dismissed" };

export const initialWorkState: WorkState = {
  boot: "loading",
  phase: "idle",
  messages: [],
  draft: "",
  inspectorOpen: false,
  inspectorTab: "activity",
};

/** Pure Desktop work-state reducer; durable truth enters only through IPC results. */
export function reduceWork(state: WorkState, event: WorkEvent): WorkState {
  switch (event.type) {
    case "capabilities_loaded":
      return { ...state, boot: "ready", capabilities: event.capabilities };
    case "capabilities_failed":
      return { ...state, boot: "unavailable", error: "desktop_unavailable" };
    case "draft_changed":
      return { ...state, draft: event.value, error: undefined };
    case "submission_started":
      return { ...state, phase: "submitting", error: undefined };
    case "submission_succeeded": {
      const ordinal = state.messages.length;
      return {
        ...state,
        phase: "idle",
        sessionId: event.result.session_id,
        draft: "",
        error: undefined,
        messages: [
          ...state.messages,
          { id: `user-${ordinal}`, role: "user", text: event.input },
          {
            id: event.result.turn_id,
            role: "assistant",
            text: event.result.text,
            terminal: event.result.terminal,
          },
        ],
      };
    }
    case "submission_failed":
      return { ...state, phase: "idle", error: event.code };
    case "new_work":
      return {
        ...initialWorkState,
        boot: state.boot,
        capabilities: state.capabilities,
      };
    case "inspector_toggled":
      return { ...state, inspectorOpen: !state.inspectorOpen };
    case "inspector_selected":
      return { ...state, inspectorOpen: true, inspectorTab: event.tab };
    case "error_dismissed":
      return { ...state, error: undefined };
  }
}

export function canSubmit(state: WorkState): boolean {
  return state.boot === "ready"
    && state.capabilities?.configured === true
    && state.phase === "idle"
    && state.draft.trim().length > 0;
}
