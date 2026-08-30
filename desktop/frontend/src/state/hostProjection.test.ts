import { describe, expect, it } from "vitest";

import type { HostDefinitionPage, HostEvent, HostSessionPage, HostTimelinePage } from "../ipc/host";
import { mapDefinitions, mapHostEvent, mapSessions, mapTimeline, ProductHostMappingError } from "./hostProjection";

const DIGEST = "a".repeat(64);

describe("A-UX1 Host product projection", () => {
  it("retains complete H2/H3 public values", () => {
    const definitions: HostDefinitionPage = { api_version: "v1", definitions: [{ api_version: "v1",
      definition_id: "definition-a", definition_revision: "revision-a", capabilities: ["chat", "tools"] }] };
    const sessions: HostSessionPage = { api_version: "v1", sessions: [{ api_version: "v1",
      session_id: "session-a", agent_instance_id: "agent-a", definition_id: "definition-a",
      definition_revision: "revision-a", opened_at: "2026-08-30T00:00:00Z", latest_position: 9,
      latest_turn_id: "turn-a", latest_turn_state: "suspended", turn_count: 1 }] };
    const timeline = completeTimeline();
    expect(mapDefinitions(definitions).definitions[0]).toEqual({ definitionId: "definition-a",
      definitionRevision: "revision-a", capabilities: ["chat", "tools"] });
    expect(mapSessions(sessions).sessions[0]?.definitionRevision).toBe("revision-a");
    const mapped = mapTimeline(timeline, "session-a");
    expect(mapped.items[0]).toMatchObject({ userText: "hello", completionText: undefined,
      contentTruncated: false, suspension: { titleKey: "suspension.approval.title",
        actionLabelKey: "suspension.approval.continue", responseSchemaDigest: DIGEST } });
    expect(mapped.activities[0]).toMatchObject({ labelKey: "agent.activity.effect", terminal: false });
  });

  it("preserves an unknown event neutrally and rejects unsafe protocol shapes", () => {
    const event: HostEvent = { api_version: "v1", session_id: "session-a", position: 10,
      event: "future.event", turn_id: "turn-a", execution_id: "execution-a", text: "" };
    expect(mapHostEvent(event, "session-a")).toEqual({ type: "host_event", event: "future.event",
      position: 10, turnId: "turn-a", activity: undefined });
    const base = completeTimeline();
    const invalid: HostTimelinePage = { ...base, items: base.items.map((item) => ({ ...item,
      suspension: item.suspension && { ...item.suspension, prompt_json: [...new TextEncoder().encode(
        JSON.stringify({ schema_version: 1, title_key: "safe", action_label_key: "safe", secret: "leak" }),
      )] } })) };
    let failure: unknown;
    try { mapTimeline(invalid, "session-a"); } catch (error) { failure = error; }
    expect(failure).toBeInstanceOf(ProductHostMappingError);
    expect(String(failure)).not.toContain("leak");
  });
});

function completeTimeline(): HostTimelinePage {
  return { api_version: "v1", session_id: "session-a", scanned_through_position: 9,
    observed_max_position: 9, has_more: false, items: [{ turn_id: "turn-a", started_position: 1,
      latest_position: 9, state: "suspended", user_text: "hello", content_truncated: false,
      suspension: { suspension_id: "suspension-a", session_version: 3, kind: "approval",
        prompt_schema: "garive.public-suspension-prompt.v1", prompt_json: [...new TextEncoder().encode(JSON.stringify({
          schema_version: 1, title_key: "suspension.approval.title",
          action_label_key: "suspension.approval.continue",
        }))], prompt_digest: DIGEST, response_schema_json: [123, 125], response_schema_digest: DIGEST },
      activities: [{ api_version: "v1", activity_id: "activity-a", kind: "effect",
        label_key: "agent.activity.effect", state: "running", source_position: 8, terminal: false }] }] };
}
