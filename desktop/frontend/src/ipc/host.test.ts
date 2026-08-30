import { describe, expect, it } from "vitest";
import { getDesktopCapabilities, runAgentTurn } from "./host";

describe("desktop Host IPC", () => {
  it("returns one typed embedded Runtime terminal", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const expected = { session_id: "session-1", turn_id: "turn-1", execution_id: "execution-1",
      cursor: 9, text: "durable answer", terminal: "completed" as const };
    const result = await runAgentTurn("definition-main", "hello", "session-0", async <T>(
      command: string, args: Record<string, unknown>,
    ) => {
      calls.push({ command, args });
      return expected as T;
    });
    expect(calls).toEqual([{ command: "run_agent_turn", args: {
      definitionId: "definition-main", sessionId: "session-0", input: "hello",
    } }]);
    expect(result).toEqual(expected);
  });

  it("loads a truthful capability snapshot", async () => {
    const expected = {
      configured: true, agent_definition_id: "definition-main", multi_turn: true, durable_navigation: false,
      activity: false, setup: false, workspaces: false, artifacts: false,
    };
    const result = await getDesktopCapabilities(async <T>(command: string, args: Record<string, unknown>) => {
      expect({ command, args }).toEqual({ command: "get_desktop_capabilities", args: {} });
      return expected as T;
    });
    expect(result).toEqual(expected);
  });
});
