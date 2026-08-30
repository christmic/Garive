import type { PendingCommand } from "./controller";

export type SessionRail = "expanded" | "collapsed";
export type InspectorState = "open" | "closed";
export type Theme = "system" | "light" | "dark";

export interface ClientPreferencesV1 {
  readonly schema_version: 1;
  readonly selected_session_id?: string;
  readonly session_rail: SessionRail;
  readonly activity_inspector: InspectorState;
  readonly theme: Theme;
  readonly composer_drafts: readonly { readonly session_id: string; readonly text: string }[];
}

export interface PreferenceLimits {
  readonly max_document_bytes: number; readonly max_drafts: number;
  readonly max_id_bytes: number; readonly max_draft_bytes: number;
}

export interface PreferenceBytesPort {
  readPreferences(): Promise<Uint8Array | undefined>;
  writePreferences(value: Uint8Array): Promise<void>;
  readPendingCommand(): Promise<Uint8Array | undefined>;
  writePendingCommand(value: Uint8Array | undefined): Promise<void>;
}

export interface PreferenceLoadResult {
  readonly preferences: ClientPreferencesV1; readonly reset: boolean;
  readonly pending?: PendingCommand;
}

const DEFAULTS: ClientPreferencesV1 = {
  schema_version: 1, session_rail: "expanded", activity_inspector: "closed",
  theme: "system", composer_drafts: [],
};
const PREF_KEYS = new Set(["schema_version", "selected_session_id", "session_rail",
  "activity_inspector", "theme", "composer_drafts"]);
const DRAFT_KEYS = new Set(["session_id", "text"]);
const PENDING_KEYS = new Set(["schema_version", "kind", "command_id", "semantic_request_digest",
  "session_id", "turn_id", "issued_generation", "status", "definition_id", "after_position",
  "suspension_id", "session_version", "response_schema_digest", "continuation_value_kind"]);

export class JsonPreferenceAdapter {
  public constructor(private readonly port: PreferenceBytesPort, private readonly limits: PreferenceLimits) {
    if (!validLimits(limits)) throw new Error("invalid_preference_limits");
  }

  public async load(): Promise<PreferenceLoadResult> {
    const [rawPreferences, rawPending] = await Promise.all([
      this.port.readPreferences(), this.port.readPendingCommand(),
    ]);
    let preferences = DEFAULTS; let reset = false;
    if (rawPreferences) {
      try { preferences = decodePreferences(rawPreferences, this.limits); }
      catch { preferences = DEFAULTS; reset = true; }
    }
    let pending: PendingCommand | undefined;
    if (rawPending) {
      try { pending = decodePendingCommand(rawPending, this.limits); }
      catch { reset = true; await this.port.writePendingCommand(undefined); }
    }
    return { preferences, reset, pending };
  }

  public async save(preferences: ClientPreferencesV1): Promise<void> {
    const encoded = encodePreferences(preferences, this.limits);
    await this.port.writePreferences(encoded);
  }

  public async savePending(command: PendingCommand | undefined): Promise<void> {
    await this.port.writePendingCommand(command ? encodePendingCommand(command, this.limits) : undefined);
  }
}

export function decodePreferences(bytes: Uint8Array, limits: PreferenceLimits): ClientPreferencesV1 {
  if (!validLimits(limits) || bytes.byteLength > limits.max_document_bytes) fail();
  const value = parseObject(bytes); exactKeys(value, PREF_KEYS);
  if (value.schema_version !== 1 || !oneOf(value.session_rail, ["expanded", "collapsed"]) ||
      !oneOf(value.activity_inspector, ["open", "closed"]) || !oneOf(value.theme, ["system", "light", "dark"]) ||
      !Array.isArray(value.composer_drafts) || value.composer_drafts.length > limits.max_drafts) fail();
  const selected = optionalId(value.selected_session_id, limits);
  const seen = new Set<string>();
  const drafts = value.composer_drafts.map((raw) => {
    const draft = object(raw); exactKeys(draft, DRAFT_KEYS);
    const session = requiredId(draft.session_id, limits);
    if (seen.has(session) || typeof draft.text !== "string" || utf8(draft.text) > limits.max_draft_bytes) fail();
    seen.add(session); return { session_id: session, text: draft.text };
  });
  return { schema_version: 1, selected_session_id: selected,
    session_rail: value.session_rail as SessionRail,
    activity_inspector: value.activity_inspector as InspectorState,
    theme: value.theme as Theme, composer_drafts: drafts };
}

export function encodePreferences(value: ClientPreferencesV1, limits: PreferenceLimits): Uint8Array {
  const normalized = decodePreferences(new TextEncoder().encode(JSON.stringify(value)), limits);
  const bytes = new TextEncoder().encode(JSON.stringify(normalized));
  if (bytes.byteLength > limits.max_document_bytes) fail(); return bytes;
}

export function decodePendingCommand(bytes: Uint8Array, limits: PreferenceLimits): PendingCommand {
  if (!validLimits(limits) || bytes.byteLength > limits.max_document_bytes) fail();
  const value = parseObject(bytes); exactKeys(value, PENDING_KEYS);
  if (value.schema_version !== 1 || !oneOf(value.kind,
      ["create_session", "start_turn", "cancel_turn", "continue_turn"]) ||
      !oneOf(value.status, ["pending", "unknown"]) ||
      !Number.isSafeInteger(value.issued_generation) || Number(value.issued_generation) < 0) fail();
  const digest = value.semantic_request_digest;
  if (typeof digest !== "string" || !/^[0-9a-f]{64}$/.test(digest)) fail();
  const pending = { kind: value.kind as PendingCommand["kind"], commandId: requiredId(value.command_id, limits),
    requestDigest: digest, generation: Number(value.issued_generation),
    sessionId: optionalId(value.session_id, limits), turnId: optionalId(value.turn_id, limits),
    definitionId: optionalId(value.definition_id, limits),
    afterPosition: optionalNonnegative(value.after_position), suspensionId: optionalId(value.suspension_id, limits),
    sessionVersion: optionalPositive(value.session_version),
    responseSchemaDigest: optionalDigest(value.response_schema_digest),
    continuationValueKind: value.continuation_value_kind as PendingCommand["continuationValueKind"],
    status: value.status as PendingCommand["status"] };
  const continuation = Boolean(pending.suspensionId && pending.sessionVersion && pending.responseSchemaDigest &&
    oneOf(pending.continuationValueKind, ["string", "json_boolean"]));
  const noContinuation = pending.suspensionId === undefined && pending.sessionVersion === undefined &&
    pending.responseSchemaDigest === undefined && pending.continuationValueKind === undefined;
  const validShape = pending.kind === "create_session"
    ? Boolean(pending.definitionId && !pending.sessionId && !pending.turnId && pending.afterPosition === undefined && noContinuation)
    : pending.kind === "start_turn"
      ? Boolean(!pending.definitionId && pending.sessionId && !pending.turnId && pending.afterPosition === undefined && noContinuation)
      : pending.kind === "cancel_turn"
        ? Boolean(!pending.definitionId && pending.sessionId && pending.turnId && pending.afterPosition !== undefined && noContinuation)
        : Boolean(!pending.definitionId && pending.sessionId && pending.turnId && pending.afterPosition === undefined && continuation);
  if (!validShape) fail();
  return pending;
}

export function encodePendingCommand(value: PendingCommand, limits: PreferenceLimits): Uint8Array {
  const wire = { schema_version: 1, kind: value.kind, command_id: value.commandId,
    semantic_request_digest: value.requestDigest, session_id: value.sessionId, turn_id: value.turnId,
    issued_generation: value.generation, status: value.status, definition_id: value.definitionId,
    after_position: value.afterPosition, suspension_id: value.suspensionId,
    session_version: value.sessionVersion, response_schema_digest: value.responseSchemaDigest,
    continuation_value_kind: value.continuationValueKind };
  const bytes = new TextEncoder().encode(JSON.stringify(wire));
  decodePendingCommand(bytes, limits); return bytes;
}

function parseObject(bytes: Uint8Array): Record<string, unknown> {
  try { return object(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes))); }
  catch { fail(); }
}
function object(value: unknown): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) fail();
  return value as Record<string, unknown>;
}
function exactKeys(value: Record<string, unknown>, allowed: ReadonlySet<string>): void {
  if (Object.keys(value).some((key) => !allowed.has(key))) fail();
}
function requiredId(value: unknown, limits: PreferenceLimits): string {
  if (typeof value !== "string" || !value || utf8(value) > limits.max_id_bytes || /[\u0000-\u001f\u007f]/.test(value)) fail();
  return value;
}
function optionalId(value: unknown, limits: PreferenceLimits): string | undefined {
  return value === undefined ? undefined : requiredId(value, limits);
}
function optionalPositive(value: unknown): number | undefined {
  if (value === undefined) return undefined;
  if (!Number.isSafeInteger(value) || Number(value) <= 0) fail();
  return Number(value);
}
function optionalNonnegative(value: unknown): number | undefined {
  if (value === undefined) return undefined;
  if (!Number.isSafeInteger(value) || Number(value) < 0) fail();
  return Number(value);
}
function optionalDigest(value: unknown): string | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "string" || !/^[0-9a-f]{64}$/.test(value)) fail();
  return value;
}
function oneOf(value: unknown, values: readonly string[]): value is string { return typeof value === "string" && values.includes(value); }
function validLimits(value: PreferenceLimits): boolean { return value.max_document_bytes > 0 && value.max_drafts > 0 && value.max_id_bytes > 0 && value.max_draft_bytes > 0; }
function utf8(value: string): number { return new TextEncoder().encode(value).length; }
function fail(): never { throw new Error("invalid_local_preference"); }
