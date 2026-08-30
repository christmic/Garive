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
  readonly user_text: string; readonly completion_text?: string; readonly content_truncated: boolean;
}

/** Bounded durable conversation page. */
export interface HostTimelinePage {
  readonly api_version: "v1"; readonly session_id: string; readonly items: readonly HostTimelineItem[];
  readonly scanned_through_position: number; readonly observed_max_position: number;
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
