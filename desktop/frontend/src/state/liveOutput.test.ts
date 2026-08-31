import { describe, expect, it } from "vitest";
import { initialAppViewState, reduceApp, type AppEffect, type AppEffectPayload,
  type AppViewState } from "./controller";
import { decodeLiveOutput } from "./liveOutput";

describe("H4 live output projection", () => {
  it("decodes snapshots and rejects identity or sequence drift", () => {
    expect(decodeLiveOutput(raw("snapshot", 3, { text: "hello", through_sequence: 3 }), "session-1"))
      .toMatchObject({ kind: "snapshot", sequence: 3, text: "hello", throughSequence: 3 });
    expect(() => decodeLiveOutput({ ...raw("snapshot", 3, { text: "hello", through_sequence: 2 }) }, "session-1"))
      .toThrow("invalid_live_output");
    expect(() => decodeLiveOutput({ ...raw("text_delta", 4, { text: "!" }), session_id: "other" }, "session-1"))
      .toThrow("invalid_live_output");
  });

  it("shows exact deltas, fails closed on gaps, and yields to durable terminal truth", () => {
    const effect: AppEffect = { effectId: "effect-live", kind: "follow_events", generation: 0,
      sessionId: "session-1", afterPosition: 2 };
    let state: AppViewState = { ...initialAppViewState(), selectedSessionId: "session-1", cursor: 2,
      timeline: [{ turnId: "turn-1", state: "running", latestPosition: 2 }],
      execution: "following" as const, outstanding: [effect] };
    state = apply(state, effect, { type: "live_output", output:
      decodeLiveOutput(raw("snapshot", 2, { text: "Hello", through_sequence: 2 }), "session-1") });
    state = apply(state, effect, { type: "live_output", output:
      decodeLiveOutput(raw("text_delta", 3, { text: " world" }), "session-1") });
    expect(state.livePreview).toMatchObject({ text: "Hello world", available: true, sequence: 3 });
    state = apply(state, effect, { type: "live_output", output:
      decodeLiveOutput(raw("text_delta", 5, { text: " hidden" }), "session-1") });
    expect(state.livePreview).toMatchObject({ text: "", available: false });
    state = apply(state, effect, { type: "host_event", event: "turn.completed", position: 6,
      turnId: "turn-1", text: "Durable answer" });
    expect(state.livePreview).toBeUndefined();
    expect(state.timeline[0]).toMatchObject({ state: "completed", completionText: "Durable answer" });
  });
});

function apply(state: AppViewState, effect: AppEffect, result: AppEffectPayload) {
  return reduceApp(state, { type: "effect_result", effectId: effect.effectId, generation: effect.generation,
    sessionId: effect.sessionId, result }).state;
}
function raw(kind: string, sequence: number, extra: Record<string, unknown>) {
  return { api_version: "v1", session_id: "session-1", turn_id: "turn-1",
    execution_id: "execution-1", stream_id: "stream-1", sequence, kind, ...extra };
}
