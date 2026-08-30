import type { DesktopCapabilities, HostActivity, HostArtifact, HostArtifactPage, HostResult, HostSuspension, HostTimelinePage, WorkspaceAttachment } from "../ipc/host";
import type { AppViewState } from "./controller";

export type BootState = "loading" | "ready" | "unavailable";
export type WorkPhase = "idle" | "submitting";
export type InspectorTab = "activity" | "artifacts";

export interface WorkMessage {
  readonly id: string;
  readonly role: "user" | "assistant";
  readonly text: string;
  readonly terminal?: HostResult["terminal"];
  readonly suspension?: HostSuspension;
}

export interface WorkState {
  readonly boot: BootState;
  readonly capabilities?: DesktopCapabilities;
  readonly phase: WorkPhase;
  readonly execution: AppViewState["execution"];
  readonly sessionId?: string;
  readonly messages: readonly WorkMessage[];
  readonly activities: readonly HostActivity[];
  readonly artifacts: readonly HostArtifact[];
  readonly workspaces: readonly WorkspaceAttachment[];
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
  | { readonly type: "session_loaded"; readonly timeline: HostTimelinePage }
  | { readonly type: "product_projected"; readonly view: AppViewState }
  | { readonly type: "artifacts_loaded"; readonly page: HostArtifactPage }
  | { readonly type: "workspaces_loaded"; readonly sessionId: string;
    readonly workspaces: readonly WorkspaceAttachment[] }
  | { readonly type: "new_work" }
  | { readonly type: "inspector_toggled" }
  | { readonly type: "inspector_selected"; readonly tab: InspectorTab }
  | { readonly type: "error_dismissed" };

export const initialWorkState: WorkState = {
  boot: "loading",
  phase: "idle",
  execution: "idle",
  messages: [],
  activities: [],
  artifacts: [],
  workspaces: [],
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
      return { ...state, phase: "submitting", execution: "submitting", error: undefined };
    case "submission_succeeded": {
      const ordinal = state.messages.length;
      return {
        ...state,
        phase: "idle",
        execution: "idle",
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
      return { ...state, phase: "idle", execution: "idle", error: event.code };
    case "session_loaded":
      return {
        ...state,
        phase: "idle",
        execution: "idle",
        sessionId: event.timeline.session_id,
        messages: timelineMessages(event.timeline),
        activities: event.timeline.items.flatMap((item) => item.activities),
        artifacts: [],
        workspaces: [],
        draft: "",
        error: undefined,
      };
    case "product_projected":
      return projectProduct(state, event.view);
    case "artifacts_loaded":
      return event.page.session_id === state.sessionId
        ? { ...state, artifacts: event.page.items }
        : state;
    case "workspaces_loaded":
      return event.sessionId === state.sessionId
        ? { ...state, workspaces: event.workspaces }
        : state;
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

function projectProduct(state: WorkState, view: AppViewState): WorkState {
  const sessionId = view.selectedSessionId;
  const sameSession = sessionId !== undefined && sessionId === state.sessionId;
  const draft = sessionId
    ? view.drafts.find((item) => item.sessionId === sessionId)?.text ?? ""
    : state.draft;
  return { ...state,
    boot: view.shell === "booting" || view.shell === "loading_navigation" ? "loading"
      : view.shell === "unavailable" ? "unavailable" : "ready",
    phase: ["submitting", "following", "cancelling", "reconnecting", "continuing"]
      .includes(view.execution) ? "submitting" : "idle",
    execution: view.execution,
    sessionId, draft, messages: productMessages(view),
    activities: view.activities.map((activity) => ({ api_version: "v1",
      activity_id: activity.activityId, kind: activity.kind, label_key: activity.labelKey ?? "agent.activity.updated",
      state: activity.state, source_position: activity.position, terminal: activity.terminal ?? false,
      safe_code: activity.safeCode })),
    artifacts: sameSession ? state.artifacts : [], workspaces: sameSession ? state.workspaces : [],
    error: view.notice?.code };
}

function productMessages(view: AppViewState): readonly WorkMessage[] {
  return view.timeline.flatMap((item) => {
    const user: WorkMessage = { id: `user-${item.turnId}`, role: "user", text: item.userText ?? "" };
    if (item.state === "running") return [user];
    const suspension = item.suspension && { suspension_id: item.suspension.suspensionId,
      session_version: item.suspension.sessionVersion, kind: item.suspension.kind,
      prompt_digest: item.suspension.promptDigest,
      response_schema_digest: item.suspension.responseSchemaDigest };
    return [user, { id: item.turnId, role: "assistant", text: item.completionText ?? "",
      terminal: item.state as HostResult["terminal"], suspension } satisfies WorkMessage];
  });
}

function timelineMessages(timeline: HostTimelinePage): readonly WorkMessage[] {
  return timeline.items.flatMap((item) => {
    const user: WorkMessage = {
      id: `user-${item.turn_id}`,
      role: "user",
      text: item.user_text,
    };
    if (item.state === "running") return [user];
    return [user, {
      id: item.turn_id,
      role: "assistant",
      text: item.completion_text ?? "",
      terminal: item.state,
      suspension: item.suspension,
    } satisfies WorkMessage];
  });
}

export function canSubmit(state: WorkState): boolean {
  const suspension = [...state.messages].reverse().find((message) => message.suspension)?.suspension;
  const resumable = !suspension || suspension.kind === "partial_output"
    || suspension.kind === "external_input_required";
  return state.boot === "ready"
    && state.capabilities?.configured === true
    && state.phase === "idle"
    && resumable
    && state.draft.trim().length > 0;
}
