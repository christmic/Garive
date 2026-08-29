import { invoke as tauriInvoke } from "@tauri-apps/api/core";

/** Typed Tauri command invocation boundary, injectable for integration tests. */
export type Invoke = <T>(command: string, args: Record<string, unknown>) => Promise<T>;
/** Verified terminal returned by the current fixture-backed desktop command. */
export interface HostResult { readonly text: string; readonly terminal: "completed"; }

/** Invokes the fixture-backed Host command and returns its typed terminal. */
export async function runFakeHost(input: string, invoke: Invoke = tauriInvoke): Promise<HostResult> {
  const text = await invoke<string>("run_fake_host", { input });
  return { text, terminal: "completed" };
}
