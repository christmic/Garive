import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

type JsonObject = Record<string, unknown>;

function fixture(name: string): JsonObject {
  return JSON.parse(readFileSync(resolve(
    import.meta.dirname,
    `../../spec/fixtures/host/${name}`,
  ), "utf8")) as JsonObject;
}

function exactFields(value: unknown, fields: readonly string[]): JsonObject {
  expect(value).not.toBeNull();
  expect(Array.isArray(value)).toBe(false);
  expect(typeof value).toBe("object");
  const object = value as JsonObject;
  expect(Object.keys(object).sort()).toEqual([...fields].sort());
  return object;
}

function cases(
  root: JsonObject,
  section: string,
  fields: readonly string[],
  names: Set<string>,
): JsonObject[] {
  const values = root[section];
  expect(Array.isArray(values)).toBe(true);
  return (values as unknown[]).map((value) => {
    const item = exactFields(value, fields);
    expect(typeof item.name).toBe("string");
    expect((item.name as string).length).toBeGreaterThan(0);
    expect(names.has(item.name as string)).toBe(false);
    names.add(item.name as string);
    return item;
  });
}

describe("shared Host projection fixtures", () => {
  it("keeps the H2 fixture strict and uniquely named", () => {
    const root = fixture("host-read-model-v1.json");
    exactFields(root, [
      "schema_version", "contract", "definition_cases", "session_page_cases",
      "session_view_cases", "timeline_cases", "cursor_cases", "failure_cases",
    ]);
    expect(root.schema_version).toBe(1);
    expect(root.contract).toBe("host-read-model-v1");
    const names = new Set<string>();
    cases(root, "definition_cases", ["name", "limit", "expected_ids", "error"], names);
    cases(root, "session_page_cases", [
      "name", "limit", "before", "opened", "expected_ids", "has_next", "error",
    ], names);
    cases(root, "session_view_cases", [
      "name", "prefix", "expected_state", "expected_turn_count", "error",
    ], names);
    cases(root, "timeline_cases", [
      "name", "after_position", "limit", "prefix", "expected_states", "truncated", "error",
    ], names);
    cases(root, "cursor_cases", ["name", "scenario", "error"], names);
    const failures = cases(root, "failure_cases", ["name", "status", "code"], names);
    expect(new Set(failures.map((item) => item.code))).toEqual(new Set([
      "invalid_request", "not_found", "read_bound_exceeded",
      "durability_unavailable", "corrupt_state",
    ]));
  });

  it("keeps the H3 fixture strict, complete and uniquely named", () => {
    const root = fixture("host-agent-activity-v1.json");
    exactFields(root, [
      "schema_version", "contract", "projection_cases", "timeline_cases",
      "reducer_cases", "bound_cases", "redaction_cases",
    ]);
    expect(root.schema_version).toBe(1);
    expect(root.contract).toBe("host-agent-activity-v1");
    const names = new Set<string>();
    const projections = cases(root, "projection_cases", [
      "name", "fact", "event", "state", "terminal", "safe_code",
    ], names);
    cases(root, "timeline_cases", ["name", "facts", "expected_states", "error"], names);
    cases(root, "reducer_cases", ["name", "from", "fact", "to", "valid"], names);
    cases(root, "bound_cases", ["name", "bound", "error"], names);
    cases(root, "redaction_cases", ["name", "canary", "must_be_absent"], names);
    expect(new Set(projections.map((item) => item.fact))).toEqual(new Set([
      "tool.preparation_rejected", "effect.prepared", "interaction.requested",
      "interaction.resolved", "interaction.cancelled", "effect.authorized",
      "effect.denied", "effect.started", "effect.receipt", "effect.completed",
      "effect.failed", "effect.uncertain", "effect.reconciled", "effect.observation",
    ]));
  });
});
