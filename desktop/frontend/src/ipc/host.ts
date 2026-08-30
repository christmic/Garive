import { invoke as tauriInvoke } from "@tauri-apps/api/core";

/** Typed Tauri command invocation boundary, injectable for integration tests. */
export type Invoke = <T>(command: string, args: Record<string, unknown>) => Promise<T>;
/** Durable embedded-Runtime terminal returned by the backend. */
export interface HostResult {
  readonly session_id: string; readonly turn_id: string; readonly execution_id: string;
  readonly cursor: number; readonly text: string;
  readonly terminal: "completed" | "suspended" | "stopped" | "failed";
}

/** Provider-neutral public activity; unknown kind/state strings remain intact. */
export interface HostActivity {
  readonly api_version: string; readonly activity_id: string; readonly kind: string;
  readonly label_key: string; readonly state: string; readonly source_position: number;
  readonly terminal: boolean; readonly safe_code?: string;
}

/** Restart-safe public continuation coordinates and canonical prompt bytes. */
export interface HostSuspension {
  readonly suspension_id: string; readonly session_version: number; readonly kind: string;
  readonly prompt_schema: string; readonly prompt_json: readonly number[];
  readonly prompt_digest: string; readonly response_schema_json?: readonly number[];
  readonly response_schema_digest?: string;
}

/** One complete durable Turn restored from a fixed Host Ledger prefix. */
export interface HostTimelineItem {
  readonly turn_id: string; readonly started_position: number; readonly latest_position: number;
  readonly state: string; readonly user_text: string; readonly completion_text?: string;
  readonly suspension?: HostSuspension; readonly content_truncated: boolean;
  readonly activities: readonly HostActivity[];
}

/** Bounded durable conversation page. */
export interface HostTimelinePage {
  readonly api_version: string; readonly session_id: string;
  readonly items: readonly HostTimelineItem[]; readonly scanned_through_position: number;
  readonly observed_max_position: number; readonly has_more: boolean;
}

/** Maps untrusted IPC JSON without collapsing optional presence or future strings. */
export function decodeHostTimelinePage(raw: unknown): HostTimelinePage {
  const value = object(raw); const items = array(value.items).map((item) => timelineItem(object(item)));
  return {
    api_version: text(value.api_version), session_id: text(value.session_id), items,
    scanned_through_position: position(value.scanned_through_position),
    observed_max_position: position(value.observed_max_position), has_more: boolean(value.has_more),
  };
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

function timelineItem(value: Record<string, unknown>): HostTimelineItem {
  return {
    turn_id: text(value.turn_id), started_position: position(value.started_position),
    latest_position: position(value.latest_position), state: text(value.state),
    user_text: text(value.user_text), completion_text: optionalText(value.completion_text),
    suspension: value.suspension === undefined ? undefined : suspension(object(value.suspension)),
    content_truncated: boolean(value.content_truncated),
    activities: array(value.activities).map((item) => activity(object(item))),
  };
}

function activity(value: Record<string, unknown>): HostActivity {
  return {
    api_version: text(value.api_version), activity_id: text(value.activity_id),
    kind: text(value.kind), label_key: text(value.label_key), state: text(value.state),
    source_position: position(value.source_position), terminal: boolean(value.terminal),
    safe_code: optionalText(value.safe_code),
  };
}

function suspension(value: Record<string, unknown>): HostSuspension {
  return {
    suspension_id: text(value.suspension_id), session_version: position(value.session_version),
    kind: text(value.kind), prompt_schema: text(value.prompt_schema),
    prompt_json: bytes(value.prompt_json), prompt_digest: text(value.prompt_digest),
    response_schema_json: value.response_schema_json === undefined
      ? undefined : bytes(value.response_schema_json),
    response_schema_digest: optionalText(value.response_schema_digest),
  };
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
function optionalText(value: unknown): string | undefined {
  return value === undefined ? undefined : text(value);
}
function position(value: unknown): number {
  if (!Number.isSafeInteger(value) || (value as number) < 0) throw new Error("invalid_host_value");
  return value as number;
}
function boolean(value: unknown): boolean {
  if (typeof value !== "boolean") throw new Error("invalid_host_value"); return value;
}
function bytes(value: unknown): readonly number[] {
  const output = array(value); if (!output.every((item) => Number.isInteger(item) && Number(item) >= 0 && Number(item) <= 255)) throw new Error("invalid_host_value");
  return output as readonly number[];
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

/** Invokes one typed Turn against the backend-owned embedded R1 composition. */
export async function runAgentTurn(
  definitionId: string, input: string, invoke: Invoke = tauriInvoke,
): Promise<HostResult> {
  if (!definitionId || !input) throw new Error("invalid_command");
  return invoke<HostResult>("run_agent_turn", { definitionId, input });
}
