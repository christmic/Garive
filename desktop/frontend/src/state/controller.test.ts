import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import {
  initialAppViewState, reduceApp, type ActivityItem, type AppEffect,
  type AppEffectPayload, type AppIntent, type AppViewState, type PendingCommand,
  type TimelineItem,
} from "./controller";
import { JsonPreferenceAdapter, type PreferenceBytesPort } from "./preferences";

const FIXTURE = object(JSON.parse(readFileSync(fileURLToPath(new URL(
  "../../../../spec/fixtures/host/client-product-experience-v1.json", import.meta.url,
)), "utf8")));
const FAMILIES = ["bootstrap_cases", "navigation_cases", "command_cases", "follow_cases",
  "suspension_cases", "activity_cases", "preference_cases", "failure_cases"] as const;
const limits = object(FIXTURE.limits);

describe("A-UX1 shared controller", () => {
  it("consumes every ordered state-machine scenario", () => {
    validateFixture();
    for (const family of FAMILIES.filter((value) => value !== "preference_cases")) {
      for (const raw of array(FIXTURE[family])) runControllerCase(object(raw));
    }
  });

  it("strictly resets every invalid preference document", async () => {
    for (const raw of array(FIXTURE.preference_cases)) {
      const test = object(raw); const port = new MemoryPort(encode(test.document));
      const adapter = new JsonPreferenceAdapter(port, preferenceLimits());
      const loaded = await adapter.load();
      expect(loaded.reset, text(test.name)).toBe(test.expected_reset);
      expect(loaded.preferences.composer_drafts).toHaveLength(number(test.expected_draft_count));
      if (!loaded.reset) await expect(adapter.save(loaded.preferences)).resolves.toBeUndefined();
    }
  });

  it("preserves the complete safe error vocabulary without raw failures", () => {
    const actual = array(FIXTURE.failure_cases).map((raw) => {
      const test = object(raw); const error = object(test.error);
      expect(Object.keys(error).sort()).toEqual(["code", "kind"]);
      expect(error.kind).toBe(test.expected_public_kind);
      expect(JSON.stringify(error)).not.toContain("raw_body");
      return error.kind;
    });
    expect(actual).toEqual(["configuration", "validation", "command_unknown", "host",
      "transport", "protocol", "local_preference"]);
  });

  it("rejects fixture root, case, duplicate, and omission drift", () => {
    expect(() => validateFixture({ ...FIXTURE, unknown: true })).toThrow();
    const unknownCase = structuredClone(FIXTURE);
    object(array(unknownCase.bootstrap_cases)[0]).unknown = true;
    expect(() => validateFixture(unknownCase)).toThrow();
    const omitted = structuredClone(FIXTURE);
    delete object(array(omitted.bootstrap_cases)[0]).expected_state;
    expect(() => validateFixture(omitted)).toThrow();
    const duplicate = structuredClone(FIXTURE);
    object(array(duplicate.preference_cases)[1]).name = object(array(duplicate.preference_cases)[0]).name;
    expect(() => validateFixture(duplicate)).toThrow();
  });
});

function runControllerCase(test: Record<string, unknown>): void {
  let state = decodeState(object(test.initial_state));
  const emitted: AppEffect[] = []; const aliases = new Map<string, AppEffect>();
  for (const rawStep of array(test.steps)) {
    const step = object(rawStep); let reduction;
    if (step.intent !== undefined) {
      reduction = reduceApp(state, decodeIntent(object(step.intent)), controllerLimits());
    } else if (step.seed_effect !== undefined) {
      const seed = object(step.seed_effect);
      const effect: AppEffect = { effectId: `effect-${state.nextEffect}`, generation: state.generation,
        kind: text(seed.kind) as AppEffect["kind"], sessionId: optionalText(seed.session_id),
        afterPosition: optionalNumber(seed.after_position) };
      state = { ...state, nextEffect: state.nextEffect + 1, outstanding: [...state.outstanding, effect] };
      emitted.push(effect); continue;
    } else {
      const resolve = object(step.resolve); const effect = resolve.alias !== undefined
        ? aliases.get(text(resolve.alias)) : state.outstanding.find((item) => item.kind === text(resolve.effect_kind));
      if (!effect) throw new Error(`missing effect in ${text(test.name)}`);
      reduction = reduceApp(state, { type: "effect_result", effectId: effect.effectId,
        generation: effect.generation, sessionId: effect.sessionId, requestDigest: effect.requestDigest,
        result: decodeResult(object(resolve.result)) }, controllerLimits());
    }
    state = reduction.state; emitted.push(...reduction.effects);
    if (step.capture !== undefined) {
      const capture = object(step.capture); const effect = reduction.effects.find((item) => item.kind === text(capture.effect_kind));
      if (!effect) throw new Error("capture effect missing"); aliases.set(text(capture.as), effect);
    }
  }
  expect(emitted.map((item) => item.kind), text(test.name)).toEqual(array(test.expected_effects));
  expect(project(state), text(test.name)).toEqual(test.expected_state);
  if (test.expected_retried_command_id !== undefined) {
    const starts = emitted.filter((item) => item.kind === "start_turn");
    expect(starts).toHaveLength(2);
    expect(starts[1]?.commandId).toBe(test.expected_retried_command_id);
    expect(starts[1]?.requestDigest).toBe(test.expected_retried_request_digest);
  }
  if (test.expected_effect_binding !== undefined) {
    const effect = emitted.find((item) => item.kind === "continue_turn");
    const binding = object(test.expected_effect_binding);
    expect({ suspension_id: effect?.suspensionId, session_version: effect?.sessionVersion,
      response_schema_digest: effect?.responseSchemaDigest }).toEqual(binding);
  }
  if (test.expected_cancel_after_position !== undefined) {
    const effect = emitted.find((item) => item.kind === "cancel_turn");
    expect(effect?.afterPosition).toBe(test.expected_cancel_after_position);
  }
}

function decodeState(raw: Record<string, unknown>): AppViewState {
  const base = initialAppViewState(text(raw.configuration) as AppViewState["configuration"]);
  return { ...base, shell: text(raw.shell) as AppViewState["shell"], generation: number(raw.generation),
    definitions: array(raw.definition_ids).map((id) => ({ definitionId: text(id), definitionRevision: "fixture-revision", capabilities: [] })),
    sessions: array(raw.session_ids).map((id) => ({ sessionId: text(id) })),
    selectedSessionId: nullableText(raw.selected_session_id), timelineSessionId: nullableText(raw.selected_session_id),
    timeline: array(raw.timeline).map(decodeTimeline), cursor: number(raw.cursor),
    drafts: array(raw.drafts).map((value) => { const item = object(value); return { sessionId: text(item.session_id), text: text(item.text) }; }),
    execution: text(raw.execution) as AppViewState["execution"], pending: array(raw.pending).map(decodePending),
    activities: array(raw.activities).map(decodeActivity),
    notice: raw.notice === null ? undefined : object(raw.notice) as unknown as AppViewState["notice"] };
}

function decodeIntent(raw: Record<string, unknown>): AppIntent {
  const type = text(raw.type);
  switch (type) {
    case "boot": return { type };
    case "select_session": return { type, sessionId: text(raw.session_id) };
    case "edit_draft": return { type, sessionId: text(raw.session_id), text: text(raw.text) };
    case "create_session": return { type, definitionId: text(raw.definition_id), commandId: text(raw.command_id), requestDigest: text(raw.request_digest) };
    case "submit_draft": return { type, sessionId: text(raw.session_id), commandId: text(raw.command_id), requestDigest: text(raw.request_digest) };
    case "retry_pending": return { type, sessionId: optionalText(raw.session_id) };
    case "reconnect": return { type, sessionId: text(raw.session_id) };
    case "cancel_turn": return { type, sessionId: text(raw.session_id), turnId: text(raw.turn_id),
      commandId: text(raw.command_id), requestDigest: text(raw.request_digest) };
    case "continue_suspension": return { type, sessionId: text(raw.session_id), turnId: text(raw.turn_id), input: text(raw.input),
      commandId: text(raw.command_id), requestDigest: text(raw.request_digest) };
    default: throw new Error(`unknown fixture intent ${type}`);
  }
}

function decodeResult(raw: Record<string, unknown>): AppEffectPayload {
  const type = text(raw.type);
  switch (type) {
    case "preferences_loaded": return { type, selectedSessionId: nullableText(raw.selected_session_id), drafts: array(raw.drafts).map((value) => {
      const item = object(value); return { sessionId: text(item.session_id), text: text(item.text) };
    }) };
    case "definitions_loaded": return { type, definitions: array(raw.definition_ids).map((id) => ({
      definitionId: text(id), definitionRevision: "fixture-revision", capabilities: [],
    })) };
    case "session_page_loaded": return { type, sessions: array(raw.sessions).map((value) => {
      const item = object(value); return { sessionId: text(item.session_id) };
    }) };
    case "timeline_loaded": return { type, items: array(raw.items).map(decodeTimeline), cursor: number(raw.cursor),
      activities: array(raw.activities).map(decodeActivity) };
    case "command_succeeded": return { type, sessionId: text(raw.session_id), turnId: optionalText(raw.turn_id), committedPosition: number(raw.committed_position) };
    case "host_event": return { type, event: text(raw.event), position: number(raw.position), turnId: optionalText(raw.turn_id),
      text: optionalText(raw.text),
      activity: raw.activity === undefined ? undefined : decodeActivity(object(raw.activity)) };
    case "event_stream_ended": return { type };
    case "failed": return { type, error: object(raw.error) as unknown as Extract<AppEffectPayload, {type:"failed"}>["error"] };
    default: throw new Error(`unknown fixture result ${type}`);
  }
}

function project(state: AppViewState): unknown {
  return { configuration: state.configuration, shell: state.shell, generation: state.generation,
    definition_ids: state.definitions.map((item) => item.definitionId), session_ids: state.sessions.map((item) => item.sessionId),
    selected_session_id: state.selectedSessionId ?? null,
    timeline: state.timeline.map((item) => ({ turn_id: item.turnId, state: item.state,
      latest_position: item.latestPosition, ...(item.suspension ? { suspension_id: item.suspension.suspensionId,
        session_version: item.suspension.sessionVersion, response_schema_digest: item.suspension.responseSchemaDigest } : {}) })),
    cursor: state.cursor, drafts: state.drafts.map((item) => ({ session_id: item.sessionId, text: item.text })),
    execution: state.execution, pending: state.pending.map((item) => ({ kind: item.kind,
      command_id: item.commandId, request_digest: item.requestDigest, session_id: item.sessionId ?? null,
      turn_id: item.turnId ?? null, status: item.status })),
    activities: state.activities.map((item) => ({ activity_id: item.activityId, kind: item.kind,
      state: item.state, ...(item.turnId ? { turn_id: item.turnId } : {}), position: item.position, neutral: item.neutral })),
    notice: state.notice ?? null };
}

function decodeTimeline(value: unknown): TimelineItem { const item = object(value); return {
  turnId: text(item.turn_id), state: text(item.state), latestPosition: number(item.latest_position),
  suspension: item.suspension_id === undefined ? undefined : {
    suspensionId: text(item.suspension_id), sessionVersion: number(item.session_version), kind: "fixture",
    responseSchemaDigest: optionalText(item.response_schema_digest),
  },
}; }
function decodeActivity(value: unknown): ActivityItem { const item = object(value); return {
  activityId: text(item.activity_id), kind: text(item.kind), state: text(item.state), turnId: optionalText(item.turn_id),
  position: number(item.position), neutral: item.neutral === undefined ? false : Boolean(item.neutral),
}; }
function decodePending(value: unknown): PendingCommand { const item = object(value); return {
  kind: text(item.kind) as PendingCommand["kind"], commandId: text(item.command_id), requestDigest: text(item.request_digest),
  generation: 0, sessionId: nullableText(item.session_id), turnId: nullableText(item.turn_id), status: text(item.status) as PendingCommand["status"],
}; }

function validateFixture(value: Record<string, unknown> = FIXTURE): void {
  expect(value.schema_version).toBe(1); expect(value.contract).toBe("client-product-experience-v1");
  expect(Object.keys(value).sort()).toEqual(["activity_cases","bootstrap_cases","command_cases","contract","failure_cases",
    "follow_cases","limits","navigation_cases","preference_cases","schema_version","suspension_cases"]);
  for (const family of FAMILIES) {
    const cases = array(value[family]); const names = cases.map((raw) => text(object(raw).name));
    expect(names.length, family).toBeGreaterThan(0); expect(new Set(names).size, family).toBe(names.length);
    for (const raw of cases) {
      const item = object(raw); const keys = Object.keys(item);
      const required = family === "preference_cases"
        ? ["document", "expected_draft_count", "expected_reset", "name"]
        : ["expected_effects", "expected_state", "initial_state", "name", "steps"];
      const allowed = family === "command_cases" ? [...required, "expected_retried_command_id", "expected_retried_request_digest", "expected_cancel_after_position"]
        : family === "suspension_cases" ? [...required, "expected_effect_binding"]
          : family === "failure_cases" ? [...required, "error", "expected_public_kind"] : required;
      for (const key of required) expect(keys, `${family}.${text(item.name)} missing ${key}`).toContain(key);
      expect(keys.every((key) => allowed.includes(key)), `${family}.${text(item.name)} unknown key`).toBe(true);
    }
  }
}
function controllerLimits() { return { maxDraftBytes: number(limits.max_draft_bytes), maxActivities: number(limits.max_activities) }; }
function preferenceLimits() { return { max_document_bytes: number(limits.max_preference_bytes), max_drafts: number(limits.max_drafts),
  max_id_bytes: number(limits.max_id_bytes), max_draft_bytes: number(limits.max_draft_bytes) }; }
class MemoryPort implements PreferenceBytesPort {
  public pending?: Uint8Array; public constructor(public preferences?: Uint8Array) {}
  public async readPreferences() { return this.preferences; } public async writePreferences(value: Uint8Array) { this.preferences = value; }
  public async readPendingCommand() { return this.pending; } public async writePendingCommand(value: Uint8Array | undefined) { this.pending = value; }
}
function encode(value: unknown): Uint8Array { return new TextEncoder().encode(JSON.stringify(value)); }
function object(value: unknown): Record<string, unknown> { if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error("invalid fixture"); return value as Record<string, unknown>; }
function array(value: unknown): unknown[] { if (!Array.isArray(value)) throw new Error("invalid fixture"); return value; }
function text(value: unknown): string { if (typeof value !== "string") throw new Error("invalid fixture"); return value; }
function number(value: unknown): number { if (!Number.isSafeInteger(value)) throw new Error("invalid fixture"); return value as number; }
function optionalText(value: unknown): string | undefined { return value === undefined ? undefined : text(value); }
function nullableText(value: unknown): string | undefined { return value === null || value === undefined ? undefined : text(value); }
function optionalNumber(value: unknown): number | undefined { return value === undefined ? undefined : number(value); }
