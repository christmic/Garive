import { describe, expect, it } from "vitest";
import {
  getDesktopCapabilities, getRecentSessions, getSessionTimeline, runAgentTurn,
  attachWorkspaceToSession, cancelSetup, chooseWorkspace, commitSetup, continueAgentTurn,
  createWorkSession, getSessionWorkspaces, getSetupCatalogue, prepareSetup, revokeWorkspace,
  verifyWorkspace,
} from "./host";

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

  it("continues one exact durable text suspension", async () => {
    const result = await continueAgentTurn("session-1", "turn-1", {
      suspension_id: "suspension-1", session_version: 3, kind: "partial_output",
    }, "continue", async <T>(command: string, args: Record<string, unknown>) => {
      expect({ command, args }).toEqual({ command: "continue_agent_turn", args: {
        sessionId: "session-1", turnId: "turn-1", suspensionId: "suspension-1",
        sessionVersion: 3, input: "continue",
      } });
      return { terminal: "completed" } as T;
    });
    expect(result.terminal).toBe("completed");
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

  it("loads durable navigation through bounded typed commands", async () => {
    const invoke = async <T>(command: string, args: Record<string, unknown>) => {
      if (command === "get_recent_sessions") {
        expect(args).toEqual({ limit: 12 });
        return [{ session_id: "session-1" }] as T;
      }
      expect({ command, args }).toEqual({ command: "get_session_timeline", args: {
        sessionId: "session-1", afterPosition: 0, limit: 32,
      } });
      return { api_version: "v1", session_id: "session-1", items: [] } as T;
    };
    expect(await getRecentSessions(12, invoke)).toEqual([{ session_id: "session-1" }]);
    expect((await getSessionTimeline("session-1", 0, 32, invoke)).session_id).toBe("session-1");
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
    await getSetupCatalogue(invoke);
    await prepareSetup({ schema_version: 1, caller_nonce: "nonce", catalogue_revision: "catalogue-1",
      profile_id: "profile", model_target_id: "target", model_id: "model",
      deployment_id: "deployment", definition_id: "definition" }, invoke);
    expect((await commitSetup("plan-1", "secret-once", invoke)).restart_required).toBe(true);
    expect(calls).toEqual(["get_setup_catalogue", "prepare_setup", "commit_setup"]);
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

  it("keeps native Workspace paths behind opaque commands", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const invoke = async <T>(command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      if (command === "choose_workspace" || command === "verify_workspace") {
        return { schema_version: 1, workspace_id: "workspace-1", display_name: "Briefs",
          access: "enumerate", grant_revision: 1, state: "active",
          expires_at: "2026-08-30T12:00:00Z" } as T;
      }
      return undefined as T;
    };
    const selected = await chooseWorkspace(invoke);
    await verifyWorkspace(selected!.workspace_id, invoke);
    await revokeWorkspace(selected!.workspace_id, invoke);
    expect(JSON.stringify(calls)).not.toContain("/Users/");
    expect(calls).toEqual([
      { command: "choose_workspace", args: {} },
      { command: "verify_workspace", args: { workspaceId: "workspace-1" } },
      { command: "revoke_workspace", args: { workspaceId: "workspace-1" } },
    ]);
  });

  it("durably attaches Workspace context before a Turn", async () => {
    const calls: string[] = [];
    const invoke = async <T>(command: string) => {
      calls.push(command);
      if (command === "create_work_session") return "session-1" as T;
      return { session_id: "session-1", workspace_id: "workspace-1", attached_position: 2 } as T;
    };
    const session = await createWorkSession("definition-main", invoke);
    const attachment = await attachWorkspaceToSession(session, "workspace-1", invoke);
    await getSessionWorkspaces(session, invoke);
    expect(attachment.attached_position).toBe(2);
    expect(calls).toEqual([
      "create_work_session", "attach_workspace_to_session", "get_session_workspaces",
    ]);
  });
});
