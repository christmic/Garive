import { beforeEach, describe, expect, it } from "vitest";
import type { AppEffect, AppViewState } from "../../desktop/frontend/src/state/controller";
import { WebProductEffectPort } from "../src/WebProductEffectPort";
import type { FetchHostClient } from "../src/host";

const storage = new Map<string, string>();
beforeEach(() => {
  storage.clear();
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key),
  } });
});

describe("Web product effect port", () => {
  it("maps live H2 documents into the shared controller contract", async () => {
    const host = {
      readDefinitions: async () => ({ api_version: "v1", definitions: [{ api_version: "v1",
        definition_id: "agent-main", definition_revision: "revision-a", capabilities: ["text"] }] }),
      readSessions: async () => ({ api_version: "v1", sessions: [{ api_version: "v1",
        session_id: "session-a", agent_instance_id: "agent-a", definition_id: "agent-main",
        definition_revision: "revision-a", opened_at: "2026-08-31T00:00:00Z",
        latest_position: 4, latest_turn_id: "turn-a", latest_turn_state: "completed", turn_count: 1 }] }),
    } as unknown as FetchHostClient;
    const port = new WebProductEffectPort(host);
    expect(await collect(port, effect("load_definitions"))).toEqual([{ type: "definitions_loaded",
      definitions: [{ definitionId: "agent-main", definitionRevision: "revision-a", capabilities: ["text"] }] }]);
    expect(await collect(port, effect("load_session_page"))).toMatchObject([{ type: "session_page_loaded",
      sessions: [{ sessionId: "session-a", state: "completed", turnCount: 1 }] }]);
  });

  it("persists browser preferences through the same bounded codec", async () => {
    const port = new WebProductEffectPort({} as FetchHostClient);
    const state = { ...snapshot(), selectedSessionId: "session-a",
      drafts: [{ sessionId: "session-a", text: "kept draft" }] };
    expect(await collect(port, effect("save_preferences"), state)).toEqual([{ type: "preferences_saved" }]);
    expect(await collect(port, effect("load_preferences"))).toEqual([{ type: "preferences_loaded",
      selectedSessionId: "session-a", drafts: [{ sessionId: "session-a", text: "kept draft" }],
      pending: undefined }]);
  });
});

async function collect(port: WebProductEffectPort, effectValue: AppEffect, state = snapshot()) {
  const values = []; for await (const value of port.run(effectValue, state, new AbortController().signal)) values.push(value);
  return values;
}
function effect(kind: AppEffect["kind"]): AppEffect { return { effectId: `effect-${kind}`, kind, generation: 1 }; }
function snapshot(): AppViewState {
  return { configuration: "configured", shell: "ready", generation: 1, nextEffect: 2,
    definitions: [], sessions: [], timeline: [], cursor: 0, drafts: [], execution: "idle",
    pending: [], activities: [], outstanding: [], preferenceDirty: false };
}
