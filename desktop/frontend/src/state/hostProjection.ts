import type {
  HostActivity, HostDefinitionPage, HostEvent, HostSessionPage, HostSuspension, HostTimelinePage,
} from "../ipc/host";
import type {
  ActivityItem, AppEffectPayload, AppError, DefinitionItem, SessionItem, SuspensionItem, TimelineItem,
} from "./controller";

/** Content-free protocol mapping failure for the product composition root. */
export class ProductHostMappingError extends Error {
  public readonly error: AppError = { kind: "protocol", code: "invalid_host_value" };
  public constructor() { super("protocol:invalid_host_value"); }
}

/** Maps one validated H2 definition page into product values. */
export function mapDefinitions(page: HostDefinitionPage): Extract<AppEffectPayload, { type: "definitions_loaded" }> {
  version(page.api_version); const definitions: DefinitionItem[] = page.definitions.map((value) => {
    version(value.api_version); required(value.definition_id); required(value.definition_revision);
    if (!sortedUnique(value.capabilities) || value.capabilities.some((item) => !item)) invalid();
    return { definitionId: value.definition_id, definitionRevision: value.definition_revision,
      capabilities: value.capabilities };
  });
  if (!unique(definitions.map((item) => item.definitionId))) invalid();
  return { type: "definitions_loaded", definitions };
}

/** Maps one H2 Session page without inventing titles or execution state. */
export function mapSessions(page: HostSessionPage): Extract<AppEffectPayload, { type: "session_page_loaded" }> {
  version(page.api_version); const sessions: SessionItem[] = page.sessions.map((value) => {
    version(value.api_version); required(value.session_id); required(value.agent_instance_id);
    required(value.definition_id); required(value.definition_revision); required(value.opened_at);
    if (value.latest_position <= 0 || value.turn_count < 0 ||
        (value.latest_turn_id === undefined) !== (value.latest_turn_state === undefined)) invalid();
    return { sessionId: value.session_id, agentInstanceId: value.agent_instance_id,
      definitionId: value.definition_id, definitionRevision: value.definition_revision,
      openedAt: value.opened_at, latestPosition: value.latest_position,
      latestTurnId: value.latest_turn_id, state: value.latest_turn_state, turnCount: value.turn_count };
  });
  if (!unique(sessions.map((item) => item.sessionId))) invalid();
  return { type: "session_page_loaded", sessions };
}

/** Maps one complete H2/H3 timeline page for the expected Session. */
export function mapTimeline(page: HostTimelinePage, expectedSessionId: string): Extract<AppEffectPayload, { type: "timeline_loaded" }> {
  version(page.api_version); required(expectedSessionId);
  if (page.session_id !== expectedSessionId || page.observed_max_position < page.scanned_through_position) invalid();
  const items: TimelineItem[] = page.items.map((value) => {
    required(value.turn_id); required(value.state);
    if (value.started_position <= 0 || value.latest_position < value.started_position) invalid();
    const activities = value.activities.map((activity) => mapActivity(activity, value.turn_id));
    return { turnId: value.turn_id, startedPosition: value.started_position, latestPosition: value.latest_position,
      state: value.state, userText: value.user_text, completionText: value.completion_text,
      suspension: value.suspension && mapSuspension(value.suspension), contentTruncated: value.content_truncated,
      activities };
  });
  if (!unique(items.map((item) => item.turnId)) || items.some((item, index) => index > 0 &&
      items[index - 1]!.latestPosition > item.latestPosition)) invalid();
  return { type: "timeline_loaded", items, cursor: page.scanned_through_position,
    activities: items.flatMap((item) => item.activities ?? []) };
}

/** Maps one H1/H3 event while preserving unknown names as neutral controller input. */
export function mapHostEvent(value: HostEvent, expectedSessionId: string): Extract<AppEffectPayload, { type: "host_event" }> {
  version(value.api_version); required(expectedSessionId); required(value.event);
  if (value.session_id !== expectedSessionId || value.position <= 0) invalid();
  const turnId = value.turn_id || undefined;
  return { type: "host_event", event: value.event, position: value.position, turnId,
    text: value.text || undefined,
    activity: value.activity && mapActivity(value.activity, turnId) };
}

function mapActivity(value: HostActivity, turnId?: string): ActivityItem {
  version(value.api_version); required(value.activity_id); required(value.kind); required(value.label_key); required(value.state);
  if (value.source_position <= 0 || value.safe_code === "") invalid();
  return { activityId: value.activity_id, kind: value.kind, labelKey: value.label_key,
    state: value.state, turnId, position: value.source_position, terminal: value.terminal,
    safeCode: value.safe_code, neutral: false };
}

function mapSuspension(value: HostSuspension): SuspensionItem {
  required(value.suspension_id); required(value.kind); required(value.prompt_schema); required(value.prompt_digest);
  if (value.session_version <= 0 || value.prompt_schema !== "garive.public-suspension-prompt.v1" ||
      !HEX.test(value.prompt_digest) || (value.response_schema_json === undefined) !==
      (value.response_schema_digest === undefined) ||
      (value.response_schema_digest !== undefined && !HEX.test(value.response_schema_digest))) invalid();
  let prompt: Record<string, unknown>;
  try { prompt = object(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(Uint8Array.from(value.prompt_json)))); }
  catch { invalid(); }
  if (Object.keys(prompt).some((key) => !PROMPT_KEYS.has(key)) || prompt.schema_version !== 1) invalid();
  return { suspensionId: value.suspension_id, sessionVersion: value.session_version, kind: value.kind,
    titleKey: promptText(prompt.title_key), messageText: optionalPromptText(prompt.message_text),
    actionLabelKey: promptText(prompt.action_label_key), cancelLabelKey: optionalPromptText(prompt.cancel_label_key),
    promptDigest: value.prompt_digest, responseSchemaDigest: value.response_schema_digest };
}

function version(value: string): void { if (value !== "v1") invalid(); }
function required(value: string): void { if (!value) invalid(); }
function sortedUnique(values: readonly string[]): boolean { return unique(values) && values.every((item, index) => index === 0 || values[index - 1]! < item); }
function unique(values: readonly string[]): boolean { return new Set(values).size === values.length; }
function promptText(value: unknown): string { if (typeof value !== "string" || !value) invalid(); return value; }
function optionalPromptText(value: unknown): string | undefined { return value === undefined ? undefined : promptText(value); }
function object(value: unknown): Record<string, unknown> { if (typeof value !== "object" || value === null || Array.isArray(value)) invalid(); return value as Record<string, unknown>; }
function invalid(): never { throw new ProductHostMappingError(); }
const HEX = /^[0-9a-f]{64}$/;
const PROMPT_KEYS = new Set(["schema_version", "title_key", "message_text", "action_label_key", "cancel_label_key"]);
