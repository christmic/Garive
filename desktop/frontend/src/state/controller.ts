export type AppErrorKind = "configuration" | "validation" | "command_unknown" |
  "host" | "transport" | "protocol" | "local_preference";
export type ShellState = "booting" | "not_configured" | "loading_navigation" | "ready" | "unavailable";
export type ExecutionState = "idle" | "submitting" | "following" | "cancelling" |
  "disconnected" | "reconnecting" | "suspended" | "continuing";
export type PendingStatus = "pending" | "unknown";
export type ContinuationValueKind = "string" | "json_boolean";

export interface AppError { readonly kind: AppErrorKind; readonly code: string }
export interface Draft { readonly sessionId: string; readonly text: string }
export interface DefinitionItem {
  readonly definitionId: string; readonly definitionRevision: string;
  readonly capabilities: readonly string[];
}
export interface SessionItem {
  readonly sessionId: string; readonly agentInstanceId?: string; readonly definitionId?: string;
  readonly definitionRevision?: string; readonly openedAt?: string; readonly latestPosition?: number;
  readonly latestTurnId?: string; readonly state?: string; readonly turnCount?: number;
}
export interface SuspensionItem {
  readonly suspensionId: string; readonly sessionVersion: number; readonly kind: string;
  readonly titleKey?: string; readonly messageText?: string; readonly actionLabelKey?: string;
  readonly cancelLabelKey?: string; readonly promptDigest?: string;
  readonly responseSchemaDigest?: string;
}
export interface TimelineItem {
  readonly turnId: string; readonly startedPosition?: number; readonly state: string;
  readonly latestPosition: number; readonly userText?: string; readonly completionText?: string;
  readonly suspension?: SuspensionItem; readonly contentTruncated?: boolean;
  readonly activities?: readonly ActivityItem[];
}
export interface ActivityItem {
  readonly activityId: string; readonly kind: string; readonly labelKey?: string;
  readonly state: string; readonly turnId?: string; readonly position: number;
  readonly terminal?: boolean; readonly safeCode?: string; readonly neutral: boolean;
}
export interface PendingCommand {
  readonly kind: "create_session" | "start_turn" | "cancel_turn" | "continue_turn";
  readonly commandId: string; readonly requestDigest: string; readonly generation: number;
  readonly sessionId?: string; readonly turnId?: string; readonly status: PendingStatus;
}
export type EffectKind = "load_preferences" | "save_preferences" | "load_definitions" |
  "load_session_page" | "load_timeline" | "follow_events" | "create_session" |
  "start_turn" | "cancel_turn" | "continue_turn";
export interface AppEffect {
  readonly effectId: string; readonly kind: EffectKind; readonly generation: number;
  readonly sessionId?: string; readonly commandId?: string; readonly requestDigest?: string;
  readonly afterPosition?: number; readonly definitionId?: string; readonly text?: string;
  readonly turnId?: string; readonly suspensionId?: string; readonly sessionVersion?: number;
  readonly responseSchemaDigest?: string; readonly continuationValueKind?: ContinuationValueKind;
}
export interface AppViewState {
  readonly configuration: "configured" | "not_configured";
  readonly shell: ShellState; readonly generation: number; readonly nextEffect: number;
  readonly definitions: readonly DefinitionItem[]; readonly sessions: readonly SessionItem[];
  readonly selectedSessionId?: string; readonly timelineSessionId?: string;
  readonly timeline: readonly TimelineItem[]; readonly cursor: number;
  readonly drafts: readonly Draft[]; readonly execution: ExecutionState;
  readonly pending: readonly PendingCommand[]; readonly activities: readonly ActivityItem[];
  readonly outstanding: readonly AppEffect[]; readonly preferenceDirty: boolean; readonly notice?: AppError;
}

export type AppIntent =
  | { readonly type: "boot" }
  | { readonly type: "select_session"; readonly sessionId: string }
  | { readonly type: "edit_draft"; readonly sessionId: string; readonly text: string }
  | { readonly type: "create_session"; readonly definitionId: string; readonly commandId: string; readonly requestDigest: string }
  | { readonly type: "submit_draft"; readonly sessionId: string; readonly commandId: string; readonly requestDigest: string }
  | { readonly type: "retry_pending"; readonly sessionId?: string }
  | { readonly type: "cancel_turn"; readonly sessionId: string; readonly turnId: string; readonly commandId: string; readonly requestDigest: string }
  | { readonly type: "continue_suspension"; readonly sessionId: string; readonly turnId: string; readonly input: string; readonly commandId: string; readonly requestDigest: string }
  | { readonly type: "reconnect"; readonly sessionId: string }
  | { readonly type: "dismiss_notice" }
  | { readonly type: "effect_result"; readonly effectId: string; readonly generation: number; readonly sessionId?: string; readonly requestDigest?: string; readonly result: AppEffectPayload };

export type AppEffectPayload =
  | { readonly type: "preferences_loaded"; readonly selectedSessionId?: string; readonly drafts: readonly Draft[];
      readonly pending?: PendingCommand }
  | { readonly type: "preferences_saved" }
  | { readonly type: "definitions_loaded"; readonly definitions: readonly DefinitionItem[] }
  | { readonly type: "session_page_loaded"; readonly sessions: readonly SessionItem[] }
  | { readonly type: "timeline_loaded"; readonly items: readonly TimelineItem[]; readonly cursor: number; readonly activities: readonly ActivityItem[] }
  | { readonly type: "host_event"; readonly event: string; readonly position: number; readonly turnId?: string;
      readonly text?: string; readonly activity?: Omit<ActivityItem, "neutral"> }
  | { readonly type: "event_stream_ended" }
  | { readonly type: "command_succeeded"; readonly sessionId: string; readonly turnId?: string; readonly committedPosition: number }
  | { readonly type: "failed"; readonly error: AppError };

export interface Reduction { readonly state: AppViewState; readonly effects: readonly AppEffect[] }
export interface ControllerLimits { readonly maxDraftBytes: number; readonly maxActivities: number }
const DEFAULT_LIMITS: ControllerLimits = { maxDraftBytes: 4096, maxActivities: 128 };

export function initialAppViewState(
  configuration: "configured" | "not_configured" = "configured",
): AppViewState {
  return { configuration, shell: "booting", generation: 0, nextEffect: 1,
    definitions: [], sessions: [], timeline: [], cursor: 0, drafts: [], execution: "idle",
    pending: [], activities: [], outstanding: [], preferenceDirty: false };
}

export function reduceApp(
  state: AppViewState, intent: AppIntent, limits: ControllerLimits = DEFAULT_LIMITS,
): Reduction {
  if (limits.maxDraftBytes <= 0 || limits.maxActivities <= 0) return unchanged(state);
  switch (intent.type) {
    case "boot": {
      if (state.configuration === "not_configured") return changed({ ...state, shell: "not_configured" });
      const base = { ...state, shell: "loading_navigation" as const, generation: state.generation + 1 };
      return issueMany(base, [{ kind: "load_preferences" }, { kind: "load_definitions" }]);
    }
    case "select_session": {
      if (!state.sessions.some((item) => item.sessionId === intent.sessionId)) return notice(state, "validation", "session_not_found");
      const base = { ...state, selectedSessionId: intent.sessionId, timelineSessionId: undefined,
        timeline: [], cursor: 0, activities: [], execution: "idle" as const,
        generation: state.generation + 1,
        outstanding: state.outstanding.filter((effect) => mutation(effect.kind)) };
      return issueMany(base, [{ kind: "load_timeline", sessionId: intent.sessionId }, { kind: "save_preferences" }]);
    }
    case "edit_draft": {
      if (utf8(intent.text) > limits.maxDraftBytes) return notice(state, "validation", "draft_too_large");
      const drafts = state.drafts.filter((item) => item.sessionId !== intent.sessionId);
      if (intent.text.length) drafts.push({ sessionId: intent.sessionId, text: intent.text });
      return savePreferences({ ...state, drafts, notice: undefined });
    }
    case "create_session":
      if (!state.definitions.some((item) => item.definitionId === intent.definitionId)) {
        return notice(state, "validation", "definition_not_found");
      }
      return beginCommand(state, { kind: "create_session", commandId: intent.commandId,
        requestDigest: intent.requestDigest, generation: state.generation, status: "pending" }, {
        kind: "create_session", commandId: intent.commandId, requestDigest: intent.requestDigest,
        definitionId: intent.definitionId,
      });
    case "submit_draft": {
      if (!state.sessions.some((item) => item.sessionId === intent.sessionId)) return notice(state, "validation", "session_not_found");
      const text = state.drafts.find((item) => item.sessionId === intent.sessionId)?.text ?? "";
      if (!text.trim() || utf8(text) > limits.maxDraftBytes) return notice(state, "validation", "invalid_draft");
      return beginCommand({ ...state, execution: "submitting" }, {
        kind: "start_turn", commandId: intent.commandId, requestDigest: intent.requestDigest,
        generation: state.generation, sessionId: intent.sessionId, status: "pending",
      }, { kind: "start_turn", sessionId: intent.sessionId, commandId: intent.commandId,
        requestDigest: intent.requestDigest, text });
    }
    case "retry_pending": {
      const pending = state.pending.find((item) => item.sessionId === intent.sessionId && item.status === "unknown")
        ?? state.pending.find((item) => item.sessionId === undefined && item.status === "unknown");
      if (!pending) return notice(state, "validation", "no_unknown_command");
      const effect: Partial<AppEffect> & { kind: EffectKind } = { kind: pending.kind,
        sessionId: pending.sessionId, turnId: pending.turnId, commandId: pending.commandId,
        requestDigest: pending.requestDigest };
      return issueMany({ ...state, pending: replacePending(state.pending, { ...pending, status: "pending" }), notice: undefined }, [effect]);
    }
    case "cancel_turn":
      if (!state.timeline.some((item) => item.turnId === intent.turnId) ||
          !state.sessions.some((item) => item.sessionId === intent.sessionId)) {
        return notice(state, "validation", "turn_not_found");
      }
      return beginCommand({ ...state, execution: "cancelling" }, {
        kind: "cancel_turn", commandId: intent.commandId, requestDigest: intent.requestDigest,
        generation: state.generation, sessionId: intent.sessionId, turnId: intent.turnId, status: "pending",
      }, { kind: "cancel_turn", sessionId: intent.sessionId, turnId: intent.turnId,
        commandId: intent.commandId, requestDigest: intent.requestDigest,
        afterPosition: state.cursor });
    case "continue_suspension": {
      const suspension = state.timeline.find((item) => item.turnId === intent.turnId)?.suspension;
      if (!suspension?.suspensionId || !suspension.sessionVersion || !suspension.responseSchemaDigest || !intent.input) {
        return notice(state, "validation", "suspension_not_actionable");
      }
      const continuationValueKind = suspension.kind === "approval_required" ? "json_boolean" : "string";
      if (continuationValueKind === "json_boolean" && intent.input !== "true" && intent.input !== "false") {
        return notice(state, "validation", "invalid_suspension_response");
      }
      return beginCommand({ ...state, execution: "continuing" }, {
        kind: "continue_turn", commandId: intent.commandId, requestDigest: intent.requestDigest,
        generation: state.generation, sessionId: intent.sessionId, turnId: intent.turnId, status: "pending",
      }, { kind: "continue_turn", sessionId: intent.sessionId, turnId: intent.turnId,
        commandId: intent.commandId, requestDigest: intent.requestDigest, text: intent.input,
        suspensionId: suspension.suspensionId, sessionVersion: suspension.sessionVersion,
        responseSchemaDigest: suspension.responseSchemaDigest, continuationValueKind });
    }
    case "reconnect":
      if (state.execution !== "disconnected" || state.selectedSessionId !== intent.sessionId) return unchanged(state);
      return issueMany({ ...state, execution: "reconnecting" }, [{ kind: "follow_events",
        sessionId: intent.sessionId, afterPosition: state.cursor }]);
    case "dismiss_notice": return changed({ ...state, notice: undefined });
    case "effect_result": return applyResult(state, intent, limits);
  }
}

function applyResult(state: AppViewState, intent: Extract<AppIntent, { type: "effect_result" }>, limits: ControllerLimits): Reduction {
  const effect = state.outstanding.find((item) => item.effectId === intent.effectId);
  if (!effect || effect.generation !== intent.generation || effect.sessionId !== intent.sessionId ||
      effect.requestDigest !== intent.requestDigest) return unchanged(state);
  const navigationStale = !mutation(effect.kind) && effect.kind !== "save_preferences" &&
    effect.kind !== "load_preferences" && effect.generation !== state.generation;
  if (navigationStale) return changed(removeEffect(state, effect.effectId));
  if (intent.result.type === "failed") return failedResult(state, effect, intent.result.error);
  if (!resultMatches(effect.kind, intent.result.type)) {
    return failedResult(state, effect, { kind: "protocol", code: "unexpected_effect_result" });
  }
  let next = intent.result.type === "host_event" ? state : removeEffect(state, effect.effectId);
  switch (intent.result.type) {
    case "preferences_loaded": return changed({ ...next, drafts: intent.result.drafts,
      selectedSessionId: intent.result.selectedSessionId ?? next.selectedSessionId,
      pending: intent.result.pending ? [{ ...intent.result.pending, status: "unknown" }] : next.pending });
    case "preferences_saved": return next.preferenceDirty ? savePreferences(next) : changed(next);
    case "definitions_loaded":
      return issueMany({ ...next, definitions: intent.result.definitions }, [{ kind: "load_session_page" }]);
    case "session_page_loaded": {
      const selected = next.selectedSessionId && intent.result.sessions.some((item) => item.sessionId === next.selectedSessionId)
        ? next.selectedSessionId : intent.result.sessions[0]?.sessionId;
      const base = { ...next, sessions: intent.result.sessions, selectedSessionId: selected,
        shell: "ready" as const };
      return selected ? issueMany(base, [{ kind: "load_timeline", sessionId: selected }]) : changed(base);
    }
    case "timeline_loaded": {
      const latest = intent.result.items.at(-1);
      const execution: ExecutionState = latest?.state === "running" ? "following" :
        latest?.state === "suspended" ? "suspended" : "idle";
      const base = { ...next, timelineSessionId: effect.sessionId, timeline: intent.result.items,
        cursor: intent.result.cursor, activities: intent.result.activities, execution };
      return execution === "following" ? issueMany(base, [{ kind: "follow_events",
        sessionId: effect.sessionId, afterPosition: intent.result.cursor }]) : changed(base);
    }
    case "command_succeeded": return commandSucceeded(next, effect, intent.result);
    case "host_event": return hostEvent(next, effect, intent.result, limits);
    case "event_stream_ended": return changed({ ...next, execution: "disconnected",
      notice: { kind: "transport", code: "stream_ended" } });
  }
}

function commandSucceeded(state: AppViewState, effect: AppEffect, result: Extract<AppEffectPayload, { type: "command_succeeded" }>): Reduction {
  const pending = state.pending.filter((item) => item.commandId !== effect.commandId);
  if (effect.kind === "create_session") {
    const sessions = state.sessions.some((item) => item.sessionId === result.sessionId) ? state.sessions :
      [{ sessionId: result.sessionId }, ...state.sessions];
    const base = { ...state, sessions, selectedSessionId: result.sessionId, pending,
      generation: state.generation + 1, shell: "ready" as const };
    return issueWithPreference(base, [{ kind: "load_timeline", sessionId: result.sessionId }]);
  }
  const drafts = effect.kind === "start_turn" ? state.drafts.filter((item) => item.sessionId !== result.sessionId) : state.drafts;
  const timeline = effect.kind === "start_turn" && result.turnId &&
    !state.timeline.some((item) => item.turnId === result.turnId)
    ? [...state.timeline, { turnId: result.turnId, state: "running", latestPosition: result.committedPosition,
      userText: effect.text ?? "", contentTruncated: false, activities: [] }] : state.timeline;
  const base = { ...state, pending, drafts, timeline, cursor: result.committedPosition,
    execution: "following" as const, notice: undefined };
  const follow = [{ kind: "follow_events" as const, sessionId: result.sessionId,
    afterPosition: result.committedPosition }];
  return effect.kind === "start_turn" ? issueWithPreference(base, follow) : issueMany(base, follow);
}

function hostEvent(state: AppViewState, effect: AppEffect, result: Extract<AppEffectPayload, { type: "host_event" }>, limits: ControllerLimits): Reduction {
  if (result.position <= state.cursor) return unchanged(state);
  let execution = state.execution; let outstanding = state.outstanding; let notice = state.notice;
  if (result.event === "turn.suspended") execution = "suspended";
  else if (["turn.completed", "turn.stopped", "turn.failed"].includes(result.event)) {
    execution = "idle"; outstanding = outstanding.filter((item) => item.effectId !== effect.effectId);
  }
  let activities = state.activities;
  if (result.activity) {
    activities = [...activities.filter((item) => item.activityId !== result.activity!.activityId),
      { ...result.activity, neutral: false }].sort((a, b) => a.position - b.position).slice(-limits.maxActivities);
  } else if (!KNOWN_EVENTS.has(result.event)) {
    activities = [...activities, { activityId: `unknown-${result.position}`, kind: "unknown",
      state: "updated", turnId: result.turnId, position: result.position, neutral: true }].slice(-limits.maxActivities);
    notice = undefined;
  }
  const timeline = state.timeline.map((item) => {
    if (item.turnId !== result.turnId) return item;
    const turnActivities = result.activity ? [...(item.activities ?? []).filter((entry) =>
      entry.activityId !== result.activity!.activityId), { ...result.activity, neutral: false }] : item.activities;
    const terminalState = result.event === "turn.completed" ? "completed" : result.event === "turn.stopped" ? "stopped" :
      result.event === "turn.failed" ? "failed" : result.event === "turn.suspended" ? "suspended" : item.state;
    return { ...item, state: terminalState, latestPosition: result.position,
      completionText: result.event === "turn.completed" ? result.text ?? "" : item.completionText,
      activities: turnActivities };
  });
  const base = { ...state, timeline, cursor: result.position, execution, activities, outstanding, notice };
  return result.event === "turn.suspended" && effect.sessionId
    ? issueMany(base, [{ kind: "load_timeline", sessionId: effect.sessionId }]) : changed(base);
}

function failedResult(state: AppViewState, effect: AppEffect, error: AppError): Reduction {
  const next = removeEffect(state, effect.effectId);
  if (mutation(effect.kind) && (error.kind === "transport" || error.kind === "protocol")) {
    const pending = next.pending.map((item) => item.commandId === effect.commandId ? { ...item, status: "unknown" as const } : item);
    return changed({ ...next, pending, execution: "idle", notice: { kind: "command_unknown", code: "mutation_outcome_unknown" } });
  }
  if (effect.kind === "follow_events") return changed({ ...next, execution: "disconnected", notice: error });
  if (["load_definitions", "load_session_page", "load_timeline"].includes(effect.kind)) {
    return changed({ ...next, shell: "unavailable", notice: error });
  }
  return changed({ ...next, notice: error });
}

function beginCommand(state: AppViewState, pending: PendingCommand, effect: Partial<AppEffect> & { kind: EffectKind }): Reduction {
  if (!validIdentity(pending.commandId) || !/^[0-9a-f]{64}$/.test(pending.requestDigest) ||
      state.pending.length > 0) return notice(state, "validation", "command_not_admitted");
  return issueMany({ ...state, pending: [...state.pending, pending], notice: undefined }, [effect]);
}
function issueMany(state: AppViewState, raw: readonly (Partial<AppEffect> & { kind: EffectKind })[]): Reduction {
  let next = state.nextEffect;
  const effects = raw.map((value) => ({ ...value, effectId: `effect-${next++}`, generation: state.generation } as AppEffect));
  return { state: { ...state, nextEffect: next, outstanding: [...state.outstanding, ...effects] }, effects };
}
function savePreferences(state: AppViewState): Reduction {
  if (state.outstanding.some((effect) => effect.kind === "save_preferences")) {
    return changed({ ...state, preferenceDirty: true });
  }
  return issueMany({ ...state, preferenceDirty: false }, [{ kind: "save_preferences" }]);
}
function issueWithPreference(
  state: AppViewState, raw: readonly (Partial<AppEffect> & { kind: EffectKind })[],
): Reduction {
  const primary = issueMany(state, raw); const saved = savePreferences(primary.state);
  return { state: saved.state, effects: [...primary.effects, ...saved.effects] };
}
function removeEffect(state: AppViewState, id: string): AppViewState {
  return { ...state, outstanding: state.outstanding.filter((item) => item.effectId !== id) };
}
function replacePending(items: readonly PendingCommand[], replacement: PendingCommand): readonly PendingCommand[] {
  return items.map((item) => item.commandId === replacement.commandId ? replacement : item);
}
function mutation(kind: EffectKind): boolean { return ["create_session", "start_turn", "cancel_turn", "continue_turn"].includes(kind); }
function resultMatches(kind: EffectKind, type: AppEffectPayload["type"]): boolean {
  const expected: Readonly<Record<EffectKind, readonly AppEffectPayload["type"][]>> = {
    load_preferences: ["preferences_loaded"], save_preferences: ["preferences_saved"],
    load_definitions: ["definitions_loaded"], load_session_page: ["session_page_loaded"],
    load_timeline: ["timeline_loaded"], follow_events: ["host_event", "event_stream_ended"],
    create_session: ["command_succeeded"], start_turn: ["command_succeeded"],
    cancel_turn: ["command_succeeded"], continue_turn: ["command_succeeded"],
  };
  return expected[kind].includes(type);
}
function validIdentity(value: string): boolean { return value.length > 0 && value.length <= 128 && /^[\x21-\x7e]+$/.test(value); }
function utf8(value: string): number { return new TextEncoder().encode(value).length; }
function notice(state: AppViewState, kind: AppErrorKind, code: string): Reduction { return changed({ ...state, notice: { kind, code } }); }
function changed(state: AppViewState): Reduction { return { state, effects: [] }; }
function unchanged(state: AppViewState): Reduction { return { state, effects: [] }; }
const KNOWN_EVENTS = new Set(["session.created", "turn.started", "turn.completed", "turn.suspended", "turn.stopped", "turn.failed",
  "agent.activity.prepared", "agent.activity.started", "agent.activity.completed", "agent.activity.failed", "agent.activity.input_requested"]);
