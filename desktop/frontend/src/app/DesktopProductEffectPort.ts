import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { Invoke } from "../ipc/host";
import {
  cancelProductTurn, continueProductApproval, continueProductTurn, createProductSession, getProductDefinitions,
  followProductLiveOutput, getProductEvents, getProductSessions, getProductTimeline, startProductTurn,
  type TurnCommandReceipt,
} from "../ipc/productHost";
import { TauriPreferenceBytesPort } from "../ipc/productStore";
import type { AppEffect, AppEffectPayload, AppViewState, PendingCommand } from "../state/controller";
import { mapDefinitions, mapHostEvent, mapSessions, mapTimeline } from "../state/hostProjection";
import {
  JsonPreferenceAdapter, type ClientPreferencesV1, type PreferenceBytesPort, type PreferenceLimits,
} from "../state/preferences";
import { ProductPortError, type ProductEffectPort } from "./ProductRuntime";

export const DESKTOP_PREFERENCE_LIMITS: PreferenceLimits = {
  max_document_bytes: 64 * 1024, max_drafts: 64, max_id_bytes: 128, max_draft_bytes: 4096,
};
const FOLLOW_POLL_MS = 100;
const MAX_TIMELINE_PAGES = 8;

/** Complete Desktop composition adapter behind the pure product controller. */
export class DesktopProductEffectPort implements ProductEffectPort {
  readonly #preferences: JsonPreferenceAdapter;

  public constructor(
    private readonly invoke: Invoke = tauriInvoke,
    preferencePort: PreferenceBytesPort = new TauriPreferenceBytesPort(invoke),
  ) {
    this.#preferences = new JsonPreferenceAdapter(preferencePort, DESKTOP_PREFERENCE_LIMITS);
  }

  public async *run(
    effect: AppEffect, snapshot: AppViewState, signal: AbortSignal,
  ): AsyncIterable<AppEffectPayload> {
    try {
      if (signal.aborted) return;
      switch (effect.kind) {
        case "load_preferences": {
          const loaded = await this.#preferences.load();
          yield { type: "preferences_loaded", selectedSessionId: loaded.preferences.selected_session_id,
            drafts: loaded.preferences.composer_drafts.map((draft) => ({ sessionId: draft.session_id, text: draft.text })),
            pending: loaded.pending };
          return;
        }
        case "save_preferences":
          await this.#preferences.save(preferences(snapshot));
          yield { type: "preferences_saved" }; return;
        case "load_definitions":
          yield mapDefinitions(await getProductDefinitions(this.invoke)); return;
        case "load_session_page":
          yield mapSessions(await getProductSessions(this.invoke)); return;
        case "load_timeline":
          yield mapTimeline(await this.#completeTimeline(required(effect.sessionId)), required(effect.sessionId));
          return;
        case "follow_events":
          yield* this.#follow(required(effect.sessionId), position(effect.afterPosition), signal);
          return;
        case "create_session": {
          const pending = await this.#persist(effect, snapshot);
          const receipt = await createProductSession(required(effect.commandId), required(effect.definitionId), this.invoke);
          await this.#clear(pending);
          yield { type: "command_succeeded", sessionId: receipt.session_id,
            committedPosition: receipt.committed_position };
          return;
        }
        case "start_turn": {
          const pending = await this.#persist(effect, snapshot);
          const receipt = await startProductTurn(required(effect.commandId), required(effect.sessionId),
            required(effect.text), this.invoke);
          await this.#clear(pending); yield commandResult(receipt); return;
        }
        case "cancel_turn": {
          const pending = await this.#persist(effect, snapshot);
          const receipt = await cancelProductTurn(required(effect.commandId), required(effect.sessionId),
            required(effect.turnId), positivePosition(effect.afterPosition), this.invoke);
          await this.#clear(pending); yield commandResult(receipt); return;
        }
        case "continue_turn": {
          const pending = await this.#persist(effect, snapshot);
          const common = [required(effect.commandId), required(effect.sessionId), required(effect.turnId),
            required(effect.suspensionId), positivePosition(effect.sessionVersion)] as const;
          const receipt = effect.continuationValueKind === "json_boolean"
            ? await continueProductApproval(...common, booleanText(effect.text), this.invoke)
            : effect.continuationValueKind === "string"
              ? await continueProductTurn(...common, required(effect.text), this.invoke)
              : protocol();
          await this.#clear(pending); yield commandResult(receipt); return;
        }
      }
    } catch (cause) {
      throw admitted(cause);
    }
  }

  async #completeTimeline(sessionId: string) {
    const items = []; let after = 0; let observed: number | undefined;
    for (let pageNumber = 0; pageNumber < MAX_TIMELINE_PAGES; pageNumber += 1) {
      const page = await getProductTimeline(sessionId, after, this.invoke);
      if (page.session_id !== sessionId || page.scanned_through_position < after ||
          (observed !== undefined && page.observed_max_position !== observed)) protocol();
      observed ??= page.observed_max_position; items.push(...page.items);
      if (!page.has_more) return { ...page, items };
      if (page.scanned_through_position === after) protocol();
      after = page.scanned_through_position;
    }
    protocol();
  }

  async *#follow(
    sessionId: string, afterPosition: number, signal: AbortSignal,
  ): AsyncIterable<AppEffectPayload> {
    let cursor = afterPosition;
    let liveQueue: AppEffectPayload[] = [];
    void followProductLiveOutput(sessionId, (output) => {
      if (signal.aborted) return;
      if (liveQueue.length >= 256) liveQueue = [{ type: "live_output", output: {
        ...output, kind: "preview_unavailable", text: undefined,
        throughSequence: undefined, phase: undefined, labelKey: undefined, reason: undefined,
      } }];
      else liveQueue.push({ type: "live_output", output });
    }, this.invoke).catch(() => undefined);
    while (!signal.aborted) {
      while (liveQueue.length) yield liveQueue.shift()!;
      const page = await getProductEvents(sessionId, cursor, this.invoke);
      while (liveQueue.length) yield liveQueue.shift()!;
      let previous = cursor;
      for (const event of page.events) {
        if (event.position <= previous || event.position > page.observed_max_position) protocol();
        previous = event.position;
        yield mapHostEvent(event, sessionId);
        if (signal.aborted) return;
      }
      cursor = page.scanned_through_position;
      if (cursor < previous) protocol();
      if (cursor >= page.observed_max_position) await abortableDelay(signal);
    }
  }

  async #persist(effect: AppEffect, snapshot: AppViewState): Promise<PendingCommand> {
    const pending = snapshot.pending.find((value) => value.commandId === effect.commandId);
    if (!pending || pending.requestDigest !== effect.requestDigest || pending.kind !== effect.kind) protocol();
    await this.#preferences.save(preferences(snapshot));
    await this.#preferences.savePending(pending); return pending;
  }

  async #clear(pending: PendingCommand): Promise<void> {
    const loaded = await this.#preferences.load();
    if (loaded.pending?.commandId !== pending.commandId || loaded.pending.requestDigest !== pending.requestDigest) protocol();
    await this.#preferences.savePending(undefined);
  }
}

function preferences(state: AppViewState): ClientPreferencesV1 {
  return { schema_version: 1, selected_session_id: state.selectedSessionId,
    session_rail: "expanded", activity_inspector: "closed", theme: "system",
    composer_drafts: state.drafts.map((draft) => ({ session_id: draft.sessionId, text: draft.text })) };
}
function commandResult(receipt: TurnCommandReceipt): AppEffectPayload {
  return { type: "command_succeeded", sessionId: receipt.session_id, turnId: receipt.turn_id,
    committedPosition: receipt.committed_position };
}
function required(value: string | undefined): string { if (!value) protocol(); return value; }
function booleanText(value: string | undefined): boolean {
  if (value === "true") return true;
  if (value === "false") return false;
  return protocol();
}
function position(value: number | undefined): number {
  if (!Number.isSafeInteger(value) || Number(value) < 0) protocol(); return Number(value);
}
function positivePosition(value: number | undefined): number {
  const parsed = position(value); if (parsed === 0) protocol(); return parsed;
}
function protocol(): never { throw new ProductPortError("protocol", "invalid_product_effect"); }
function admitted(cause: unknown): ProductPortError {
  if (cause instanceof ProductPortError) return cause;
  const code = typeof cause === "string" ? cause : cause instanceof Error ? cause.message : "";
  if (code === "not_configured") return new ProductPortError("configuration", code);
  if (["local_preference_invalid", "local_preference_unavailable", "invalid_local_preference"].includes(code)) {
    return new ProductPortError("local_preference", code);
  }
  if (["invalid_product_host_value", "invalid_host_value", "projection_failure"].includes(code)) {
    return new ProductPortError("protocol", code);
  }
  if (["host_failure", "execution_failure", "invalid_configuration"].includes(code)) {
    return new ProductPortError("host", code);
  }
  return new ProductPortError("transport", "desktop_ipc_unavailable");
}
async function abortableDelay(signal: AbortSignal): Promise<void> {
  if (signal.aborted) return;
  await new Promise<void>((resolve) => {
    const timer = setTimeout(done, FOLLOW_POLL_MS);
    signal.addEventListener("abort", done, { once: true });
    function done() { clearTimeout(timer); signal.removeEventListener("abort", done); resolve(); }
  });
}
