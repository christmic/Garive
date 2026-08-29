import { describe, expect, it } from "vitest";
import { runAgentTurn } from "./host";

describe("desktop Host IPC", () => {
  it("returns one typed embedded Runtime terminal", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const expected = { session_id: "session-1", turn_id: "turn-1", execution_id: "execution-1",
      cursor: 9, text: "durable answer", terminal: "completed" as const };
    const result = await runAgentTurn("definition-main", "hello", async <T>(
      command: string, args: Record<string, unknown>,
    ) => {
      calls.push({ command, args });
      return expected as T;
    });
    expect(calls).toEqual([{ command: "run_agent_turn", args: {
      definitionId: "definition-main", input: "hello",
    } }]);
    expect(result).toEqual(expected);
  });
});
