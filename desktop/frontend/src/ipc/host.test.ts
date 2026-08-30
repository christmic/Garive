import { describe, expect, it } from "vitest";
import {
  getDesktopCapabilities, getRecentSessions, getSessionTimeline, runAgentTurn,
  attachWorkspaceToSession, cancelSetup, chooseWorkspace, commitSetup, continueAgentTurn,
  createWorkSession, getSessionWorkspaces, getSetupCatalogue, listWorkspaceEntries, prepareSetup,
  authorizeWorkspaceWrites, getWorkspaceRecoveryStatus, listWorkspaceAuthorizations,
  commitArtifactExport, getArtifactPreview, listArtifacts, prepareArtifactExport,
  reauthorizeWorkspace, resolveTurnApproval, revokeWorkspace,
  runAgentTurnWithWorkspaceContext, verifyWorkspace,
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

  it("projects and previews one exact immutable Artifact without paths", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const invoke = async <T>(command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      if (command === "list_artifacts") return { api_version: "v1", session_id: "session-1",
        items: [{ artifact_id: "artifact-1", revision: 2, committed_position: 17 }],
        scanned_through_position: 17, observed_max_position: 17, has_more: false } as T;
      return { schema_version: 1, artifact_id: "artifact-1", revision: 2,
        kind: "text", content_utf8: "verified", truncated: false } as T;
    };
    const page = await listArtifacts("session-1", 7, 12, invoke);
    const preview = await getArtifactPreview("session-1", page.items[0], invoke);
    expect(preview.content_utf8).toBe("verified");
    expect(calls).toEqual([
      { command: "list_artifacts", args: { sessionId: "session-1", afterPosition: 7, limit: 12 } },
      { command: "get_artifact_preview", args: { sessionId: "session-1",
        artifactId: "artifact-1", revision: 2, committedPosition: 17 } },
    ]);
    expect(JSON.stringify(calls)).not.toContain("/Users/");
  });

  it("exports through one path-free native destination capability", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const artifact = { api_version: "v1" as const, artifact_id: "artifact-1", revision: 2,
      session_id: "session-1", turn_id: "turn-1", display_name: "brief.md", kind: "text",
      mime_type: "text/markdown", byte_size: 8, content_digest: "b".repeat(64),
      committed_position: 17, verification: "not_run", preview: "text",
      workspace_id: "workspace-1", revealable: true, exportable: true };
    const invoke = async <T>(command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      return (command === "prepare_artifact_export"
        ? { schema_version: 1, export_target_id: "target-1", display_name: "copy.md",
          state: "ready", expires_at: "2026-08-30T16:00:00Z" }
        : { schema_version: 1, artifact_id: "artifact-1", revision: 2, display_name: "copy.md",
          byte_size: 8, content_digest: "b".repeat(64), state: "exported" }) as T;
    };
    const target = await prepareArtifactExport("session-1", artifact, invoke);
    const receipt = await commitArtifactExport("session-1", artifact, target!.export_target_id, invoke);
    expect(receipt.state).toBe("exported");
    const coordinates = { sessionId: "session-1", artifactId: "artifact-1", revision: 2,
      committedPosition: 17 };
    expect(calls).toEqual([
      { command: "prepare_artifact_export", args: { request: coordinates } },
      { command: "commit_artifact_export", args: { request: {
        ...coordinates, exportTargetId: "target-1",
      } } },
    ]);
    expect(JSON.stringify(calls)).not.toContain("/Users/");
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

  it("reads only aggregate Workspace recovery health", async () => {
    const status = await getWorkspaceRecoveryStatus(async <T>(command: string, args: Record<string, unknown>) => {
      expect({ command, args }).toEqual({ command: "get_workspace_recovery_status", args: {} });
      return { schema_version: 1, state: "attention_required", restored_count: 2,
        needs_reauthorization_count: 1 } as T;
    });
    expect(status).toEqual({ schema_version: 1, state: "attention_required", restored_count: 2,
      needs_reauthorization_count: 1 });
    expect(JSON.stringify(status)).not.toContain("/");
  });

  it("lists and reauthorizes Workspaces without paths crossing IPC", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const invoke = async <T>(command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      if (command === "list_workspace_authorizations") return [{ schema_version: 1,
        workspace_id: "workspace-1", display_name: "Project", grant_revision: 1,
        state: "needs_reauthorization" }] as T;
      return { schema_version: 1, workspace_id: "workspace-1", display_name: "Project",
        access: "enumerate", grant_revision: 2, state: "active",
        expires_at: "2026-08-30T15:30:00Z" } as T;
    };
    const items = await listWorkspaceAuthorizations(invoke);
    const renewed = await reauthorizeWorkspace(items[0].workspace_id, invoke);
    expect(renewed?.grant_revision).toBe(2);
    expect(calls).toEqual([
      { command: "list_workspace_authorizations", args: {} },
      { command: "reauthorize_workspace", args: { workspaceId: "workspace-1" } },
    ]);
    expect(JSON.stringify(calls)).not.toContain("/");
  });

  it("uses typed write authorization and approval commands", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const invoke = async <T>(command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      if (command === "authorize_workspace_writes") return { schema_version: 1,
        workspace_id: "workspace-1", display_name: "Project", access: "read_write",
        grant_revision: 2, state: "active", expires_at: "2026-08-30T15:30:00Z" } as T;
      return { session_id: "session-1", turn_id: "turn-1", execution_id: "execution-2",
        cursor: 19, text: "done", terminal: "completed" } as T;
    };
    const grant = await authorizeWorkspaceWrites("workspace-1", invoke);
    await resolveTurnApproval("session-1", "turn-1", {
      suspension_id: "suspension-1", session_version: 4, kind: "approval_required",
    }, true, invoke);
    expect(grant?.access).toBe("read_write");
    expect(calls).toEqual([
      { command: "authorize_workspace_writes", args: { workspaceId: "workspace-1" } },
      { command: "resolve_turn_approval", args: { sessionId: "session-1", turnId: "turn-1",
        suspensionId: "suspension-1", sessionVersion: 4, approved: true } },
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

  it("lists bounded opaque Workspace entry pages", async () => {
    const invoke = async <T>(command: string, args: Record<string, unknown>) => {
      expect(command).toBe("list_workspace_entries");
      expect(args).toEqual({
        workspaceId: "workspace-1", parentEntryId: null, cursor: null, limit: 32,
      });
      return {
        schema_version: 1, workspace_id: "workspace-1", parent_entry_id: null,
        entries: [{ schema_version: 1, entry_id: "entry-1", parent_entry_id: null,
          display_name: "brief.md", kind: "text", byte_size: 12, selectable: true }],
        next_cursor: null, has_more: false,
      } as T;
    };
    const page = await listWorkspaceEntries("workspace-1", undefined, undefined, 32, invoke);
    expect(page.entries[0].entry_id).toBe("entry-1");
  });

  it("sends only opaque selected entry identities for a contextual Turn", async () => {
    const invoke = async <T>(command: string, args: Record<string, unknown>) => {
      expect(command).toBe("run_agent_turn_with_workspace_context");
      expect(args).toEqual({
        request: {
          definitionId: "definition-main", sessionId: "session-1", input: "summarize",
          workspaceId: "workspace-1", entryIds: ["entry-1"],
        },
      });
      expect(JSON.stringify(args)).not.toContain("file content");
      return { session_id: "session-1", turn_id: "turn-1", execution_id: "execution-1",
        cursor: 8, text: "done", terminal: "completed" } as T;
    };
    const result = await runAgentTurnWithWorkspaceContext(
      "definition-main", "session-1", "summarize", "workspace-1", ["entry-1"], invoke,
    );
    expect(result.text).toBe("done");
  });
});
