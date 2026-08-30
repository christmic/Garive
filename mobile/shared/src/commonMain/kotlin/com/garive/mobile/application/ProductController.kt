package com.garive.mobile.application

import com.garive.mobile.model.*

/** User/application intent vocabulary for the pure A-UX1 controller. */
public sealed interface AppIntent {
    /** Starts configured or not-configured bootstrap. */
    public data object Boot : AppIntent
    /** Selects one discovered durable Session. */
    public data class SelectSession(public val sessionId: String) : AppIntent
    /** Replaces one local composer draft. */
    public data class EditDraft(public val sessionId: String, public val text: String) : AppIntent
    /** Creates a Session with application-owned stable identity. */
    public data class CreateSession(public val definitionId: String, public val commandId: String, public val requestDigest: String) : AppIntent
    /** Submits the selected Session draft with stable identity. */
    public data class SubmitDraft(public val sessionId: String, public val commandId: String, public val requestDigest: String) : AppIntent
    /** Retries only an uncertain byte-equivalent mutation. */
    public data class RetryPending(public val sessionId: String? = null) : AppIntent
    /** Requests cancellation without claiming a terminal. */
    public data class CancelTurn(
        public val sessionId: String, public val turnId: String,
        public val commandId: String, public val requestDigest: String,
    ) : AppIntent
    /** Continues one exact actionable suspension. */
    public data class ContinueSuspension(
        public val sessionId: String, public val turnId: String, public val input: String,
        public val commandId: String, public val requestDigest: String,
    ) : AppIntent
    /** Explicitly resumes follow from the retained cursor. */
    public data class Reconnect(public val sessionId: String) : AppIntent
    /** Clears only the current safe notice. */
    public data object DismissNotice : AppIntent
    /** Re-enters one exactly correlated effect result. */
    public data class EffectResult(
        public val effectId: String, public val generation: Long, public val sessionId: String? = null,
        public val requestDigest: String? = null, public val result: AppEffectPayload,
    ) : AppIntent
}

/** Typed values returned by composition-owned Host and preference ports. */
public sealed interface AppEffectPayload {
    public data class PreferencesLoaded(public val selectedSessionId: String?, public val drafts: List<Draft>) : AppEffectPayload
    public data object PreferencesSaved : AppEffectPayload
    public data class DefinitionsLoaded(public val definitionIds: List<String>) : AppEffectPayload
    public data class SessionPageLoaded(public val sessions: List<SessionItem>) : AppEffectPayload
    public data class TimelineLoaded(
        public val items: List<TimelineItem>, public val cursor: Long, public val activities: List<ActivityItem>,
    ) : AppEffectPayload
    public data class HostEvent(
        public val event: String, public val position: Long, public val turnId: String? = null,
        public val activity: ActivityItem? = null,
    ) : AppEffectPayload
    public data object EventStreamEnded : AppEffectPayload
    public data class CommandSucceeded(
        public val sessionId: String, public val turnId: String? = null, public val committedPosition: Long,
    ) : AppEffectPayload
    public data class Failed(public val error: AppError) : AppEffectPayload
}

/** Reduces one intent without performing I/O. */
public fun reduceApp(
    state: AppViewState,
    intent: AppIntent,
    limits: ControllerLimits = ControllerLimits(),
): Reduction {
    if (limits.maxDraftBytes <= 0 || limits.maxActivities <= 0) return Reduction(state)
    return when (intent) {
        AppIntent.Boot -> boot(state)
        is AppIntent.SelectSession -> selectSession(state, intent)
        is AppIntent.EditDraft -> editDraft(state, intent, limits)
        is AppIntent.CreateSession -> beginCommand(
            state,
            PendingCommand(CommandKind.CREATE_SESSION, intent.commandId, intent.requestDigest, state.generation, status = PendingStatus.PENDING),
            EffectDraft(EffectKind.CREATE_SESSION, commandId = intent.commandId, requestDigest = intent.requestDigest, definitionId = intent.definitionId),
        )
        is AppIntent.SubmitDraft -> submitDraft(state, intent, limits)
        is AppIntent.RetryPending -> retryPending(state, intent)
        is AppIntent.CancelTurn -> beginCommand(
            state.copy(execution = ExecutionState.CANCELLING),
            PendingCommand(CommandKind.CANCEL_TURN, intent.commandId, intent.requestDigest, state.generation,
                intent.sessionId, intent.turnId, PendingStatus.PENDING),
            EffectDraft(EffectKind.CANCEL_TURN, sessionId = intent.sessionId, turnId = intent.turnId,
                commandId = intent.commandId, requestDigest = intent.requestDigest),
        )
        is AppIntent.ContinueSuspension -> continueSuspension(state, intent)
        is AppIntent.Reconnect -> reconnect(state, intent)
        AppIntent.DismissNotice -> Reduction(state.copy(notice = null))
        is AppIntent.EffectResult -> applyResult(state, intent, limits)
    }
}

private fun boot(state: AppViewState): Reduction {
    if (state.configuration == AppConfiguration.NOT_CONFIGURED) {
        return Reduction(state.copy(shell = ShellState.NOT_CONFIGURED))
    }
    return issueMany(
        state.copy(shell = ShellState.LOADING_NAVIGATION, generation = state.generation + 1),
        listOf(EffectDraft(EffectKind.LOAD_PREFERENCES), EffectDraft(EffectKind.LOAD_DEFINITIONS)),
    )
}

private fun selectSession(state: AppViewState, intent: AppIntent.SelectSession): Reduction {
    if (state.sessions.none { it.sessionId == intent.sessionId }) return notice(state, AppErrorKind.VALIDATION, "session_not_found")
    val base = state.copy(
        selectedSessionId = intent.sessionId, timelineSessionId = null, timeline = emptyList(), cursor = 0,
        activities = emptyList(), execution = ExecutionState.IDLE, generation = state.generation + 1,
        outstanding = state.outstanding.filter { mutation(it.kind) },
    )
    return issueMany(base, listOf(EffectDraft(EffectKind.LOAD_TIMELINE, sessionId = intent.sessionId), EffectDraft(EffectKind.SAVE_PREFERENCES)))
}

private fun editDraft(state: AppViewState, intent: AppIntent.EditDraft, limits: ControllerLimits): Reduction {
    if (intent.text.encodeToByteArray().size > limits.maxDraftBytes) return notice(state, AppErrorKind.VALIDATION, "draft_too_large")
    val drafts = state.drafts.filterNot { it.sessionId == intent.sessionId }.toMutableList()
    if (intent.text.isNotEmpty()) drafts += Draft(intent.sessionId, intent.text)
    return issueMany(state.copy(drafts = drafts, notice = null), listOf(EffectDraft(EffectKind.SAVE_PREFERENCES)))
}

private fun submitDraft(state: AppViewState, intent: AppIntent.SubmitDraft, limits: ControllerLimits): Reduction {
    val text = state.drafts.firstOrNull { it.sessionId == intent.sessionId }?.text.orEmpty()
    if (text.isBlank() || text.encodeToByteArray().size > limits.maxDraftBytes) return notice(state, AppErrorKind.VALIDATION, "invalid_draft")
    return beginCommand(
        state.copy(execution = ExecutionState.SUBMITTING),
        PendingCommand(CommandKind.START_TURN, intent.commandId, intent.requestDigest, state.generation,
            intent.sessionId, status = PendingStatus.PENDING),
        EffectDraft(EffectKind.START_TURN, sessionId = intent.sessionId, commandId = intent.commandId,
            requestDigest = intent.requestDigest, text = text),
    )
}

private fun retryPending(state: AppViewState, intent: AppIntent.RetryPending): Reduction {
    val pending = state.pending.firstOrNull { it.sessionId == intent.sessionId && it.status == PendingStatus.UNKNOWN }
        ?: state.pending.firstOrNull { it.sessionId == null && it.status == PendingStatus.UNKNOWN }
        ?: return notice(state, AppErrorKind.VALIDATION, "no_unknown_command")
    val effect = EffectDraft(
        kind = effectKind(pending.kind), sessionId = pending.sessionId, turnId = pending.turnId,
        commandId = pending.commandId, requestDigest = pending.requestDigest,
    )
    return issueMany(
        state.copy(pending = replacePending(state.pending, pending.copy(status = PendingStatus.PENDING)), notice = null),
        listOf(effect),
    )
}

private fun continueSuspension(state: AppViewState, intent: AppIntent.ContinueSuspension): Reduction {
    val turn = state.timeline.firstOrNull { it.turnId == intent.turnId }
    if (turn?.suspensionId == null || turn.sessionVersion == null || turn.responseSchemaDigest == null || intent.input.isEmpty()) {
        return notice(state, AppErrorKind.VALIDATION, "suspension_not_actionable")
    }
    return beginCommand(
        state.copy(execution = ExecutionState.CONTINUING),
        PendingCommand(CommandKind.CONTINUE_TURN, intent.commandId, intent.requestDigest, state.generation,
            intent.sessionId, intent.turnId, PendingStatus.PENDING),
        EffectDraft(EffectKind.CONTINUE_TURN, sessionId = intent.sessionId, turnId = intent.turnId,
            commandId = intent.commandId, requestDigest = intent.requestDigest, text = intent.input,
            suspensionId = turn.suspensionId, sessionVersion = turn.sessionVersion,
            responseSchemaDigest = turn.responseSchemaDigest),
    )
}

private fun reconnect(state: AppViewState, intent: AppIntent.Reconnect): Reduction {
    if (state.execution != ExecutionState.DISCONNECTED || state.selectedSessionId != intent.sessionId) return Reduction(state)
    return issueMany(state.copy(execution = ExecutionState.RECONNECTING),
        listOf(EffectDraft(EffectKind.FOLLOW_EVENTS, sessionId = intent.sessionId, afterPosition = state.cursor)))
}

private fun applyResult(state: AppViewState, intent: AppIntent.EffectResult, limits: ControllerLimits): Reduction {
    val effect = state.outstanding.firstOrNull { it.effectId == intent.effectId } ?: return Reduction(state)
    if (effect.generation != intent.generation || effect.sessionId != intent.sessionId || effect.requestDigest != intent.requestDigest) {
        return Reduction(state)
    }
    val navigationStale = !mutation(effect.kind) && effect.kind !in setOf(EffectKind.SAVE_PREFERENCES, EffectKind.LOAD_PREFERENCES) &&
        effect.generation != state.generation
    if (navigationStale) return Reduction(removeEffect(state, effect.effectId))
    if (intent.result is AppEffectPayload.Failed) return failedResult(state, effect, intent.result.error)
    val next = if (intent.result is AppEffectPayload.HostEvent) state else removeEffect(state, effect.effectId)
    return when (val result = intent.result) {
        is AppEffectPayload.PreferencesLoaded -> Reduction(next.copy(drafts = result.drafts,
            selectedSessionId = result.selectedSessionId ?: next.selectedSessionId))
        AppEffectPayload.PreferencesSaved -> Reduction(next)
        is AppEffectPayload.DefinitionsLoaded -> issueMany(next.copy(definitionIds = result.definitionIds), listOf(EffectDraft(EffectKind.LOAD_SESSION_PAGE)))
        is AppEffectPayload.SessionPageLoaded -> sessionPageLoaded(next, result)
        is AppEffectPayload.TimelineLoaded -> timelineLoaded(next, effect, result)
        is AppEffectPayload.CommandSucceeded -> commandSucceeded(next, effect, result)
        is AppEffectPayload.HostEvent -> hostEvent(next, effect, result, limits)
        AppEffectPayload.EventStreamEnded -> Reduction(next.copy(execution = ExecutionState.DISCONNECTED,
            notice = AppError(AppErrorKind.TRANSPORT, "stream_ended")))
        is AppEffectPayload.Failed -> error("handled above")
    }
}

private fun sessionPageLoaded(state: AppViewState, result: AppEffectPayload.SessionPageLoaded): Reduction {
    val selected = state.selectedSessionId?.takeIf { id -> result.sessions.any { it.sessionId == id } }
        ?: result.sessions.firstOrNull()?.sessionId
    val base = state.copy(sessions = result.sessions, selectedSessionId = selected, shell = ShellState.READY)
    return if (selected == null) Reduction(base) else issueMany(base, listOf(EffectDraft(EffectKind.LOAD_TIMELINE, sessionId = selected)))
}

private fun timelineLoaded(state: AppViewState, effect: AppEffect, result: AppEffectPayload.TimelineLoaded): Reduction {
    val execution = when (result.items.lastOrNull()?.state) {
        "running" -> ExecutionState.FOLLOWING
        "suspended" -> ExecutionState.SUSPENDED
        else -> ExecutionState.IDLE
    }
    val base = state.copy(timelineSessionId = effect.sessionId, timeline = result.items, cursor = result.cursor,
        activities = result.activities, execution = execution)
    return if (execution != ExecutionState.FOLLOWING) Reduction(base) else issueMany(
        base, listOf(EffectDraft(EffectKind.FOLLOW_EVENTS, sessionId = effect.sessionId, afterPosition = result.cursor)),
    )
}

private fun commandSucceeded(state: AppViewState, effect: AppEffect, result: AppEffectPayload.CommandSucceeded): Reduction {
    val pending = state.pending.filterNot { it.commandId == effect.commandId }
    if (effect.kind == EffectKind.CREATE_SESSION) {
        val sessions = if (state.sessions.any { it.sessionId == result.sessionId }) state.sessions else listOf(SessionItem(result.sessionId)) + state.sessions
        return issueMany(
            state.copy(sessions = sessions, selectedSessionId = result.sessionId, pending = pending,
                generation = state.generation + 1, shell = ShellState.READY),
            listOf(EffectDraft(EffectKind.LOAD_TIMELINE, sessionId = result.sessionId), EffectDraft(EffectKind.SAVE_PREFERENCES)),
        )
    }
    val drafts = if (effect.kind == EffectKind.START_TURN) state.drafts.filterNot { it.sessionId == result.sessionId } else state.drafts
    val effects = mutableListOf(EffectDraft(EffectKind.FOLLOW_EVENTS, sessionId = result.sessionId, afterPosition = result.committedPosition))
    if (effect.kind == EffectKind.START_TURN) effects += EffectDraft(EffectKind.SAVE_PREFERENCES)
    return issueMany(state.copy(pending = pending, drafts = drafts, cursor = result.committedPosition,
        execution = ExecutionState.FOLLOWING, notice = null), effects)
}

private fun hostEvent(
    state: AppViewState, effect: AppEffect, result: AppEffectPayload.HostEvent, limits: ControllerLimits,
): Reduction {
    if (result.position <= state.cursor) return Reduction(state)
    var execution = state.execution
    var outstanding = state.outstanding
    var notice = state.notice
    when (result.event) {
        "turn.suspended" -> execution = ExecutionState.SUSPENDED
        "turn.completed", "turn.stopped", "turn.failed" -> {
            execution = ExecutionState.IDLE; outstanding = outstanding.filterNot { it.effectId == effect.effectId }
        }
    }
    var activities = state.activities
    if (result.activity != null) {
        activities = (activities.filterNot { it.activityId == result.activity.activityId } + result.activity.copy(neutral = false))
            .sortedBy { it.position }.takeLast(limits.maxActivities)
    } else if (result.event !in KNOWN_EVENTS) {
        activities = (activities + ActivityItem("unknown-${result.position}", "unknown", "updated",
            result.turnId, result.position, true)).takeLast(limits.maxActivities)
        notice = null
    }
    return Reduction(state.copy(cursor = result.position, execution = execution, activities = activities,
        outstanding = outstanding, notice = notice))
}

private fun failedResult(state: AppViewState, effect: AppEffect, error: AppError): Reduction {
    val next = removeEffect(state, effect.effectId)
    if (mutation(effect.kind) && error.kind == AppErrorKind.TRANSPORT) {
        val pending = next.pending.map { if (it.commandId == effect.commandId) it.copy(status = PendingStatus.UNKNOWN) else it }
        return Reduction(next.copy(pending = pending, execution = ExecutionState.IDLE,
            notice = AppError(AppErrorKind.COMMAND_UNKNOWN, "mutation_outcome_unknown")))
    }
    if (effect.kind == EffectKind.FOLLOW_EVENTS) return Reduction(next.copy(execution = ExecutionState.DISCONNECTED, notice = error))
    if (effect.kind in setOf(EffectKind.LOAD_DEFINITIONS, EffectKind.LOAD_SESSION_PAGE, EffectKind.LOAD_TIMELINE)) {
        return Reduction(next.copy(shell = ShellState.UNAVAILABLE, notice = error))
    }
    return Reduction(next.copy(notice = error))
}

private fun beginCommand(state: AppViewState, pending: PendingCommand, effect: EffectDraft): Reduction {
    if (!validIdentity(pending.commandId) || !HEX_DIGEST.matches(pending.requestDigest) ||
        state.pending.any { it.sessionId == pending.sessionId }
    ) return notice(state, AppErrorKind.VALIDATION, "command_not_admitted")
    return issueMany(state.copy(pending = state.pending + pending, notice = null), listOf(effect))
}

private data class EffectDraft(
    val kind: EffectKind, val sessionId: String? = null, val commandId: String? = null,
    val requestDigest: String? = null, val afterPosition: Long? = null, val definitionId: String? = null,
    val text: String? = null, val turnId: String? = null, val suspensionId: String? = null,
    val sessionVersion: Long? = null, val responseSchemaDigest: String? = null,
)

private fun issueMany(state: AppViewState, drafts: List<EffectDraft>): Reduction {
    var next = state.nextEffect
    val effects = drafts.map { draft ->
        AppEffect("effect-${next++}", draft.kind, state.generation, draft.sessionId, draft.commandId,
            draft.requestDigest, draft.afterPosition, draft.definitionId, draft.text, draft.turnId,
            draft.suspensionId, draft.sessionVersion, draft.responseSchemaDigest)
    }
    return Reduction(state.copy(nextEffect = next, outstanding = state.outstanding + effects), effects)
}

private fun removeEffect(state: AppViewState, id: String): AppViewState = state.copy(outstanding = state.outstanding.filterNot { it.effectId == id })
private fun replacePending(values: List<PendingCommand>, replacement: PendingCommand): List<PendingCommand> =
    values.map { if (it.commandId == replacement.commandId) replacement else it }
private fun mutation(kind: EffectKind): Boolean = kind in setOf(EffectKind.CREATE_SESSION, EffectKind.START_TURN, EffectKind.CANCEL_TURN, EffectKind.CONTINUE_TURN)
private fun effectKind(kind: CommandKind): EffectKind = when (kind) {
    CommandKind.CREATE_SESSION -> EffectKind.CREATE_SESSION
    CommandKind.START_TURN -> EffectKind.START_TURN
    CommandKind.CANCEL_TURN -> EffectKind.CANCEL_TURN
    CommandKind.CONTINUE_TURN -> EffectKind.CONTINUE_TURN
}
private fun validIdentity(value: String): Boolean = value.isNotEmpty() && value.length <= 128 && value.all { it.code in 0x21..0x7e }
private fun notice(state: AppViewState, kind: AppErrorKind, code: String): Reduction = Reduction(state.copy(notice = AppError(kind, code)))
private val HEX_DIGEST: Regex = Regex("[0-9a-f]{64}")
private val KNOWN_EVENTS: Set<String> = setOf(
    "session.created", "turn.started", "turn.completed", "turn.suspended", "turn.stopped", "turn.failed",
    "agent.activity.prepared", "agent.activity.started", "agent.activity.completed", "agent.activity.failed",
    "agent.activity.input_requested",
)
