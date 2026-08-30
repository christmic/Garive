import { describe, expect, it } from "vitest";
import { canSubmit, initialWorkState, reduceWork } from "./workspace";

const capabilities = {
  configured: true, agent_definition_id: "definition-main", multi_turn: true, durable_navigation: false,
  activity: false, setup: false, workspaces: false, artifacts: false,
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
  });
});
