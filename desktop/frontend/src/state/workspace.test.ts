import { describe, expect, it } from "vitest";
import { canSubmit, initialWorkState, reduceWork } from "./workspace";
import { initialAppViewState } from "./controller";

const capabilities = {
  configured: true, agent_definition_id: "definition-main", multi_turn: true, durable_navigation: false,
  activity: false, setup: false, workspaces: false, artifacts: false, updater: false,
};

describe("Desktop work state", () => {
  it("shows user input only after durable acknowledgement and reuses the Session", () => {
    let state = reduceWork(initialWorkState, { type: "capabilities_loaded", capabilities });
    state = reduceWork(state, { type: "draft_changed", value: "Prepare the review" });
    expect(canSubmit(state)).toBe(true);
    state = reduceWork(state, { type: "submission_started" });
    expect(state.messages).toEqual([]);
    state = reduceWork(state, { type: "submission_succeeded", input: state.draft, result: {
      session_id: "session-1", turn_id: "turn-1", execution_id: "execution-1",
      cursor: 9, text: "Review prepared", terminal: "completed",
    } });
    expect(state.sessionId).toBe("session-1");
    expect(state.messages.map((message) => message.role)).toEqual(["user", "assistant"]);
    expect(state.draft).toBe("");
  });

  it("retains the draft on failure and gates an unconfigured backend", () => {
    let state = reduceWork(initialWorkState, {
      type: "capabilities_loaded", capabilities: { ...capabilities, configured: false, multi_turn: false },
    });
    state = reduceWork(state, { type: "draft_changed", value: "private outcome" });
    expect(canSubmit(state)).toBe(false);
    state = reduceWork(state, { type: "submission_started" });
    state = reduceWork(state, { type: "submission_failed", code: "not_configured" });
    expect(state.draft).toBe("private outcome");
    expect(state.error).toBe("not_configured");
  });

  it("retains a next-instruction draft while the current turn is submitting", () => {
    let state = reduceWork(initialWorkState, { type: "capabilities_loaded", capabilities });
    state = reduceWork(state, { type: "draft_changed", value: "Current outcome" });
    state = reduceWork(state, { type: "submission_started" });
    state = reduceWork(state, { type: "draft_changed", value: "Follow up after completion" });
    expect(state.draft).toBe("Follow up after completion");
    expect(canSubmit(state)).toBe(false);
  });

  it("restores a conversation only from a durable timeline", () => {
    const state = reduceWork(initialWorkState, { type: "session_loaded", timeline: {
      api_version: "v1", session_id: "session-old", scanned_through_position: 7,
      observed_max_position: 7, has_more: false, items: [{
        turn_id: "turn-old", started_position: 2, latest_position: 7,
        state: "completed", user_text: "Recover this", completion_text: "Recovered",
        content_truncated: false, activities: [],
      }],
    } });
    expect(state.sessionId).toBe("session-old");
    expect(state.messages.map((message) => message.text)).toEqual(["Recover this", "Recovered"]);
  });

  it("admits only installed text suspension kinds through the composer", () => {
    const suspended = (kind: "partial_output" | "approval_required") => reduceWork({
      ...initialWorkState, boot: "ready", capabilities, draft: "continue",
    }, { type: "session_loaded", timeline: {
      api_version: "v1", session_id: "session-1", scanned_through_position: 7,
      observed_max_position: 7, has_more: false, items: [{ turn_id: "turn-1",
        started_position: 2, latest_position: 7, state: "suspended", user_text: "start",
        suspension: { suspension_id: "s-1", session_version: 3, kind }, content_truncated: false,
        activities: [] }],
    } });
    const partial = reduceWork(suspended("partial_output"), { type: "draft_changed", value: "continue" });
    const approval = reduceWork(suspended("approval_required"), { type: "draft_changed", value: "approve" });
    expect(canSubmit(partial)).toBe(true);
    expect(canSubmit(approval)).toBe(false);
  });

  it("restores only committed H3 activity from the durable timeline", () => {
    const state = reduceWork(initialWorkState, { type: "session_loaded", timeline: {
      api_version: "v1", session_id: "session-h3", scanned_through_position: 9,
      observed_max_position: 9, has_more: false, items: [{ turn_id: "turn-h3",
        started_position: 2, latest_position: 9, state: "completed", user_text: "read",
        completion_text: "done", content_truncated: false, activities: [{ api_version: "v1",
          activity_id: "activity-1", kind: "tool", label_key: "agent.activity.read_file",
          state: "completed", source_position: 8, terminal: true }],
      }],
    } });
    expect(state.activities).toHaveLength(1);
    expect(state.activities[0]?.state).toBe("completed");
    expect(state.messages[1]?.activities?.map((activity) => activity.activity_id))
      .toEqual(["activity-1"]);
  });

  it("admits Goal progress only from the matching durable Session projection", () => {
    const loaded = reduceWork(initialWorkState, { type: "session_loaded", timeline: {
      api_version: "v1", session_id: "session-1", scanned_through_position: 1,
      observed_max_position: 1, has_more: false, items: [] } });
    const page = { api_version: "v1" as const, session_id: "session-1", session_version: 3,
      observed_max_position: 7, goals: [{ api_version: "v1" as const, goal_id: "goal-1",
        revision: 2, state: "active" as const, definition_digest: "a".repeat(64),
        objective: "Deliver the desktop", objective_truncated: false, attempt_number: 1,
        criteria_total: 3, criteria_satisfied: 1 }] };
    const stale = reduceWork(loaded, { type: "goals_loaded", page: { ...page,
      session_id: "session-old" } });
    expect(stale.goals).toEqual([]);
    expect(reduceWork(loaded, { type: "goals_loaded", page }).goals).toEqual(page.goals);
  });

  it("admits Artifact projections only for the currently loaded Session", () => {
    const loaded = reduceWork(initialWorkState, { type: "session_loaded", timeline: {
      api_version: "v1", session_id: "session-1", scanned_through_position: 1,
      observed_max_position: 1, has_more: false, items: [],
    } });
    const artifact = { api_version: "v1" as const, artifact_id: "artifact-1", revision: 1,
      session_id: "session-1", turn_id: "turn-1", display_name: "brief.md", kind: "document",
      mime_type: "text/markdown", byte_size: 7, content_digest: "a".repeat(64),
      committed_position: 9, verification: "not_run", preview: "text",
      workspace_id: "workspace-1", revealable: true, exportable: true };
    const stale = reduceWork(loaded, { type: "artifacts_loaded", page: { api_version: "v1",
      session_id: "session-old", items: [artifact], scanned_through_position: 9,
      observed_max_position: 9, has_more: false } });
    expect(stale.artifacts).toEqual([]);
    const current = reduceWork(loaded, { type: "artifacts_loaded", page: { api_version: "v1",
      session_id: "session-1", items: [artifact], scanned_through_position: 9,
      observed_max_position: 9, has_more: false } });
    expect(current.artifacts).toEqual([artifact]);
  });

  it("keeps Workspace attachments scoped to the current durable Session", () => {
    const loaded = reduceWork(initialWorkState, { type: "session_loaded", timeline: {
      api_version: "v1", session_id: "session-1", scanned_through_position: 1,
      observed_max_position: 1, has_more: false, items: [],
    } });
    const attachment = { api_version: "v1" as const, session_id: "session-1",
      workspace_id: "workspace-1", display_name: "Launch materials", grant_revision: 2,
      access: "read_write" as const, attached_position: 4 };
    const stale = reduceWork(loaded, { type: "workspaces_loaded", sessionId: "session-old",
      workspaces: [attachment] });
    expect(stale.workspaces).toEqual([]);
    const current = reduceWork(loaded, { type: "workspaces_loaded", sessionId: "session-1",
      workspaces: [attachment] });
    expect(current.workspaces).toEqual([attachment]);
  });

  it("renders durable conversation state only from the product controller projection", () => {
    const view = { ...initialAppViewState(), shell: "ready" as const,
      selectedSessionId: "session-1", timelineSessionId: "session-1", execution: "following" as const,
      drafts: [{ sessionId: "session-1", text: "next" }], timeline: [{ turnId: "turn-1",
        state: "running", latestPosition: 4, userText: "hello", activities: [] }],
      activities: [{ activityId: "activity-1", kind: "tool", labelKey: "agent.activity.read_file",
        state: "running", turnId: "turn-1", position: 4, neutral: false }] };
    const state = reduceWork({ ...initialWorkState, capabilities }, { type: "product_projected", view });
    expect(state.sessionId).toBe("session-1");
    expect(state.phase).toBe("submitting");
    expect(state.execution).toBe("following");
    expect(state.draft).toBe("next");
    expect(state.messages).toEqual([{ id: "user-turn-1", role: "user", text: "hello" }]);
    expect(state.activities[0]?.activity_id).toBe("activity-1");
  });

  it("preserves disconnected execution truth for an explicit reconnect action", () => {
    const view = { ...initialAppViewState(), shell: "ready" as const,
      selectedSessionId: "session-1", timelineSessionId: "session-1",
      execution: "disconnected" as const, sessions: [{ sessionId: "session-1" }] };
    const state = reduceWork({ ...initialWorkState, capabilities }, { type: "product_projected", view });
    expect(state.boot).toBe("ready");
    expect(state.phase).toBe("idle");
    expect(state.execution).toBe("disconnected");
  });
});
