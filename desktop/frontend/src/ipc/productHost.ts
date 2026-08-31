import { Channel, invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { LiveOutputItem } from "../state/controller";
import { decodeLiveOutput } from "../state/liveOutput";
import {
  decodeHostDefinitionPage, decodeHostEvent, decodeHostSessionPage, decodeHostTimelinePage,
  type HostDefinitionPage, type HostEvent, type HostSessionPage, type HostTimelinePage, type Invoke,
} from "./host";

export interface CreateSessionReceipt {
  readonly session_id: string; readonly agent_instance_id: string; readonly committed_position: number;
}

export interface TurnCommandReceipt {
  readonly session_id: string; readonly turn_id: string; readonly execution_id: string;
  readonly committed_position: number;
}

export interface HostEventPage {
  readonly events: readonly HostEvent[]; readonly scanned_through_position: number;
  readonly observed_max_position: number;
}

export async function getProductDefinitions(invoke: Invoke = tauriInvoke): Promise<HostDefinitionPage> {
  return decodeHostDefinitionPage(await invoke<unknown>("get_agent_definitions", {}));
}

export async function getProductSessions(invoke: Invoke = tauriInvoke): Promise<HostSessionPage> {
  return decodeHostSessionPage(await invoke<unknown>("get_product_sessions", { limit: 64, before: null }));
}

export async function getProductTimeline(
  sessionId: string, afterPosition = 0, invoke: Invoke = tauriInvoke,
): Promise<HostTimelinePage> {
  required(sessionId);
  return decodeHostTimelinePage(await invoke<unknown>("get_product_timeline", {
    sessionId, afterPosition: safePosition(afterPosition), limit: 64,
  }));
}

export async function getProductEvents(
  sessionId: string, afterPosition: number, invoke: Invoke = tauriInvoke,
): Promise<HostEventPage> {
  required(sessionId);
  const raw = object(await invoke<unknown>("get_session_events", {
    sessionId, afterPosition: safePosition(afterPosition),
  }));
  const scanned = safePosition(raw.scanned_through_position);
  const observed = safePosition(raw.observed_max_position);
  if (!Array.isArray(raw.events) || scanned < afterPosition || observed < scanned) invalid();
  return { events: raw.events.map(decodeHostEvent), scanned_through_position: scanned,
    observed_max_position: observed };
}

export async function followProductLiveOutput(
  sessionId: string, onOutput: (output: LiveOutputItem) => void, invoke: Invoke = tauriInvoke,
): Promise<void> {
  required(sessionId);
  const channel = new Channel<unknown>();
  channel.onmessage = (raw) => onOutput(decodeLiveOutput(raw, sessionId));
  await invoke<void>("follow_live_output", { sessionId, onEvent: channel });
}

export async function createProductSession(
  commandId: string, definitionId: string, invoke: Invoke = tauriInvoke,
): Promise<CreateSessionReceipt> {
  required(commandId); required(definitionId);
  return createReceipt(await invoke<unknown>("create_product_session", { commandId, definitionId }));
}

export async function startProductTurn(
  commandId: string, sessionId: string, input: string, invoke: Invoke = tauriInvoke,
): Promise<TurnCommandReceipt> {
  required(commandId); required(sessionId); required(input);
  return turnReceipt(await invoke<unknown>("start_product_turn", { commandId, sessionId, input }), sessionId);
}

export async function startProductTurnWithWorkspaceContext(
  commandId: string, sessionId: string, input: string, workspaceId: string,
  entryIds: readonly string[], invoke: Invoke = tauriInvoke,
): Promise<TurnCommandReceipt> {
  [commandId, sessionId, input, workspaceId].forEach(required);
  if (!entryIds.length || entryIds.some((entryId) => !entryId)) invalid();
  return turnReceipt(await invoke<unknown>("start_product_turn_with_workspace_context", { request: {
    commandId, sessionId, input, workspaceId, entryIds,
  } }), sessionId);
}

export async function cancelProductTurn(
  commandId: string, sessionId: string, turnId: string, requestedThroughPosition: number,
  invoke: Invoke = tauriInvoke,
): Promise<TurnCommandReceipt> {
  required(commandId); required(sessionId); required(turnId);
  return turnReceipt(await invoke<unknown>("cancel_product_turn", { commandId, sessionId, turnId,
    requestedThroughPosition: safePositivePosition(requestedThroughPosition) }), sessionId, turnId);
}

export async function continueProductTurn(
  commandId: string, sessionId: string, turnId: string, suspensionId: string,
  sessionVersion: number, input: string, invoke: Invoke = tauriInvoke,
): Promise<TurnCommandReceipt> {
  [commandId, sessionId, turnId, suspensionId, input].forEach(required);
  return turnReceipt(await invoke<unknown>("continue_product_turn", { commandId, sessionId, turnId,
    suspensionId, sessionVersion: safePositivePosition(sessionVersion), input }), sessionId, turnId);
}

export async function continueProductApproval(
  commandId: string, sessionId: string, turnId: string, suspensionId: string,
  sessionVersion: number, approved: boolean, invoke: Invoke = tauriInvoke,
): Promise<TurnCommandReceipt> {
  [commandId, sessionId, turnId, suspensionId].forEach(required);
  return turnReceipt(await invoke<unknown>("continue_product_approval", { commandId, sessionId, turnId,
    suspensionId, sessionVersion: safePositivePosition(sessionVersion), approved }), sessionId, turnId);
}

function createReceipt(raw: unknown): CreateSessionReceipt {
  const value = object(raw); return { session_id: required(value.session_id),
    agent_instance_id: required(value.agent_instance_id),
    committed_position: safePositivePosition(value.committed_position) };
}

function turnReceipt(raw: unknown, sessionId: string, turnId?: string): TurnCommandReceipt {
  const value = object(raw); const receipt = { session_id: required(value.session_id),
    turn_id: required(value.turn_id), execution_id: required(value.execution_id),
    committed_position: safePositivePosition(value.committed_position) };
  if (receipt.session_id !== sessionId || (turnId !== undefined && receipt.turn_id !== turnId)) invalid();
  return receipt;
}

function object(value: unknown): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) invalid();
  return value as Record<string, unknown>;
}
function required(value: unknown): string {
  if (typeof value !== "string" || value.length === 0) invalid(); return value;
}
function safePosition(value: unknown): number {
  if (!Number.isSafeInteger(value) || Number(value) < 0) invalid(); return Number(value);
}
function safePositivePosition(value: unknown): number {
  const position = safePosition(value); if (position === 0) invalid(); return position;
}
function invalid(): never { throw new Error("invalid_product_host_value"); }
