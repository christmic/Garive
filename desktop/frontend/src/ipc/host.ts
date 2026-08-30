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
  readonly multi_turn: boolean;
  readonly durable_navigation: boolean;
  readonly activity: boolean;
  readonly setup: boolean;
  readonly workspaces: boolean;
  readonly artifacts: boolean;
}

/** Loads the capability snapshot without exposing configuration values. */
export async function getDesktopCapabilities(
  invoke: Invoke = tauriInvoke,
): Promise<DesktopCapabilities> {
  return invoke<DesktopCapabilities>("get_desktop_capabilities", {});
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
