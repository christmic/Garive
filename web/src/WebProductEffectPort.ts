import {
  decodeHostDefinitionPage, decodeHostEvent, decodeHostSessionPage, decodeHostTimelinePage,
} from "../../desktop/frontend/src/ipc/host";
import type {
  AppEffect, AppEffectPayload, AppViewState, PendingCommand,
} from "../../desktop/frontend/src/state/controller";
import {
  mapDefinitions, mapHostEvent, mapSessions, mapTimeline,
} from "../../desktop/frontend/src/state/hostProjection";
import {
  JsonPreferenceAdapter, type ClientPreferencesV1, type PreferenceBytesPort,
} from "../../desktop/frontend/src/state/preferences";
import {
  ProductPortError, type ProductEffectPort,
} from "../../desktop/frontend/src/app/ProductRuntime";
import { decodeLiveOutput } from "../../desktop/frontend/src/state/liveOutput";
import { FetchHostClient, HostClientError } from "./host";

const LIMITS = { max_document_bytes: 64 * 1024, max_drafts: 64,
  max_id_bytes: 128, max_draft_bytes: 4096 } as const;
const ENCODER = new TextEncoder();

class BrowserPreferencePort implements PreferenceBytesPort {
  public async readPreferences() { return this.#read("garive.web.preferences.v1"); }
  public async writePreferences(value: Uint8Array) { this.#write("garive.web.preferences.v1", value); }
  public async readPendingCommand() { return this.#read("garive.web.pending.v1"); }
  public async writePendingCommand(value: Uint8Array | undefined) {
    if (value) this.#write("garive.web.pending.v1", value);
    else localStorage.removeItem("garive.web.pending.v1");
  }
  #read(key: string): Uint8Array | undefined {
    const value = localStorage.getItem(key); return value ? ENCODER.encode(value) : undefined;
  }
  #write(key: string, value: Uint8Array) {
    localStorage.setItem(key, new TextDecoder("utf-8", { fatal: true }).decode(value));
  }
}

/** Browser transport adapter for the same product controller and Work UI used by Desktop. */
export class WebProductEffectPort implements ProductEffectPort {
  readonly #preferences = new JsonPreferenceAdapter(new BrowserPreferencePort(), LIMITS);
  public constructor(private readonly host: FetchHostClient) {}

  public async *run(effect: AppEffect, snapshot: AppViewState, signal: AbortSignal): AsyncIterable<AppEffectPayload> {
    try {
      if (signal.aborted) return;
      switch (effect.kind) {
        case "load_preferences": {
          const loaded = await this.#preferences.load();
          yield { type: "preferences_loaded", selectedSessionId: loaded.preferences.selected_session_id,
            drafts: loaded.preferences.composer_drafts.map((item) => ({ sessionId: item.session_id, text: item.text })),
            pending: loaded.pending }; return;
        }
        case "save_preferences":
          await this.#preferences.save(preferences(snapshot));
          yield { type: "preferences_saved" }; return;
        case "load_definitions":
          yield mapDefinitions(decodeHostDefinitionPage(await this.host.readDefinitions())); return;
        case "load_session_page":
          yield mapSessions(decodeHostSessionPage(await this.host.readSessions())); return;
        case "load_timeline": {
          const sessionId = required(effect.sessionId);
          yield mapTimeline(await this.#completeTimeline(sessionId), sessionId); return;
        }
        case "follow_events": {
          const sessionId = required(effect.sessionId);
          yield* this.#follow(sessionId, position(effect.afterPosition), signal);
          return;
        }
        case "create_session": {
          const pending = await this.#persist(effect, snapshot);
          const receipt = await this.host.createSession(required(effect.commandId), required(effect.definitionId));
          await this.#clear(pending); yield { type: "command_succeeded", sessionId: receipt.session_id,
            committedPosition: receipt.committed_position }; return;
        }
        case "start_turn": {
          const pending = await this.#persist(effect, snapshot);
          const receipt = await this.host.startTurn(required(effect.commandId), required(effect.sessionId), required(effect.text));
          await this.#clear(pending); yield commandResult(receipt); return;
        }
        case "cancel_turn": {
          const pending = await this.#persist(effect, snapshot);
          const receipt = await this.host.cancelTurn(required(effect.commandId), required(effect.sessionId),
            required(effect.turnId), positive(effect.afterPosition));
          await this.#clear(pending); yield commandResult(receipt); return;
        }
        case "continue_turn": {
          const pending = await this.#persist(effect, snapshot);
          const input = effect.continuationValueKind === "json_boolean"
            ? booleanText(effect.text) : required(effect.text);
          const receipt = await this.host.continueTurnInput(required(effect.commandId), required(effect.sessionId),
            required(effect.turnId), required(effect.suspensionId), positive(effect.sessionVersion), input);
          await this.#clear(pending); yield commandResult(receipt); return;
        }
      }
    } catch (cause) {
      const error = admitted(cause);
      console.warn("[garive:web] product effect failed", effect.kind, error.kind, error.code);
      throw error;
    }
  }

  async #persist(effect: AppEffect, snapshot: AppViewState) {
    const pending = snapshot.pending.find((item) => item.commandId === effect.commandId);
    if (!pending || pending.requestDigest !== effect.requestDigest || pending.kind !== effect.kind) protocol();
    await this.#preferences.save(preferences(snapshot)); await this.#preferences.savePending(pending); return pending;
  }
  async #clear(pending: PendingCommand) {
    const loaded = await this.#preferences.load();
    if (loaded.pending?.commandId !== pending.commandId) protocol();
    await this.#preferences.savePending(undefined);
  }
  async #completeTimeline(sessionId: string) {
    const items = []; let after = 0; let observed: number | undefined;
    for (let pageNumber = 0; pageNumber < 8; pageNumber += 1) {
      const page = decodeHostTimelinePage(normalizeTimeline(
        await this.host.readTimeline(sessionId, after, 64)));
      if (page.session_id !== sessionId || page.scanned_through_position < after ||
          (observed !== undefined && page.observed_max_position !== observed)) protocol();
      observed ??= page.observed_max_position; items.push(...page.items);
      if (!page.has_more) return { ...page, items };
      if (page.scanned_through_position === after) protocol(); after = page.scanned_through_position;
    }
    return protocol();
  }
  async *#follow(sessionId: string, afterPosition: number, signal: AbortSignal): AsyncIterable<AppEffectPayload> {
    const durable = this.host.followEvents(sessionId, afterPosition, signal)[Symbol.asyncIterator]();
    const live = this.host.followLiveOutput(sessionId, signal)[Symbol.asyncIterator]();
    let durableNext = durable.next(); let liveNext: Promise<IteratorResult<unknown>> | undefined = live.next();
    while (!signal.aborted) {
      const candidates: Promise<{ source: "durable" | "live"; result: IteratorResult<unknown> }>[] = [
        durableNext.then((result) => ({ source: "durable" as const, result })),
      ];
      if (liveNext) candidates.push(liveNext.then((result) => ({ source: "live" as const, result }))
        .catch(() => ({ source: "live" as const, result: { done: true, value: undefined } })));
      const next = await Promise.race(candidates);
      if (next.source === "live") {
        if (next.result.done) { liveNext = undefined; continue; }
        yield { type: "live_output", output: decodeLiveOutput(next.result.value, sessionId) };
        liveNext = live.next();
      } else {
        if (next.result.done) return;
        yield mapHostEvent(decodeHostEvent(next.result.value), sessionId);
        durableNext = durable.next();
      }
    }
  }
}

function preferences(state: AppViewState): ClientPreferencesV1 {
  return { schema_version: 1, selected_session_id: state.selectedSessionId, session_rail: "expanded",
    activity_inspector: "closed", theme: "system",
    composer_drafts: state.drafts.map((item) => ({ session_id: item.sessionId, text: item.text })) };
}
function commandResult(receipt: { session_id: string; turn_id: string; committed_position: number }): AppEffectPayload {
  return { type: "command_succeeded", sessionId: receipt.session_id, turnId: receipt.turn_id,
    committedPosition: receipt.committed_position };
}
function normalizeTimeline(raw: Record<string, unknown>): Record<string, unknown> {
  const copy = structuredClone(raw); const items = Array.isArray(copy.items) ? copy.items : [];
  for (const item of items) {
    if (!item || typeof item !== "object") continue;
    const suspension = (item as Record<string, unknown>).suspension;
    if (!suspension || typeof suspension !== "object") continue;
    const value = suspension as Record<string, unknown>;
    for (const key of ["prompt_json", "response_schema_json"]) {
      if (typeof value[key] === "string") value[key] = [...ENCODER.encode(value[key])];
    }
  }
  return copy;
}
function required(value: string | undefined): string { if (!value) protocol(); return value; }
function position(value: number | undefined): number {
  if (!Number.isSafeInteger(value) || Number(value) < 0) protocol(); return Number(value);
}
function positive(value: number | undefined): number { const result = position(value); if (!result) protocol(); return result; }
function booleanText(value: string | undefined): boolean {
  if (value === "true") return true; if (value === "false") return false; return protocol();
}
function protocol(): never { throw new ProductPortError("protocol", "invalid_product_effect"); }
function admitted(cause: unknown): ProductPortError {
  if (cause instanceof ProductPortError) return cause;
  if (cause instanceof HostClientError) {
    const kind = cause.code === "invalid_configuration" ? "configuration"
      : cause.code === "transport_failure" || cause.code === "follow_deadline" ? "transport" : "host";
    return new ProductPortError(kind, cause.code);
  }
  const code = cause instanceof Error ? cause.message : "";
  if (code === "invalid_local_preference") return new ProductPortError("local_preference", code);
  return new ProductPortError("protocol", "web_product_port_failure");
}
