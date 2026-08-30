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
});
