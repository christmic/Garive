import { invoke as tauriInvoke } from "@tauri-apps/api/core";

/** Typed Tauri command invocation boundary, injectable for integration tests. */
export type Invoke = <T>(command: string, args: Record<string, unknown>) => Promise<T>;
/** Durable embedded-Runtime terminal returned by the backend. */
export interface HostResult {
  readonly session_id: string; readonly turn_id: string; readonly execution_id: string;
  readonly cursor: number; readonly text: string;
  readonly terminal: "completed" | "suspended" | "stopped" | "failed";
}

/** Backend-proved Desktop capability availability; false values remain gated. */
export interface DesktopCapabilities {
  readonly configured: boolean;
  readonly agent_definition_id?: string;
  readonly multi_turn: boolean;
  readonly durable_navigation: boolean;
  readonly activity: boolean;
  readonly setup: boolean;
  readonly workspaces: boolean;
  readonly artifacts: boolean;
}

/** Restart-safe durable Session navigation summary. */
export interface HostSessionSummary {
  readonly api_version: "v1"; readonly session_id: string; readonly agent_instance_id: string;
  readonly definition_id: string; readonly definition_revision: string; readonly opened_at: string;
  readonly latest_position: number; readonly latest_turn_id?: string;
  readonly latest_turn_state?: "running" | "suspended" | "completed" | "stopped" | "failed";
  readonly turn_count: number;
}

/** One complete durable Turn restored from the Runtime. */
export interface HostTimelineItem {
  readonly turn_id: string; readonly started_position: number; readonly latest_position: number;
  readonly state: "running" | "suspended" | "completed" | "stopped" | "failed";
  readonly user_text: string; readonly completion_text?: string;
  readonly suspension?: HostSuspension; readonly content_truncated: boolean;
  readonly activities: readonly HostActivity[];
}

export interface HostActivity {
  readonly api_version: "v1"; readonly activity_id: string;
  readonly kind: "tool" | "interaction" | string; readonly label_key: string;
  readonly state: string; readonly source_position: number; readonly terminal: boolean;
  readonly safe_code?: string;
}

export interface HostSuspension {
  readonly suspension_id: string; readonly session_version: number;
  readonly kind: "approval_required" | "external_input_required" | "operator_reconciliation"
    | "resource_unavailable" | "partial_output" | "delegation_pending";
}

/** Bounded durable conversation page. */
export interface HostTimelinePage {
  readonly api_version: "v1"; readonly session_id: string; readonly items: readonly HostTimelineItem[];
  readonly scanned_through_position: number; readonly observed_max_position: number;
  readonly has_more: boolean;
}

export interface SetupProfile {
  readonly profile_id: string; readonly display_name_key: string;
  readonly endpoint_mode: "fixed" | "optional_override";
  readonly supported_capabilities: readonly string[];
}

export interface SetupCatalogue {
  readonly schema_version: 1; readonly catalogue_revision: string;
  readonly profiles: readonly SetupProfile[]; readonly max_text_bytes: number;
  readonly max_endpoint_bytes: number; readonly max_secret_bytes: number;
}

export interface SetupInput {
  readonly schema_version: 1; readonly caller_nonce: string; readonly catalogue_revision: string;
  readonly profile_id: string; readonly endpoint_override?: string;
  readonly model_target_id: string; readonly model_id: string;
  readonly deployment_id: string; readonly definition_id: string;
}

export interface SetupPlan {
  readonly schema_version: 1; readonly setup_id: string; readonly caller_nonce: string;
  readonly catalogue_revision: string; readonly effective_configuration_digest: string;
  readonly expires_at: string;
  readonly summary: Omit<SetupInput, "schema_version" | "caller_nonce" | "catalogue_revision"> & {
    readonly endpoint_mode: "fixed" | "override";
  };
  readonly plan_digest: string;
}

export interface SetupReceipt {
  readonly schema_version: 1; readonly setup_id: string; readonly plan_digest: string;
  readonly configuration_revision: number; readonly configuration_digest: string;
  readonly restart_required: true; readonly receipt_digest: string;
}

/** Opaque process-local Workspace selection; no filesystem path crosses IPC. */
export interface WorkspaceGrant {
  readonly schema_version: 1; readonly workspace_id: string; readonly display_name: string;
  readonly access: "enumerate"; readonly grant_revision: number;
  readonly state: "active"; readonly expires_at: string;
}

export interface WorkspaceAttachment {
  readonly api_version: "v1"; readonly session_id: string; readonly workspace_id: string;
  readonly display_name: string; readonly grant_revision: number; readonly access: "enumerate";
  readonly attached_position: number;
}

/** Loads the capability snapshot without exposing configuration values. */
export async function getDesktopCapabilities(
  invoke: Invoke = tauriInvoke,
): Promise<DesktopCapabilities> {
  return invoke<DesktopCapabilities>("get_desktop_capabilities", {});
}

/** Loads recent durable Sessions from the embedded Runtime. */
export async function getRecentSessions(
  limit = 20,
  invoke: Invoke = tauriInvoke,
): Promise<readonly HostSessionSummary[]> {
  return invoke<HostSessionSummary[]>("get_recent_sessions", { limit });
}

/** Restores a durable conversation without reading raw Runtime facts. */
export async function getSessionTimeline(
  sessionId: string,
  afterPosition = 0,
  limit = 64,
  invoke: Invoke = tauriInvoke,
): Promise<HostTimelinePage> {
  if (!sessionId) throw new Error("invalid_request");
  return invoke<HostTimelinePage>("get_session_timeline", {
    sessionId, afterPosition, limit,
  });
}

export async function getSetupCatalogue(invoke: Invoke = tauriInvoke): Promise<SetupCatalogue> {
  return invoke<SetupCatalogue>("get_setup_catalogue", {});
}

export async function prepareSetup(
  input: SetupInput,
  invoke: Invoke = tauriInvoke,
): Promise<SetupPlan> {
  return invoke<SetupPlan>("prepare_setup", { input });
}

export async function commitSetup(
  planDigest: string,
  credential: string,
  invoke: Invoke = tauriInvoke,
): Promise<SetupReceipt> {
  if (!planDigest || !credential) throw new Error("setup_credential_rejected");
  return invoke<SetupReceipt>("commit_setup", { planDigest, credential });
}

export async function cancelSetup(
  planDigest: string,
  invoke: Invoke = tauriInvoke,
): Promise<"cancelled" | "already_committed"> {
  if (!planDigest) throw new Error("setup_plan_stale");
  return invoke<"cancelled" | "already_committed">("cancel_setup", { planDigest });
}

export async function restartDesktop(invoke: Invoke = tauriInvoke): Promise<void> {
  await invoke<void>("restart_desktop", {});
}

export async function chooseWorkspace(invoke: Invoke = tauriInvoke): Promise<WorkspaceGrant | null> {
  return invoke<WorkspaceGrant | null>("choose_workspace", {});
}

export async function verifyWorkspace(
  workspaceId: string,
  invoke: Invoke = tauriInvoke,
): Promise<WorkspaceGrant> {
  if (!workspaceId) throw new Error("workspace_capability_invalid");
  return invoke<WorkspaceGrant>("verify_workspace", { workspaceId });
}

export async function revokeWorkspace(
  workspaceId: string,
  invoke: Invoke = tauriInvoke,
): Promise<void> {
  if (!workspaceId) throw new Error("workspace_capability_invalid");
  await invoke<void>("revoke_workspace", { workspaceId });
}

export async function createWorkSession(
  definitionId: string,
  invoke: Invoke = tauriInvoke,
): Promise<string> {
  if (!definitionId) throw new Error("invalid_command");
  return invoke<string>("create_work_session", { definitionId });
}

export async function attachWorkspaceToSession(
  sessionId: string,
  workspaceId: string,
  invoke: Invoke = tauriInvoke,
): Promise<WorkspaceAttachment> {
  if (!sessionId || !workspaceId) throw new Error("workspace_capability_invalid");
  return invoke<WorkspaceAttachment>("attach_workspace_to_session", { sessionId, workspaceId });
}

export async function getSessionWorkspaces(
  sessionId: string,
  invoke: Invoke = tauriInvoke,
): Promise<readonly WorkspaceAttachment[]> {
  if (!sessionId) throw new Error("invalid_request");
  return invoke<WorkspaceAttachment[]>("get_session_workspaces", { sessionId });
}

/** Invokes one typed Turn against the backend-owned embedded R1 composition. */
export async function runAgentTurn(
  definitionId: string,
  input: string,
  sessionId?: string,
  invoke: Invoke = tauriInvoke,
): Promise<HostResult> {
  if (!definitionId || !input) throw new Error("invalid_command");
  return invoke<HostResult>("run_agent_turn", {
    definitionId,
    sessionId: sessionId ?? null,
    input,
  });
}

/** Continues one exact restart-safe text suspension. */
export async function continueAgentTurn(
  sessionId: string,
  turnId: string,
  suspension: HostSuspension,
  input: string,
  invoke: Invoke = tauriInvoke,
): Promise<HostResult> {
  if (!sessionId || !turnId || !suspension.suspension_id || !input) {
    throw new Error("invalid_command");
  }
  return invoke<HostResult>("continue_agent_turn", {
    sessionId,
    turnId,
    suspensionId: suspension.suspension_id,
    sessionVersion: suspension.session_version,
    input,
  });
}
