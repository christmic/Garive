import { invoke as tauriInvoke } from "@tauri-apps/api/core";

export type Invoke = <T>(command: string, args: Record<string, unknown>) => Promise<T>;
export interface HostResult { readonly text: string; readonly terminal: "completed"; }

export async function runFakeHost(input: string, invoke: Invoke = tauriInvoke): Promise<HostResult> {
  const text = await invoke<string>("run_fake_host", { input });
  return { text, terminal: "completed" };
}
