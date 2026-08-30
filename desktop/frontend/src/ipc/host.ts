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

/** One immutable Artifact fact projected by the embedded Runtime. */
export interface HostArtifact {
  readonly api_version: "v1"; readonly artifact_id: string; readonly revision: number;
  readonly session_id: string; readonly turn_id: string; readonly display_name: string;
  readonly kind: string; readonly mime_type: string; readonly byte_size: number;
  readonly content_digest: string; readonly committed_position: number;
  readonly verification: string; readonly preview: "text" | "unavailable" | string;
  readonly workspace_id?: string; readonly revealable: boolean; readonly exportable: boolean;
}

/** Bounded immutable Artifact projection page. */
export interface HostArtifactPage {
  readonly api_version: "v1"; readonly session_id: string; readonly items: readonly HostArtifact[];
  readonly scanned_through_position: number; readonly observed_max_position: number;
  readonly has_more: boolean;
}

/** Digest-verified content returned only for one exact Artifact revision. */
export interface ArtifactPreview {
  readonly schema_version: 1; readonly artifact_id: string; readonly revision: number;
  readonly kind: "text"; readonly content_utf8: string; readonly truncated: boolean;
}

/** One path-free, expiring native save-panel destination capability. */
export interface ArtifactExportTarget {
  readonly schema_version: 1; readonly export_target_id: string; readonly display_name: string;
  readonly state: "ready"; readonly expires_at: string;
}

/** Terminal receipt for an exact digest-verified Artifact export. */
export interface ArtifactExportReceipt {
  readonly schema_version: 1; readonly artifact_id: string; readonly revision: number;
  readonly display_name: string; readonly byte_size: number; readonly content_digest: string;
  readonly state: "exported";
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
  readonly access: "enumerate" | "read_write"; readonly grant_revision: number;
  readonly state: "active"; readonly expires_at: string;
}

/** Aggregate path-free health of durable macOS Workspace authorization. */
export interface WorkspaceRecoveryStatus {
  readonly schema_version: 1;
  readonly state: "ready" | "attention_required" | "index_unavailable";
  readonly restored_count: number;
  readonly needs_reauthorization_count: number;
}

export interface WorkspaceAuthorization {
  readonly schema_version: 1;
  readonly workspace_id: string;
  readonly display_name: string;
  readonly grant_revision: number;
  readonly state: "active" | "needs_reauthorization";
}

export interface WorkspaceAttachment {
  readonly api_version: "v1"; readonly session_id: string; readonly workspace_id: string;
  readonly display_name: string; readonly grant_revision: number;
  readonly access: "enumerate" | "read_write";
  readonly attached_position: number;
}

export interface WorkspaceEntry {
  readonly schema_version: 1; readonly entry_id: string;
  readonly parent_entry_id: string | null;
  readonly display_name: string;
  readonly kind: "directory" | "text" | "image" | "pdf" | "table" | "presentation" | "unknown";
  readonly byte_size: number | null; readonly selectable: boolean;
}

export interface WorkspaceEntryPage {
  readonly schema_version: 1; readonly workspace_id: string;
  readonly parent_entry_id: string | null;
  readonly entries: readonly WorkspaceEntry[]; readonly next_cursor: string | null;
  readonly has_more: boolean;
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

/** Restores immutable Artifacts without exposing backing filesystem paths. */
export async function listArtifacts(
  sessionId: string,
  afterPosition = 0,
  limit = 64,
  invoke: Invoke = tauriInvoke,
): Promise<HostArtifactPage> {
  if (!sessionId || afterPosition < 0 || limit < 1 || limit > 64) {
    throw new Error("invalid_request");
  }
  return invoke<HostArtifactPage>("list_artifacts", { sessionId, afterPosition, limit });
}

/** Requests a bounded preview using only backend-verified Artifact coordinates. */
export async function getArtifactPreview(
  sessionId: string,
  artifact: Pick<HostArtifact, "artifact_id" | "revision" | "committed_position">,
  invoke: Invoke = tauriInvoke,
): Promise<ArtifactPreview> {
  if (!sessionId || !artifact.artifact_id || artifact.revision < 1
      || artifact.committed_position < 1) throw new Error("artifact_not_found");
  return invoke<ArtifactPreview>("get_artifact_preview", {
    sessionId, artifactId: artifact.artifact_id, revision: artifact.revision,
    committedPosition: artifact.committed_position,
  });
}

function artifactCommand(sessionId: string, artifact: HostArtifact) {
  if (!sessionId || !artifact.artifact_id || artifact.revision < 1
      || artifact.committed_position < 1) throw new Error("artifact_not_found");
  return { sessionId, artifactId: artifact.artifact_id, revision: artifact.revision,
    committedPosition: artifact.committed_position };
}

/** Opens the native save panel and returns no destination path to React. */
export async function prepareArtifactExport(
  sessionId: string,
  artifact: HostArtifact,
  invoke: Invoke = tauriInvoke,
): Promise<ArtifactExportTarget | null> {
  return invoke<ArtifactExportTarget | null>("prepare_artifact_export", {
    request: artifactCommand(sessionId, artifact),
  });
}

/** Consumes one exact native destination capability to create a new local copy. */
export async function commitArtifactExport(
  sessionId: string,
  artifact: HostArtifact,
  exportTargetId: string,
  invoke: Invoke = tauriInvoke,
): Promise<ArtifactExportReceipt> {
  if (!exportTargetId) throw new Error("artifact_export_invalid");
  return invoke<ArtifactExportReceipt>("commit_artifact_export", {
    request: { ...artifactCommand(sessionId, artifact), exportTargetId },
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

export async function getWorkspaceRecoveryStatus(
  invoke: Invoke = tauriInvoke,
): Promise<WorkspaceRecoveryStatus> {
  return invoke<WorkspaceRecoveryStatus>("get_workspace_recovery_status", {});
}

export async function listWorkspaceAuthorizations(
  invoke: Invoke = tauriInvoke,
): Promise<readonly WorkspaceAuthorization[]> {
  return invoke<WorkspaceAuthorization[]>("list_workspace_authorizations", {});
}

export async function reauthorizeWorkspace(
  workspaceId: string,
  invoke: Invoke = tauriInvoke,
): Promise<WorkspaceGrant | null> {
  if (!workspaceId) throw new Error("workspace_capability_invalid");
  return invoke<WorkspaceGrant | null>("reauthorize_workspace", { workspaceId });
}

export async function authorizeWorkspaceWrites(
  workspaceId: string,
  invoke: Invoke = tauriInvoke,
): Promise<WorkspaceGrant | null> {
  if (!workspaceId) throw new Error("workspace_capability_invalid");
  return invoke<WorkspaceGrant | null>("authorize_workspace_writes", { workspaceId });
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

export async function listWorkspaceEntries(
  workspaceId: string,
  parentEntryId?: string,
  cursor?: string,
  limit = 32,
  invoke: Invoke = tauriInvoke,
): Promise<WorkspaceEntryPage> {
  if (!workspaceId || limit < 1 || limit > 64) throw new Error("workspace_capability_invalid");
  return invoke<WorkspaceEntryPage>("list_workspace_entries", {
    workspaceId, parentEntryId: parentEntryId ?? null, cursor: cursor ?? null, limit,
  });
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

export async function runAgentTurnWithWorkspaceContext(
  definitionId: string,
  sessionId: string,
  input: string,
  workspaceId: string,
  entryIds: readonly string[],
  invoke: Invoke = tauriInvoke,
): Promise<HostResult> {
  if (!definitionId || !sessionId || !input || !workspaceId
      || entryIds.length < 1 || entryIds.length > 8 || new Set(entryIds).size !== entryIds.length) {
    throw new Error("workspace_capability_invalid");
  }
  return invoke<HostResult>("run_agent_turn_with_workspace_context", {
    request: { definitionId, sessionId, input, workspaceId, entryIds },
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

/** Resolves one exact durable approval without overloading text continuation. */
export async function resolveTurnApproval(
  sessionId: string,
  turnId: string,
  suspension: HostSuspension,
  approved: boolean,
  invoke: Invoke = tauriInvoke,
): Promise<HostResult> {
  if (!sessionId || !turnId || !suspension.suspension_id
      || suspension.kind !== "approval_required") {
    throw new Error("invalid_command");
  }
  return invoke<HostResult>("resolve_turn_approval", {
    sessionId,
    turnId,
    suspensionId: suspension.suspension_id,
    sessionVersion: suspension.session_version,
    approved,
  });
}
