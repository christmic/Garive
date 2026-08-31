import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  FetchHostClient, HOST_CLIENT_FAILURES, HostClientError, type HostEvent,
  reduceHostEvents,
} from "../src/host";

interface Fixture {
  session_id: string;
  valid_stream: HostEvent[];
  expected: { cursor: number; terminal: string; text: string; unknown_events: string[] };
  reconnect: { after_position: number; events: HostEvent[]; expected_applied_positions: number[] };
  disconnect_before_terminal: HostEvent[];
  invalid_streams: Array<{ mutation: string; expected: string }>;
  failure_codes: string[];
}
const fixture = JSON.parse(readFileSync(resolve(
  import.meta.dirname, "../../spec/fixtures/host/live-host-client-v1.json",
), "utf8")) as Fixture;

describe("A1 event reducer", () => {
  it("consumes gaps, unknown events, reconnect duplicates and terminal text", () => {
    const view = reduceHostEvents(fixture.session_id, fixture.valid_stream);
    expect(view.cursor).toBe(fixture.expected.cursor);
    expect(view.terminal).toBe(fixture.expected.terminal);
    expect(view.text).toBe(fixture.expected.text);
    expect(view.unknownEvents).toEqual(fixture.expected.unknown_events);
    const initial = reduceHostEvents(fixture.session_id, fixture.valid_stream.slice(0, 2));
    const reconnected = reduceHostEvents(fixture.session_id, fixture.reconnect.events, initial);
    expect(Object.keys(reconnected.fingerprints).map(Number).filter((value) => value > 2)).toEqual(
      fixture.reconnect.expected_applied_positions,
    );
  });

  it("never invents a terminal on disconnect", () => {
    expect(reduceHostEvents(fixture.session_id, fixture.disconnect_before_terminal).terminal).toBeUndefined();
  });

  it("returns every shared invalid-stream code", () => {
    for (const test of fixture.invalid_streams) {
      const events = mutation(test.mutation);
      try {
        reduceHostEvents(fixture.session_id, events);
        throw new Error(`accepted ${test.mutation}`);
      } catch (error) {
        expect(error).toBeInstanceOf(HostClientError);
        expect((error as HostClientError).code, test.mutation).toBe(test.expected);
      }
    }
    expect(HOST_CLIENT_FAILURES).toEqual(fixture.failure_codes);
  });
});

describe("A1 fetch transport", () => {
  it("reads H2 navigation and exposes ordered SSE events to the shared product controller", async () => {
    const calls: string[] = [];
    const responses = [
      jsonResponse({ api_version: "v1", definitions: [] }),
      jsonResponse({ api_version: "v1", sessions: [] }),
      jsonResponse({ api_version: "v1", session_id: fixture.session_id, items: [],
        scanned_through_position: 0, observed_max_position: 0, has_more: false }),
      sseResponse(fixture.valid_stream.slice(0, 2)),
    ];
    const client = new FetchHostClient("http://127.0.0.1:1430/", limits(), async (input) => {
      calls.push(String(input)); return responses.shift()!;
    });
    await client.readDefinitions(); await client.readSessions();
    await client.readTimeline(fixture.session_id);
    const controller = new AbortController(); const positions: number[] = [];
    try {
      for await (const event of client.followEvents(fixture.session_id, 0, controller.signal)) {
        positions.push(event.position); if (positions.length === 2) controller.abort();
      }
    } catch (error) {
      expect(error).toMatchObject({ code: "transport_failure" });
    }
    expect(calls.slice(0, 3)).toEqual([
      "http://127.0.0.1:1430/v1/agent-definitions",
      "http://127.0.0.1:1430/v1/sessions?limit=64",
      `http://127.0.0.1:1430/v1/sessions/${fixture.session_id}/timeline?after_position=0&limit=64`,
    ]);
    expect(positions).toEqual([1, 2]);
  });

  it("sends stable command IDs and follows SSE until durable terminal", async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const responses = [
      jsonResponse({ session_id: fixture.session_id, agent_instance_id: "agent", committed_position: 1 }),
      jsonResponse({ session_id: fixture.session_id, turn_id: "turn-client", execution_id: "execution-client", committed_position: 4 }),
      sseResponse(fixture.valid_stream),
    ];
    const client = new FetchHostClient("http://127.0.0.1:1234/", limits(), async (input, init) => {
      calls.push({ url: String(input), init });
      return responses.shift() ?? new Response(null, { status: 500 });
    });
    const session = await client.createSession("create-stable", "definition-main");
    await client.startTurn("turn-stable", session.session_id, "hello");
    const terminal = await client.followUntilTerminal(session.session_id);
    expect(terminal.terminal).toBe("completed");
    expect(terminal.text).toBe("durable answer");
    expect((calls[0]?.init?.headers as Record<string, string>)["Idempotency-Key"]).toBe("create-stable");
    expect((calls[1]?.init?.headers as Record<string, string>)["Idempotency-Key"]).toBe("turn-stable");
    expect(calls[2]?.url).toContain("after_position=0");
  });

  it("follows the bounded ephemeral live-output endpoint independently of durable events", async () => {
    const output = { api_version: "v1", session_id: fixture.session_id, turn_id: "turn-live",
      execution_id: "execution-live", stream_id: "stream-live", sequence: 1,
      kind: "text_delta", text: "real delta" };
    const client = new FetchHostClient("http://127.0.0.1:1234/", limits(), async () =>
      sseResponse([output]));
    const controller = new AbortController(); const observed: unknown[] = [];
    try {
      for await (const event of client.followLiveOutput(fixture.session_id, controller.signal)) {
        observed.push(event); controller.abort();
      }
    } catch (error) { expect(error).toMatchObject({ code: "transport_failure" }); }
    expect(observed).toEqual([output]);
  });

  it("rejects non-loopback configuration and redacts host failures", async () => {
    expect(() => new FetchHostClient("https://example.com/", limits())).toThrowError(HostClientError);
    const client = new FetchHostClient("http://localhost:1234/", limits(), async () =>
      jsonResponse({ code: "not_found", message: "ignored" }, 404));
    await expect(client.createSession("create", "definition")).rejects.toMatchObject({
      code: "host_failure", status: 404, message: "host_failure",
    });
  });

  it("maps cancel and continuation to the exact H1 mutations", async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const turn = { session_id: fixture.session_id, turn_id: "turn-client",
      execution_id: "execution-client", committed_position: 12 };
    const client = new FetchHostClient("http://127.0.0.1:1234/", limits(), async (input, init) => {
      calls.push({ url: String(input), init }); return jsonResponse(turn);
    });
    await client.cancelTurn("cancel-stable", fixture.session_id, "turn-client", 9);
    await client.continueTurn(
      "continue-stable", fixture.session_id, "turn-client", "suspension-client", 4, "approved input",
    );
    expect(calls[0]?.url).toContain("/v1/turns/turn-client:cancel");
    expect(calls[0]?.init?.body).toBe(JSON.stringify({
      session_id: fixture.session_id, requested_through_position: 9,
    }));
    expect(calls[1]?.url).toContain("/v1/turns/turn-client:continue");
    expect(calls[1]?.init?.body).toBe(JSON.stringify({
      session_id: fixture.session_id, suspension_id: "suspension-client",
      expected_session_version: 4, input: "approved input",
    }));
  });
});

function mutation(name: string): HostEvent[] {
  const first = { ...fixture.valid_stream[0]! };
  switch (name) {
    case "api_version_v2": return [{ ...first, api_version: "v2" }];
    case "session_other": return [{ ...first, session_id: "other" }];
    case "position_zero": return [{ ...first, position: 0 }];
    case "position_backward": return [{ ...first, position: 2 }, { ...first, position: 1 }];
    case "duplicate_conflict": return [{ ...first, position: 2 }, { ...first, position: 2, event: "turn.started" }];
    case "event_count_17": return Array.from({ length: 17 }, (_, index) => ({ ...first, position: index + 1 }));
    default: throw new Error(`unknown mutation ${name}`);
  }
}

function limits() {
  return { maxCommandBytes: 4_096, maxEventBytes: 8_192, maxEvents: 16, followDeadlineMs: 1_000 };
}
function jsonResponse(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });
}
function sseResponse(events: readonly unknown[]): Response {
  const body = events.map((event) => `event: host\ndata: ${JSON.stringify(event)}\n\n`).join("");
  return new Response(body, { status: 200, headers: { "Content-Type": "text/event-stream" } });
}
