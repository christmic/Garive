package com.garive.mobile.application

import com.garive.mobile.host.HostClientError
import com.garive.mobile.host.HostClientException
import com.garive.mobile.host.MobileHost
import com.garive.mobile.model.MobileActivityItem
import com.garive.mobile.model.MobileAgentCard
import com.garive.mobile.model.MobileConnectionState
import com.garive.mobile.model.MobileDecision
import com.garive.mobile.model.MobileDestination
import com.garive.mobile.model.MobilePendingCommand
import com.garive.mobile.model.MobileSessionCard
import com.garive.mobile.model.MobileTurnItem
import com.garive.mobile.model.MobileWorkState
import com.garive.mobile.model.MobileWorkStatus
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** Native-supplied collision-resistant command identity source. */
public fun interface CommandIdentitySource {
    /** Returns one new printable stable command identity. */
    public fun nextId(): String
}

/** Shared Host-only workflow controller consumed by both native apps. */
public class MobileWorkController(
    private val host: MobileHost,
    private val identities: CommandIdentitySource,
    private val pageLimit: Int = 64,
    private val maxInputBytes: Int = 4_096,
    private val persistence: MobileWorkPersistence = EphemeralMobileWorkPersistence,
) {
    private val lock: Mutex = Mutex()
    private var viewState: MobileWorkState = MobileWorkState()
    private var pendingOperation: PendingOperation? = null
    private var restoredPending: Boolean = false
    private var restoredPreferences: Boolean = false
    private var preferenceTheme: String = "system"
    private val sessionDrafts: LinkedHashMap<String, String> = linkedMapOf()
    private var newTaskDraft: String = ""

    init {
        require(pageLimit > 0 && maxInputBytes > 0)
    }

    /** Returns the latest immutable product state. */
    public fun state(): MobileWorkState = viewState

    /** Loads installed Agents, durable Sessions, and the selected timeline. */
    public suspend fun boot(): MobileWorkState = lock.withLock {
        restorePreferencesLocked()
        restorePendingLocked()
        viewState = viewState.copy(connection = MobileConnectionState.CONNECTING, refreshing = true)
        refreshLocked()
    }

    /** Refreshes durable navigation and the selected conversation. */
    public suspend fun refresh(): MobileWorkState = lock.withLock {
        viewState = viewState.copy(refreshing = true)
        refreshLocked()
    }

    /** Selects a stable top-level destination. */
    public fun selectDestination(destination: MobileDestination): MobileWorkState {
        viewState = viewState.copy(destination = destination, noticeCode = null)
        savePreferencesLocked()
        return viewState
    }

    /** Starts a native new-task composition without leaking another Session's draft. */
    public fun beginTask(): MobileWorkState {
        viewState = viewState.copy(
            selectedSessionId = null,
            timeline = emptyList(),
            timelineCursor = 0,
            draft = newTaskDraft,
            noticeCode = null,
        )
        savePreferencesLocked()
        return viewState
    }

    /** Mirrors the native appearance choice into the strict preference document. */
    public fun setTheme(theme: String): MobileWorkState {
        require(theme in setOf("system", "light", "dark"))
        preferenceTheme = theme
        savePreferencesLocked()
        return viewState
    }

    /** Dismisses one presentation notice without changing durable or pending work. */
    public fun dismissNotice(): MobileWorkState {
        viewState = viewState.copy(noticeCode = null)
        return viewState
    }

    /** Updates the current bounded composer draft. */
    public fun editDraft(text: String): MobileWorkState {
        viewState = viewState.copy(draft = text, noticeCode = null)
        if (text.encodeToByteArray().size <= maxInputBytes) {
            val sessionId = viewState.selectedSessionId
            if (sessionId == null) {
                newTaskDraft = text
            } else if (text.isEmpty()) {
                sessionDrafts.remove(sessionId)
            } else {
                sessionDrafts.remove(sessionId)
                sessionDrafts[sessionId] = text
                while (sessionDrafts.size > 20) sessionDrafts.remove(sessionDrafts.keys.first())
            }
            savePreferencesLocked()
        }
        return viewState
    }

    /** Opens one durable Session and loads its complete current timeline page. */
    public suspend fun openSession(sessionId: String): MobileWorkState = lock.withLock {
        try {
            val timeline = host.timeline(sessionId, 0, pageLimit)
            viewState = viewState.copy(
                destination = MobileDestination.CONVERSATION,
                connection = MobileConnectionState.ONLINE,
                selectedSessionId = sessionId,
                timeline = timeline.items.map(::turnItem),
                timelineCursor = timeline.observed_max_position,
                draft = sessionDrafts[sessionId].orEmpty(),
                noticeCode = null,
            )
            savePreferencesLocked()
        } catch (error: CancellationException) {
            throw error
        } catch (error: HostClientException) {
            applyFailure(error)
        }
        viewState
    }

    /** Creates a Session and starts its first Turn using stable identities. */
    public suspend fun startTask(definitionId: String, text: String): MobileWorkState = lock.withLock {
        validateInput(text)?.let { return@withLock it }
        val operation = PendingOperation.CreateAndStart(
            definitionId = definitionId,
            text = text,
            createCommandId = identities.nextId(),
            startCommandId = identities.nextId(),
        )
        pendingOperation = operation
        savePending(persistence, operation)
        runPendingLocked(operation)
    }

    /** Starts another Turn in the currently selected durable Session. */
    public suspend fun sendTurn(text: String): MobileWorkState = lock.withLock {
        validateInput(text)?.let { return@withLock it }
        val sessionId = viewState.selectedSessionId
            ?: return@withLock notice("validation_session_required")
        val operation = PendingOperation.Start(sessionId, text, identities.nextId())
        pendingOperation = operation
        savePending(persistence, operation)
        runPendingLocked(operation)
    }

    /** Requests cancellation of the latest active Turn. */
    public suspend fun cancelLatest(): MobileWorkState = lock.withLock {
        val sessionId = viewState.selectedSessionId
            ?: return@withLock notice("validation_session_required")
        val turn = viewState.timeline.lastOrNull()
            ?: return@withLock notice("validation_turn_required")
        if (turn.status !in setOf(MobileWorkStatus.WORKING, MobileWorkStatus.NEEDS_INPUT)) {
            return@withLock notice("validation_turn_not_active")
        }
        val operation = PendingOperation.Cancel(
            sessionId,
            turn.turnId,
            turn.latestPosition,
            identities.nextId(),
        )
        pendingOperation = operation
        savePending(persistence, operation)
        runPendingLocked(operation)
    }

    /** Continues the latest supported suspension with exact coordinates. */
    public suspend fun continueLatest(input: String): MobileWorkState = lock.withLock {
        validateInput(input)?.let { return@withLock it }
        val sessionId = viewState.selectedSessionId
            ?: return@withLock notice("validation_session_required")
        val turn = viewState.timeline.lastOrNull()
            ?: return@withLock notice("validation_turn_required")
        val decision = turn.decision
            ?: return@withLock notice("validation_decision_required")
        val inputJson = decision.kind == "approval_required"
        val admittedInput = if (inputJson) {
            when (input) {
                "true", "false" -> input
                else -> return@withLock notice("validation_decision_boolean_required")
            }
        } else {
            input
        }
        val operation = PendingOperation.Continue(
            sessionId,
            turn.turnId,
            decision.suspensionId,
            decision.sessionVersion,
            admittedInput,
            inputJson,
            identities.nextId(),
        )
        pendingOperation = operation
        savePending(persistence, operation)
        runPendingLocked(operation)
    }

    /** Retries only the exact retained ambiguous mutation. */
    public suspend fun retryExact(): MobileWorkState = lock.withLock {
        val operation = pendingOperation ?: return@withLock notice("validation_retry_absent")
        runPendingLocked(operation)
    }

    /** Forgets one ambiguous retry identity without claiming that server work was undone. */
    public fun abandonPending(): MobileWorkState {
        val operation = pendingOperation ?: return notice("validation_retry_absent")
        pendingOperation = null
        clearPending(persistence)
        viewState = viewState.copy(
            draft = operation.payload() ?: viewState.draft,
            pendingCommand = null,
            noticeCode = "pending_retry_abandoned",
        )
        operation.sessionId?.let { sessionId ->
            operation.payload()?.let { sessionDrafts[sessionId] = it }
        }
        savePreferencesLocked()
        return viewState
    }

    /** Follows the selected Session to a terminal and then reloads durable truth. */
    public suspend fun followSelectedUntilTerminal(): MobileWorkState = lock.withLock {
        val sessionId = viewState.selectedSessionId
            ?: return@withLock notice("validation_session_required")
        try {
            host.followUntilTerminal(sessionId, viewState.timelineCursor)
            loadTimelineLocked(sessionId)
            loadNavigationLocked()
            viewState = viewState.copy(connection = MobileConnectionState.ONLINE, noticeCode = null)
        } catch (error: CancellationException) {
            throw error
        } catch (error: HostClientException) {
            applyFailure(error)
        }
        viewState
    }

    /** Clears the current non-secret presentation after native credential removal. */
    public fun signOut(): MobileWorkState {
        pendingOperation = null
        clearPending(persistence)
        persistence.writePreferencesRecord(null)
        sessionDrafts.clear()
        newTaskDraft = ""
        viewState = MobileWorkState(connection = MobileConnectionState.SIGNED_OUT)
        return viewState
    }

    private suspend fun refreshLocked(): MobileWorkState = try {
        val definitions = host.agentDefinitions().definitions
        viewState = viewState.copy(agents = definitions.map(::agentCard))
        loadNavigationLocked()
        viewState.selectedSessionId?.let { loadTimelineLocked(it) }
        viewState = viewState.copy(
            connection = MobileConnectionState.ONLINE,
            refreshing = false,
            noticeCode = if (pendingOperation != null) "command_unknown" else null,
        )
        viewState
    } catch (error: CancellationException) {
        throw error
    } catch (error: HostClientException) {
        applyFailure(error).copy(refreshing = false).also { viewState = it }
    }

    private suspend fun runPendingLocked(operation: PendingOperation): MobileWorkState {
        viewState = viewState.copy(pendingCommand = operation.publicValue(), noticeCode = null)
        try {
            when (operation) {
                is PendingOperation.CreateAndStart -> {
                    val sessionId = operation.sessionId ?: host
                        .createSession(operation.createCommandId, operation.definitionId)
                        .session_id
                        .also {
                            operation.sessionId = it
                            viewState = viewState.copy(pendingCommand = operation.publicValue())
                            savePending(persistence, operation)
                        }
                    host.startTurn(operation.startCommandId, sessionId, operation.text)
                    viewState = viewState.copy(
                        destination = MobileDestination.CONVERSATION,
                        selectedSessionId = sessionId,
                        draft = "",
                    )
                    newTaskDraft = ""
                }
                is PendingOperation.Start -> {
                    host.startTurn(operation.commandId, operation.sessionId, operation.text)
                    viewState = viewState.copy(draft = "")
                    sessionDrafts.remove(operation.sessionId)
                }
                is PendingOperation.Cancel -> host.cancelTurn(
                    operation.commandId,
                    operation.sessionId,
                    operation.turnId,
                    operation.position,
                )
                is PendingOperation.Continue -> if (operation.inputJson) {
                    val approve = operation.input == "true"
                    host.approvalEvent(
                        operation.commandId,
                        operation.sessionId,
                        operation.turnId,
                        operation.suspensionId,
                        operation.sessionVersion,
                        approve,
                    )
                } else {
                    host.externalInputEvent(
                        operation.commandId,
                        operation.sessionId,
                        operation.turnId,
                        operation.suspensionId,
                        operation.sessionVersion,
                        operation.input,
                    )
                }
            }
            if (operation is PendingOperation.Continue) {
                sessionDrafts.remove(operation.sessionId)
                viewState = viewState.copy(draft = "")
            }
            val sessionId = when (operation) {
                is PendingOperation.CreateAndStart -> operation.sessionId
                else -> operation.sessionId
            }
            if (sessionId != null) loadTimelineLocked(sessionId)
            loadNavigationLocked()
            pendingOperation = null
            clearPending(persistence)
            viewState = viewState.copy(
                connection = MobileConnectionState.ONLINE,
                pendingCommand = null,
                noticeCode = null,
            )
            savePreferencesLocked()
        } catch (error: CancellationException) {
            throw error
        } catch (error: HostClientException) {
            applyFailure(error)
            if (error.code !in setOf(
                    HostClientError.TRANSPORT_FAILURE,
                    HostClientError.FOLLOW_DEADLINE,
                    HostClientError.RUNTIME_UNAVAILABLE,
                )
            ) {
                pendingOperation = null
                clearPending(persistence)
                viewState = viewState.copy(pendingCommand = null)
            }
        }
        return viewState
    }

    private suspend fun loadNavigationLocked(): Unit {
        val definitions = viewState.agents.associateBy { it.definitionId }
        viewState = viewState.copy(
            sessions = host.sessions(pageLimit).sessions.map { session -> sessionCard(session, definitions) },
        )
    }

    private suspend fun loadTimelineLocked(sessionId: String): Unit {
        val timeline = host.timeline(sessionId, 0, pageLimit)
        viewState = viewState.copy(
            selectedSessionId = sessionId,
            timeline = timeline.items.map(::turnItem),
            timelineCursor = timeline.observed_max_position,
        )
    }

    private fun validateInput(text: String): MobileWorkState? = when {
        text.isBlank() -> notice("validation_input_empty")
        text.encodeToByteArray().size > maxInputBytes -> notice("validation_input_too_large")
        else -> null
    }

    private fun restorePendingLocked(): Unit {
        if (restoredPending) return
        restoredPending = true
        pendingOperation = restorePending(persistence, maxInputBytes)
        pendingOperation?.let { operation ->
            viewState = viewState.copy(
                selectedSessionId = operation.sessionId,
                draft = operation.payload().orEmpty(),
                pendingCommand = operation.publicValue(),
            )
        }
    }

    private fun restorePreferencesLocked(): Unit {
        if (restoredPreferences) return
        restoredPreferences = true
        val restored = restoreMobilePreferences(persistence)
        preferenceTheme = restored.theme
        sessionDrafts.clear()
        sessionDrafts.putAll(restored.drafts)
        viewState = viewState.copy(
            destination = restored.destination,
            selectedSessionId = restored.selectedSessionId,
            draft = restored.selectedSessionId?.let(sessionDrafts::get).orEmpty(),
        )
    }

    private fun savePreferencesLocked(): Unit {
        if (!restoredPreferences) return
        saveMobilePreferences(
            persistence,
            viewState.destination,
            viewState.selectedSessionId,
            preferenceTheme,
            sessionDrafts,
        )
    }

    private fun applyFailure(error: HostClientException): MobileWorkState {
        val connection = when (error.code) {
            HostClientError.INVALID_CONFIGURATION, HostClientError.ACTOR_FORBIDDEN,
            HostClientError.DEVICE_REAUTH_REQUIRED -> MobileConnectionState.SECURITY_ERROR
            HostClientError.AUTHENTICATION_REQUIRED -> MobileConnectionState.SIGNED_OUT
            HostClientError.TRANSPORT_FAILURE, HostClientError.FOLLOW_DEADLINE,
            HostClientError.RUNTIME_UNAVAILABLE -> MobileConnectionState.OFFLINE
            else -> viewState.connection
        }
        viewState = viewState.copy(connection = connection, refreshing = false, noticeCode = error.code.wireName)
        return viewState
    }

    private fun notice(code: String): MobileWorkState {
        viewState = viewState.copy(noticeCode = code)
        return viewState
    }
}
