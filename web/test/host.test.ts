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

  it("rejects non-loopback configuration and redacts host failures", async () => {
    expect(() => new FetchHostClient("https://example.com/", limits())).toThrowError(HostClientError);
    const client = new FetchHostClient("http://localhost:1234/", limits(), async () =>
      jsonResponse({ code: "not_found", message: "ignored" }, 404));
    await expect(client.createSession("create", "definition")).rejects.toMatchObject({
      code: "host_failure", status: 404, message: "host_failure",
    });
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
function sseResponse(events: HostEvent[]): Response {
  const body = events.map((event) => `id: ${event.position}\nevent: host\ndata: ${JSON.stringify(event)}\n\n`).join("");
  return new Response(body, { status: 200, headers: { "Content-Type": "text/event-stream" } });
}
