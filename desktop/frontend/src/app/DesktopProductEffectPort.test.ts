import { describe, expect, it } from "vitest";
import { initialAppViewState, type AppEffect, type AppViewState } from "../state/controller";
import type { PreferenceBytesPort } from "../state/preferences";
import { DesktopProductEffectPort } from "./DesktopProductEffectPort";

const DIGEST = "a".repeat(64);

describe("DesktopProductEffectPort", () => {
  it("restores preferences and exact pending identity through the product effect", async () => {
    const storage = new MemoryPreferences();
    storage.preferences = bytes(JSON.stringify({ schema_version: 1, selected_session_id: "session-1",
      session_rail: "expanded", activity_inspector: "closed", theme: "system",
      composer_drafts: [{ session_id: "session-1", text: "draft" }] }));
    storage.pending = bytes(JSON.stringify({ schema_version: 1, kind: "start_turn",
      command_id: "command-1", semantic_request_digest: DIGEST, session_id: "session-1",
      issued_generation: 2, status: "pending" }));
    const port = new DesktopProductEffectPort(async <T>() => undefined as T, storage);
    const result = await first(port, effect("load_preferences"), initialAppViewState());
    expect(result).toEqual({ type: "preferences_loaded", selectedSessionId: "session-1",
      drafts: [{ sessionId: "session-1", text: "draft" }], pending: expect.objectContaining({
        commandId: "command-1", requestDigest: DIGEST,
      }) });
  });

  it("persists command identity before IPC and clears only after exact durable receipt", async () => {
    const order: string[] = []; const storage = new MemoryPreferences(order);
    const invoke = async <T>(command: string) => {
      order.push(`invoke:${command}`);
      return { session_id: "session-1", turn_id: "turn-1", execution_id: "execution-1",
        committed_position: 7 } as T;
    };
    const port = new DesktopProductEffectPort(invoke, storage);
    const pending = { kind: "start_turn" as const, commandId: "command-1", requestDigest: DIGEST,
      generation: 2, sessionId: "session-1", status: "pending" as const };
    const snapshot: AppViewState = { ...initialAppViewState(), pending: [pending] };
    const result = await first(port, { ...effect("start_turn"), sessionId: "session-1",
      commandId: "command-1", requestDigest: DIGEST, text: "hello" }, snapshot);
    expect(result).toEqual({ type: "command_succeeded", sessionId: "session-1",
      turnId: "turn-1", committedPosition: 7 });
    expect(order.indexOf("write:pending")).toBeLessThan(order.indexOf("invoke:start_product_turn"));
    expect(order.at(-1)).toBe("write:clear");
    expect(storage.pending).toBeUndefined();
  });

  it("follows strictly advancing public events and stops on abort", async () => {
    const invoke = async <T>(command: string) => {
      expect(command).toBe("get_session_events");
      return { events: [{ api_version: "v1", session_id: "session-1", position: 8,
        event: "turn.completed", turn_id: "turn-1", execution_id: "execution-1", text: "done" }],
      scanned_through_position: 8, observed_max_position: 8 } as T;
    };
    const port = new DesktopProductEffectPort(invoke, new MemoryPreferences());
    const controller = new AbortController();
    const iterator = port.run({ ...effect("follow_events"), sessionId: "session-1", afterPosition: 7 },
      initialAppViewState(), controller.signal)[Symbol.asyncIterator]();
    expect((await iterator.next()).value).toEqual({ type: "host_event", event: "turn.completed",
      position: 8, turnId: "turn-1", text: "done", activity: undefined });
    controller.abort();
    expect((await iterator.next()).done).toBe(true);
  });
});

async function first(port: DesktopProductEffectPort, value: AppEffect, state: AppViewState) {
  const iterator = port.run(value, state, new AbortController().signal)[Symbol.asyncIterator]();
  return (await iterator.next()).value;
}
function effect(kind: AppEffect["kind"]): AppEffect {
  return { effectId: "effect-1", kind, generation: 1 };
}
function bytes(value: string): Uint8Array { return new TextEncoder().encode(value); }

class MemoryPreferences implements PreferenceBytesPort {
  public preferences?: Uint8Array; public pending?: Uint8Array;
  public constructor(private readonly order: string[] = []) {}
  public async readPreferences() { this.order.push("read:preferences"); return this.preferences; }
  public async writePreferences(value: Uint8Array) { this.order.push("write:preferences"); this.preferences = value; }
  public async readPendingCommand() { this.order.push("read:pending"); return this.pending; }
  public async writePendingCommand(value: Uint8Array | undefined) {
    this.order.push(value ? "write:pending" : "write:clear"); this.pending = value;
  }
}
