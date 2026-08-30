import { describe, expect, it } from "vitest";

import type { AppEffect, AppEffectPayload } from "../state/controller";
import { ProductPortError, ProductRuntime, type ProductEffectPort } from "./ProductRuntime";

describe("UX-B Desktop product composition", () => {
  it("drives ordered bootstrap effects back through exact reducer correlation", async () => {
    const port = new ScriptedPort({
      load_preferences: [{ type: "preferences_loaded", selectedSessionId: "session-a", drafts: [] }],
      load_definitions: [{ type: "definitions_loaded", definitions: [{ definitionId: "definition-a",
        definitionRevision: "revision-a", capabilities: [] }] }],
      load_session_page: [{ type: "session_page_loaded", sessions: [{ sessionId: "session-a" }] }],
      load_timeline: [{ type: "timeline_loaded", items: [], cursor: 4, activities: [] }],
    });
    const runtime = new ProductRuntime(port); runtime.dispatch({ type: "boot" });
    await eventually(() => runtime.state.shell === "ready" && runtime.state.cursor === 4);
    expect(port.effects).toEqual(["load_preferences", "load_definitions", "load_session_page", "load_timeline"]);
    expect(runtime.state.selectedSessionId).toBe("session-a"); runtime.dispose();
  });

  it("turns an exhausted follow stream into an explicit disconnect", async () => {
    const runtime = new ProductRuntime(new ScriptedPort({
      load_preferences: [{ type: "preferences_loaded", selectedSessionId: "session-a", drafts: [] }],
      load_definitions: [{ type: "definitions_loaded", definitions: [{ definitionId: "definition-a",
        definitionRevision: "revision-a", capabilities: [] }] }],
      load_session_page: [{ type: "session_page_loaded", sessions: [{ sessionId: "session-a" }] }],
      load_timeline: [{ type: "timeline_loaded", items: [{ turnId: "turn-a", state: "running",
        latestPosition: 9 }], cursor: 9, activities: [] }], follow_events: [],
    }));
    runtime.dispatch({ type: "boot" });
    await eventually(() => runtime.state.notice?.code === "stream_ended");
    expect(runtime.state.execution).toBe("disconnected"); runtime.dispose();
  });

  it("redacts unknown port failures and aborts orphaned navigation work", async () => {
    const port: ProductEffectPort = { run: async function* (effect, _snapshot, signal) {
      if (effect.kind === "load_preferences") {
        await new Promise<void>((resolve) => signal.addEventListener("abort", () => resolve(), { once: true }));
        return;
      }
      throw new Error("secret raw response");
    } };
    const runtime = new ProductRuntime(port); runtime.dispatch({ type: "boot" });
    await eventually(() => runtime.state.notice?.code === "product_port_failure");
    expect(JSON.stringify(runtime.state)).not.toContain("secret raw response");
    runtime.dispose();
  });

  it("preserves typed safe failures from an admitted port", async () => {
    const port: ProductEffectPort = { run: async function* () {
      throw new ProductPortError("host", "host_unavailable");
    } };
    const runtime = new ProductRuntime(port); runtime.dispatch({ type: "boot" });
    await eventually(() => runtime.state.notice?.code === "host_unavailable");
    expect(runtime.state.notice).toEqual({ kind: "host", code: "host_unavailable" });
    runtime.dispose();
  });
});

class ScriptedPort implements ProductEffectPort {
  public readonly effects: string[] = [];
  public constructor(private readonly scripts: Partial<Record<AppEffect["kind"], readonly AppEffectPayload[]>>) {}
  public async *run(effect: AppEffect): AsyncIterable<AppEffectPayload> {
    this.effects.push(effect.kind);
    for (const result of this.scripts[effect.kind] ?? []) yield result;
  }
}

async function eventually(predicate: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  throw new Error("condition_not_reached");
}
