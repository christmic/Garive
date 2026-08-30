import { describe, expect, it } from "vitest";
import {
  cancelSetup, commitSetup, decodeHostTimelinePage, getSetupCatalogue, getSetupState, prepareSetup,
  runAgentTurn,
} from "./host";

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

  it("preserves optional presence and unknown H2/H3 strings", () => {
    const raw = {
      api_version: "v1", session_id: "session-1", scanned_through_position: 9,
      observed_max_position: 9, has_more: false,
      items: [{
        turn_id: "turn-1", started_position: 1, latest_position: 9,
        state: "future_state", user_text: "hello", content_truncated: false,
        suspension: {
          suspension_id: "suspension-1", session_version: 3, kind: "future_kind",
          prompt_schema: "garive.prompt.v1", prompt_json: [123, 125], prompt_digest: "digest",
        },
        activities: [{
          api_version: "v1", activity_id: "activity-1", kind: "future_kind",
          label_key: "agent.activity.future", state: "future_state", source_position: 8,
          terminal: false,
        }],
      }],
    };
    const decoded = decodeHostTimelinePage(JSON.parse(JSON.stringify(raw)));
    expect(decoded.items[0]?.state).toBe("future_state");
    expect(decoded.items[0]?.activities[0]?.kind).toBe("future_kind");
    expect(decoded.items[0]?.completion_text).toBeUndefined();
    expect(decoded.items[0]?.activities[0]?.safe_code).toBeUndefined();
  });

  it("keeps setup credential in the write-only commit command", async () => {
    const calls: string[] = [];
    const invoke = async <T>(command: string, args: Record<string, unknown>) => {
      calls.push(command);
      if (command === "get_setup_catalogue") return { catalogue_revision: "catalogue-1" } as T;
      if (command === "prepare_setup") return { plan_digest: "plan-1" } as T;
      expect(args).toEqual({ planDigest: "plan-1", credential: "secret-once" });
      return { restart_required: true } as T;
    };
    await getSetupState(async <T>(command: string) => {
      calls.push(command);
      return { state: "not_configured" } as T;
    });
    await getSetupCatalogue(invoke);
    await prepareSetup({ schema_version: 1, caller_nonce: "nonce", catalogue_revision: "catalogue-1",
      preset_id: "preset", profile_id: "profile", model_target_id: "target", model_id: "model",
      deployment_id: "deployment", definition_id: "definition" }, invoke);
    expect((await commitSetup("plan-1", "secret-once", invoke)).restart_required).toBe(true);
    expect(calls).toEqual([
      "get_setup_state", "get_setup_catalogue", "prepare_setup", "commit_setup",
    ]);
  });

  it("cancels only one exact prepared setup plan", async () => {
    const result = await cancelSetup("plan-1", async <T>(
      command: string, args: Record<string, unknown>,
    ) => {
      expect({ command, args }).toEqual({ command: "cancel_setup", args: { planDigest: "plan-1" } });
      return "cancelled" as T;
    });
    expect(result).toBe("cancelled");
  });
});
