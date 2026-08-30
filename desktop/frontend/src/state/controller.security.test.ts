import { describe, expect, it } from "vitest";

import { initialAppViewState, reduceApp, type AppViewState } from "./controller";
import { JsonPreferenceAdapter, type PreferenceBytesPort } from "./preferences";

const DIGEST = "a".repeat(64);

describe("A-UX1 controller boundaries", () => {
  it("preserves an in-flight mutation while navigation invalidates only reads", () => {
    let state = ready(["session-a", "session-b"], "session-a");
    state = reduceApp(state, { type: "edit_draft", sessionId: "session-a", text: "hello" }).state;
    const started = reduceApp(state, { type: "submit_draft", sessionId: "session-a",
      commandId: "command-a", requestDigest: DIGEST });
    const navigated = reduceApp(started.state, { type: "select_session", sessionId: "session-b" });
    expect(navigated.state.pending.map((item) => item.commandId)).toEqual(["command-a"]);
    expect(navigated.state.outstanding.some((item) => item.kind === "start_turn")).toBe(true);
    expect(navigated.state.outstanding.some((item) => item.kind === "load_timeline" && item.sessionId === "session-b")).toBe(true);
  });

  it("ignores a result whose exact correlation coordinates are forged", () => {
    let state = ready(["session-a"], "session-a");
    state = reduceApp(state, { type: "edit_draft", sessionId: "session-a", text: "hello" }).state;
    const started = reduceApp(state, { type: "submit_draft", sessionId: "session-a",
      commandId: "command-a", requestDigest: DIGEST });
    const effect = started.effects[0]!;
    const forged = reduceApp(started.state, { type: "effect_result", effectId: effect.effectId,
      generation: effect.generation, sessionId: "session-other", requestDigest: effect.requestDigest,
      result: { type: "command_succeeded", sessionId: "session-a", turnId: "turn-a", committedPosition: 4 } });
    expect(forged.state).toBe(started.state);
    expect(forged.effects).toEqual([]);
  });

  it("admits at most one mutation per Session and counts UTF-8 bytes", () => {
    let state = ready(["session-a"], "session-a");
    state = reduceApp(state, { type: "edit_draft", sessionId: "session-a", text: "ok" },
      { maxDraftBytes: 3, maxActivities: 2 }).state;
    state = reduceApp(state, { type: "submit_draft", sessionId: "session-a",
      commandId: "command-a", requestDigest: DIGEST }, { maxDraftBytes: 3, maxActivities: 2 }).state;
    const second = reduceApp(state, { type: "submit_draft", sessionId: "session-a",
      commandId: "command-b", requestDigest: "b".repeat(64) }, { maxDraftBytes: 3, maxActivities: 2 });
    expect(second.state.notice?.code).toBe("command_not_admitted");
    const oversized = reduceApp(second.state, { type: "edit_draft", sessionId: "session-a", text: "🦀" },
      { maxDraftBytes: 3, maxActivities: 2 });
    expect(oversized.state.notice?.code).toBe("draft_too_large");
  });

  it("coalesces preference writes while preserving the latest state", () => {
    const first = reduceApp(ready(["session-a"], "session-a"),
      { type: "edit_draft", sessionId: "session-a", text: "first" });
    const second = reduceApp(first.state,
      { type: "edit_draft", sessionId: "session-a", text: "latest" });
    expect(second.effects).toEqual([]);
    expect(second.state.outstanding.filter((effect) => effect.kind === "save_preferences")).toHaveLength(1);
    const save = first.effects[0]!;
    const completed = reduceApp(second.state, { type: "effect_result", effectId: save.effectId,
      generation: save.generation, result: { type: "preferences_saved" } });
    expect(completed.effects.map((effect) => effect.kind)).toEqual(["save_preferences"]);
    expect(completed.state.drafts).toEqual([{ sessionId: "session-a", text: "latest" }]);
    expect(completed.state.preferenceDirty).toBe(false);
  });

  it("round-trips a minimal pending record and clears only a corrupt record", async () => {
    const port = new MemoryPort(); const adapter = new JsonPreferenceAdapter(port,
      { max_document_bytes: 1024, max_drafts: 4, max_id_bytes: 128, max_draft_bytes: 256 });
    await adapter.savePending({ kind: "start_turn", commandId: "command-a", requestDigest: DIGEST,
      generation: 3, sessionId: "session-a", status: "unknown" });
    expect((await adapter.load()).pending?.commandId).toBe("command-a");
    port.pending = new TextEncoder().encode("{\"schema_version\":2}");
    const loaded = await adapter.load();
    expect(loaded.reset).toBe(true); expect(loaded.pending).toBeUndefined(); expect(port.pending).toBeUndefined();
  });
});

function ready(ids: readonly string[], selected: string): AppViewState {
  return { ...initialAppViewState(), shell: "ready", generation: 1,
    sessions: ids.map((sessionId) => ({ sessionId })), selectedSessionId: selected };
}
class MemoryPort implements PreferenceBytesPort {
  public preferences?: Uint8Array; public pending?: Uint8Array;
  public async readPreferences() { return this.preferences; } public async writePreferences(value: Uint8Array) { this.preferences = value; }
  public async readPendingCommand() { return this.pending; } public async writePendingCommand(value: Uint8Array | undefined) { this.pending = value; }
}
