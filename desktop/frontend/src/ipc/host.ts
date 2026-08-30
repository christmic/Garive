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

/** Installed immutable Agent definition exposed by H2. */
export interface HostDefinition {
  readonly api_version: "v1"; readonly definition_id: string;
  readonly definition_revision: string; readonly capabilities: readonly string[];
}
/** Bounded installed-Agent discovery page. */
export interface HostDefinitionPage {
  readonly api_version: "v1"; readonly definitions: readonly HostDefinition[];
}
/** Restart-safe H2 Session navigation summary. */
export interface HostSessionSummary {
  readonly api_version: "v1"; readonly session_id: string; readonly agent_instance_id: string;
  readonly definition_id: string; readonly definition_revision: string; readonly opened_at: string;
  readonly latest_position: number; readonly latest_turn_id?: string;
  readonly latest_turn_state?: "running" | "suspended" | "completed" | "stopped" | "failed";
  readonly turn_count: number;
}
/** Reverse-opened H2 Session page. */
export interface HostSessionPage {
  readonly api_version: "v1"; readonly sessions: readonly HostSessionSummary[]; readonly next_before?: string;
}

/** One public H1/H3 event; unknown event names remain strings. */
export interface HostEvent {
  readonly api_version: "v1"; readonly session_id: string; readonly position: number;
  readonly event: string; readonly turn_id: string; readonly execution_id: string;
  readonly text: string; readonly activity?: HostActivity;
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
  readonly prompt_schema?: string; readonly prompt_json?: readonly number[];
  readonly prompt_digest?: string; readonly response_schema_json?: readonly number[];
  readonly response_schema_digest?: string;
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

/** Maps untrusted IPC JSON without collapsing optional presence. */
export function decodeHostTimelinePage(raw: unknown): HostTimelinePage {
  const value = object(raw); return {
    api_version: apiVersion(value.api_version), session_id: text(value.session_id),
    items: array(value.items).map((item) => timelineItem(object(item))),
    scanned_through_position: position(value.scanned_through_position),
    observed_max_position: position(value.observed_max_position), has_more: boolean(value.has_more),
  };
}

/** Strictly maps an untrusted H2 definition page. */
export function decodeHostDefinitionPage(raw: unknown): HostDefinitionPage {
  const value = object(raw); return { api_version: apiVersion(value.api_version),
    definitions: array(value.definitions).map((rawDefinition) => {
      const definition = object(rawDefinition); return { api_version: apiVersion(definition.api_version),
        definition_id: text(definition.definition_id), definition_revision: text(definition.definition_revision),
        capabilities: array(definition.capabilities).map(text) };
    }) };
}

/** Strictly maps an untrusted H2 Session page. */
export function decodeHostSessionPage(raw: unknown): HostSessionPage {
  const value = object(raw); return { api_version: apiVersion(value.api_version),
    sessions: array(value.sessions).map((rawSession) => {
      const session = object(rawSession); return { api_version: apiVersion(session.api_version),
        session_id: text(session.session_id), agent_instance_id: text(session.agent_instance_id),
        definition_id: text(session.definition_id), definition_revision: text(session.definition_revision),
        opened_at: text(session.opened_at), latest_position: position(session.latest_position),
        latest_turn_id: optionalText(session.latest_turn_id), latest_turn_state: optionalTurnState(session.latest_turn_state),
        turn_count: position(session.turn_count) };
    }), next_before: optionalText(value.next_before) };
}

/** Strictly maps one untrusted H1/H3 event. */
export function decodeHostEvent(raw: unknown): HostEvent {
  const value = object(raw); return { api_version: apiVersion(value.api_version), session_id: text(value.session_id),
    position: position(value.position), event: text(value.event), turn_id: optionalText(value.turn_id) ?? "",
    execution_id: optionalText(value.execution_id) ?? "", text: optionalText(value.text) ?? "",
    activity: value.activity === undefined ? undefined : activity(object(value.activity)) };
}

function timelineItem(value: Record<string, unknown>): HostTimelineItem {
  return {
    turn_id: text(value.turn_id), started_position: position(value.started_position),
    latest_position: position(value.latest_position), state: turnState(value.state),
    user_text: text(value.user_text), completion_text: optionalText(value.completion_text),
    suspension: value.suspension === undefined ? undefined : suspension(object(value.suspension)),
    content_truncated: boolean(value.content_truncated),
    activities: array(value.activities).map((item) => activity(object(item))),
  };
}
function activity(value: Record<string, unknown>): HostActivity {
  return { api_version: apiVersion(value.api_version), activity_id: text(value.activity_id),
    kind: text(value.kind), label_key: text(value.label_key), state: text(value.state),
    source_position: position(value.source_position), terminal: boolean(value.terminal),
    safe_code: optionalText(value.safe_code) };
}
function suspension(value: Record<string, unknown>): HostSuspension {
  return { suspension_id: text(value.suspension_id), session_version: position(value.session_version),
    kind: suspensionKind(value.kind), prompt_schema: text(value.prompt_schema),
    prompt_json: bytes(value.prompt_json), prompt_digest: text(value.prompt_digest),
    response_schema_json: value.response_schema_json === undefined ? undefined : bytes(value.response_schema_json),
    response_schema_digest: optionalText(value.response_schema_digest) };
}
function object(value: unknown): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error("invalid_host_value");
  return value as Record<string, unknown>;
}
function array(value: unknown): readonly unknown[] {
  if (!Array.isArray(value)) throw new Error("invalid_host_value"); return value;
}
function text(value: unknown): string {
  if (typeof value !== "string" || value.length === 0) throw new Error("invalid_host_value"); return value;
}
function optionalText(value: unknown): string | undefined { return value === undefined ? undefined : text(value); }
function position(value: unknown): number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) throw new Error("invalid_host_value");
  return value as number;
}
function boolean(value: unknown): boolean {
  if (typeof value !== "boolean") throw new Error("invalid_host_value"); return value;
}
function bytes(value: unknown): readonly number[] {
  const output = array(value);
  if (!output.every((item) => Number.isInteger(item) && Number(item) >= 0 && Number(item) <= 255)) {
    throw new Error("invalid_host_value");
  }
  return output as readonly number[];
}
function apiVersion(value: unknown): "v1" {
  if (value !== "v1") throw new Error("invalid_host_value"); return "v1";
}
function turnState(value: unknown): NonNullable<HostSessionSummary["latest_turn_state"]> {
  const parsed = text(value);
  if (!["running", "suspended", "completed", "stopped", "failed"].includes(parsed)) throw new Error("invalid_host_value");
  return parsed as NonNullable<HostSessionSummary["latest_turn_state"]>;
}
function optionalTurnState(value: unknown): HostSessionSummary["latest_turn_state"] {
  return value === undefined ? undefined : turnState(value);
}
function suspensionKind(value: unknown): HostSuspension["kind"] {
  const parsed = text(value);
  if (!["approval_required", "external_input_required", "operator_reconciliation", "resource_unavailable",
    "partial_output", "delegation_pending"].includes(parsed)) throw new Error("invalid_host_value");
  return parsed as HostSuspension["kind"];
}

export interface SetupProfile {
  readonly profile_id: string; readonly display_name_key: string;
  readonly endpoint_mode: "fixed" | "optional_override";
  readonly model_mode: "exact_id"; readonly credential_label_key: string;
  readonly supported_capabilities: readonly string[];
}

export interface SetupPreset {
  readonly preset_id: string; readonly display_name_key: string;
  readonly supported_profile_ids: readonly string[];
}

export interface SetupLimits {
  readonly max_profiles: number; readonly max_text_bytes: number;
  readonly max_endpoint_bytes: number; readonly max_secret_bytes: number;
  readonly max_plan_count: number; readonly plan_lifetime_seconds: number;
}

export interface SetupCatalogue {
  readonly schema_version: 1; readonly catalogue_revision: string;
  readonly profiles: readonly SetupProfile[]; readonly presets: readonly SetupPreset[];
  readonly limits: SetupLimits;
}

export interface SetupInput {
  readonly schema_version: 1; readonly caller_nonce: string; readonly catalogue_revision: string;
  readonly preset_id: string; readonly profile_id: string; readonly endpoint_override?: string;
  readonly model_target_id: string; readonly model_id: string;
  readonly deployment_id: string; readonly definition_id: string;
}

export interface SetupPlan {
  readonly schema_version: 1; readonly setup_id: string; readonly caller_nonce: string;
  readonly catalogue_revision: string; readonly effective_configuration_digest: string;
  readonly expected_configuration_revision?: number;
  readonly expected_configuration_digest?: string;
  readonly expires_at: string;
  readonly summary: Omit<SetupInput, "schema_version" | "caller_nonce" | "catalogue_revision"> & {
    readonly endpoint_mode: "fixed" | "override";
  };
  readonly plan_digest: string;
}

export type SetupState =
  | { readonly state: "not_configured" }
  | { readonly state: "configured"; readonly restart_required: boolean }
  | { readonly state: "invalid_configuration"; readonly code: string }
  | { readonly state: "setup_recovering" };

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

export interface WorkspaceRevocationReceipt {
  readonly schema_version: 1; readonly workspace_id: string; readonly grant_revision: number;
  readonly outcome: "revoked" | "already_revoked"; readonly cleanup_pending: boolean;
}

export interface WorkspaceAttachment {
  readonly api_version: "v1"; readonly session_id: string; readonly workspace_id: string;
  readonly display_name: string; readonly grant_revision: number;
  readonly access: "enumerate" | "read_write";
  readonly attached_position: number;
}

export interface WorkspaceDetachment {
  readonly api_version: "v1"; readonly session_id: string; readonly workspace_id: string;
  readonly grant_revision: number; readonly outcome: "detached" | "already_detached";
  readonly detached_position: number;
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

/** Restores a bounded complete timeline by following strictly advancing Host pages. */
export async function getCompleteSessionTimeline(
  sessionId: string,
  invoke: Invoke = tauriInvoke,
): Promise<HostTimelinePage> {
  const items: HostTimelinePage["items"][number][] = [];
  let afterPosition = 0;
  let observedMaxPosition: number | undefined;
  let latest: HostTimelinePage | undefined;
  for (let pageNumber = 0; pageNumber < 8; pageNumber += 1) {
    const page = await getSessionTimeline(sessionId, afterPosition, 64, invoke);
    observedMaxPosition ??= page.observed_max_position;
    if (page.session_id !== sessionId || page.scanned_through_position < afterPosition
        || page.scanned_through_position > page.observed_max_position
        || page.observed_max_position !== observedMaxPosition) {
      throw new Error("projection_failure");
    }
    items.push(...page.items);
    if (items.length > 512) throw new Error("projection_failure");
    latest = page;
    if (!page.has_more) return { ...page, items };
    if (page.scanned_through_position === afterPosition) throw new Error("projection_failure");
    afterPosition = page.scanned_through_position;
  }
  throw new Error(latest?.has_more ? "projection_failure" : "host_failure");
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

/** Restores every Artifact in one bounded fixed-prefix sequence. */
export async function listAllArtifacts(
  sessionId: string,
  invoke: Invoke = tauriInvoke,
): Promise<HostArtifactPage> {
  const items: HostArtifact[] = [];
  let afterPosition = 0;
  let observedMaxPosition: number | undefined;
  let latest: HostArtifactPage | undefined;
  for (let pageNumber = 0; pageNumber < 4; pageNumber += 1) {
    const page = await listArtifacts(sessionId, afterPosition, 64, invoke);
    observedMaxPosition ??= page.observed_max_position;
    if (page.session_id !== sessionId || page.scanned_through_position < afterPosition
        || page.scanned_through_position > page.observed_max_position
        || page.observed_max_position !== observedMaxPosition) {
      throw new Error("projection_failure");
    }
    items.push(...page.items);
    if (items.length > 256) throw new Error("projection_failure");
    latest = page;
    if (!page.has_more) return { ...page, items };
    if (page.scanned_through_position === afterPosition) throw new Error("projection_failure");
    afterPosition = page.scanned_through_position;
  }
  throw new Error(latest?.has_more ? "projection_failure" : "host_failure");
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
  if (!exportTargetId) throw new Error("artifact_export_stale");
  return invoke<ArtifactExportReceipt>("commit_artifact_export", {
    request: { ...artifactCommand(sessionId, artifact), exportTargetId },
  });
}

export async function getSetupCatalogue(invoke: Invoke = tauriInvoke): Promise<SetupCatalogue> {
  return invoke<SetupCatalogue>("get_setup_catalogue", {});
}

export async function getSetupState(invoke: Invoke = tauriInvoke): Promise<SetupState> {
  return invoke<SetupState>("get_setup_state", {});
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
  expectedGrantRevision: number,
  invoke: Invoke = tauriInvoke,
): Promise<WorkspaceRevocationReceipt> {
  if (!workspaceId || !Number.isSafeInteger(expectedGrantRevision)
      || expectedGrantRevision < 1) throw new Error("workspace_capability_invalid");
  return invoke<WorkspaceRevocationReceipt>("revoke_workspace", {
    workspaceId, expectedGrantRevision,
  });
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

export async function detachWorkspaceFromSession(
  sessionId: string,
  workspaceId: string,
  grantRevision: number,
  invoke: Invoke = tauriInvoke,
): Promise<WorkspaceDetachment> {
  if (!sessionId || !workspaceId || !Number.isSafeInteger(grantRevision) || grantRevision < 1) {
    throw new Error("workspace_capability_invalid");
  }
  return invoke<WorkspaceDetachment>("detach_workspace_from_session", {
    sessionId, workspaceId, grantRevision,
  });
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
