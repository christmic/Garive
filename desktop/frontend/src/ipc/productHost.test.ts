import { describe, expect, it } from "vitest";
import {
  cancelProductTurn, createProductSession, getProductDefinitions, getProductEvents,
  getProductSessions, getProductTimeline, startProductTurn, continueProductApproval,
} from "./productHost";

describe("product Host IPC", () => {
  it("uses only bounded H2 product reads", async () => {
    const calls: string[] = [];
    const invoke = async <T>(command: string) => {
      calls.push(command);
      if (command === "get_agent_definitions") return { api_version: "v1", definitions: [] } as T;
      if (command === "get_product_sessions") return { api_version: "v1", sessions: [] } as T;
      return { api_version: "v1", session_id: "session-1", items: [],
        scanned_through_position: 0, observed_max_position: 0, has_more: false } as T;
    };
    expect((await getProductDefinitions(invoke)).definitions).toEqual([]);
    expect((await getProductSessions(invoke)).sessions).toEqual([]);
    expect((await getProductTimeline("session-1", 0, invoke)).items).toEqual([]);
    expect(calls).toEqual(["get_agent_definitions", "get_product_sessions", "get_product_timeline"]);
  });

  it("binds every mutation to caller-owned exact coordinates", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const invoke = async <T>(command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      if (command === "create_product_session") return { session_id: "session-1",
        agent_instance_id: "agent-1", committed_position: 1 } as T;
      return { session_id: "session-1", turn_id: "turn-1",
        execution_id: "execution-1", committed_position: 7 } as T;
    };
    await createProductSession("command-create", "definition-1", invoke);
    await startProductTurn("command-start", "session-1", "hello", invoke);
    await cancelProductTurn("command-cancel", "session-1", "turn-1", 6, invoke);
    await continueProductApproval("command-approval", "session-1", "turn-1", "suspension-1", 7, true, invoke);
    expect(calls).toEqual([
      { command: "create_product_session", args: { commandId: "command-create", definitionId: "definition-1" } },
      { command: "start_product_turn", args: { commandId: "command-start", sessionId: "session-1", input: "hello" } },
      { command: "cancel_product_turn", args: { commandId: "command-cancel", sessionId: "session-1",
        turnId: "turn-1", requestedThroughPosition: 6 } },
      { command: "continue_product_approval", args: { commandId: "command-approval", sessionId: "session-1",
        turnId: "turn-1", suspensionId: "suspension-1", sessionVersion: 7, approved: true } },
    ]);
  });

  it("rejects reordered coordinates and malformed event watermarks", async () => {
    await expect(startProductTurn("command", "session-1", "hello", async <T>() => ({
      session_id: "session-other", turn_id: "turn-1", execution_id: "execution-1", committed_position: 2,
    } as T))).rejects.toThrow("invalid_product_host_value");
    await expect(getProductEvents("session-1", 5, async <T>() => ({
      events: [], scanned_through_position: 4, observed_max_position: 5,
    } as T))).rejects.toThrow("invalid_product_host_value");
  });
});
