package com.garive.mobile.application

import com.garive.host.v1.AgentDefinitionPageV1
import com.garive.host.v1.AgentDefinitionSummaryV1
import com.garive.host.v1.CreateSessionResponseV1
import com.garive.host.v1.HostActivityV1
import com.garive.host.v1.SessionPageV1
import com.garive.host.v1.SessionSummaryV1
import com.garive.host.v1.SessionViewV1
import com.garive.host.v1.SuspensionViewV1
import com.garive.host.v1.TurnCommandResponseV1
import com.garive.host.v1.TurnTimelineItemV1
import com.garive.host.v1.TurnTimelinePageV1
import com.garive.mobile.host.HostClientError
import com.garive.mobile.host.HostClientException
import com.garive.mobile.host.HostTerminalKind
import com.garive.mobile.host.HostView
import com.garive.mobile.host.MobileHost
import com.garive.mobile.model.MobileConnectionState
import com.garive.mobile.model.MobileDestination
import com.garive.mobile.model.MobileWorkStatus
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

public class MobileWorkControllerTest {
    @Test
    public fun bootAndOpenMapDurableAttentionWithoutClientTruth(): Unit = runBlocking {
        val host = FakeMobileHost()
        val controller = MobileWorkController(host, identities())

        val booted = controller.boot()
        assertEquals(MobileConnectionState.ONLINE, booted.connection)
        assertEquals("Definition Main", booted.agents.single().displayName)
        assertEquals(MobileWorkStatus.NEEDS_INPUT, booted.attention.single().status)

        val opened = controller.openSession("session-1")
        assertEquals(MobileDestination.CONVERSATION, opened.destination)
        assertEquals("Approval needed", opened.timeline.single().decision?.title)
        assertEquals("Read File", opened.timeline.single().activities.single().label)
        assertEquals(9, opened.timelineCursor)
    }

    @Test
    public fun unknownStartRetainsAndRetriesExactIdentity(): Unit = runBlocking {
        val host = FakeMobileHost(failFirstStart = true)
        val controller = MobileWorkController(host, identities())
        controller.boot()
        controller.editDraft("Ship the mobile client")

        val unknown = controller.startTask("definition-main", "Ship the mobile client")
        assertEquals(MobileConnectionState.OFFLINE, unknown.connection)
        assertEquals("start", unknown.pendingCommand?.kind)
        assertEquals("command-2", unknown.pendingCommand?.commandId)
        assertEquals("Ship the mobile client", unknown.draft)

        val retried = controller.retryExact()
        assertEquals(listOf("command-2", "command-2"), host.startCommandIds)
        assertEquals(MobileConnectionState.ONLINE, retried.connection)
        assertNull(retried.pendingCommand)
        assertEquals("", retried.draft)
        assertEquals("session-new", retried.selectedSessionId)
    }

    @Test
    public fun approvalUsesTypedCanonicalJson(): Unit = runBlocking {
        val host = FakeMobileHost()
        val controller = MobileWorkController(host, identities())
        controller.boot()
        controller.openSession("session-1")

        controller.continueLatest("true")

        assertEquals("true", host.continuationInput)
        assertEquals(true, host.continuationIsJson)
    }

    @Test
    public fun approvalCanBeExplicitlyDeclined(): Unit = runBlocking {
        val host = FakeMobileHost()
        val controller = MobileWorkController(host, identities())
        controller.boot()
        controller.openSession("session-1")

        controller.continueLatest("false")

        assertEquals("false", host.continuationInput)
        assertEquals(true, host.continuationIsJson)
    }

    @Test
    public fun revokedGrantFailsClosedToSignedOut(): Unit = runBlocking {
        val controller = MobileWorkController(
            FakeMobileHost(failDefinitionsWith = HostClientError.AUTHENTICATION_REQUIRED),
            identities(),
        )

        val state = controller.boot()

        assertEquals(MobileConnectionState.SIGNED_OUT, state.connection)
        assertEquals("authentication_required", state.noticeCode)
    }

    @Test
    public fun deviceBindingFailureIsAVisibleSecurityError(): Unit = runBlocking {
        val controller = MobileWorkController(
            FakeMobileHost(failDefinitionsWith = HostClientError.DEVICE_REAUTH_REQUIRED),
            identities(),
        )

        val state = controller.boot()

        assertEquals(MobileConnectionState.SECURITY_ERROR, state.connection)
        assertEquals("device_reauth_required", state.noticeCode)
    }

    @Test
    public fun ambiguousStartSurvivesRestartWithExactIdentity(): Unit = runBlocking {
        val persistence = MemoryMobileWorkPersistence()
        val first = MobileWorkController(
            FakeMobileHost(failFirstStart = true), identities(), persistence = persistence,
        )
        first.boot()

        val unknown = first.startTask("definition-main", "Ship the mobile client")

        assertEquals("command-2", unknown.pendingCommand?.commandId)
        assertEquals("Ship the mobile client", persistence.payload)
        val restoredHost = FakeMobileHost()
        val restored = MobileWorkController(restoredHost, identities(), persistence = persistence)

        val booted = restored.boot()

        assertEquals("command-2", booted.pendingCommand?.commandId)
        assertEquals("Ship the mobile client", booted.draft)
        assertEquals(MobileConnectionState.ONLINE, booted.connection)
        assertEquals("command_unknown", booted.noticeCode)
        val retried = restored.retryExact()
        assertEquals(listOf("command-2"), restoredHost.startCommandIds)
        assertNull(retried.pendingCommand)
        assertNull(persistence.record)
        assertNull(persistence.payload)
    }

    @Test
    public fun corruptPendingRecordClearsRecordAndPayloadOnly(): Unit = runBlocking {
        val persistence = MemoryMobileWorkPersistence(record = "{\"schema_version\":999}", payload = "private draft")
        val controller = MobileWorkController(FakeMobileHost(), identities(), persistence = persistence)

        val state = controller.boot()

        assertNull(state.pendingCommand)
        assertEquals("", state.draft)
        assertNull(persistence.record)
        assertNull(persistence.payload)
        assertEquals(MobileConnectionState.ONLINE, state.connection)
    }

    @Test
    public fun abandoningUnknownRetryClearsIdentityButKeepsEditableDraft(): Unit = runBlocking {
        val persistence = MemoryMobileWorkPersistence()
        val controller = MobileWorkController(
            FakeMobileHost(failFirstStart = true), identities(), persistence = persistence,
        )
        controller.boot()
        controller.startTask("definition-main", "Review before resubmitting")

        val abandoned = controller.abandonPending()

        assertNull(abandoned.pendingCommand)
        assertEquals("Review before resubmitting", abandoned.draft)
        assertEquals("pending_retry_abandoned", abandoned.noticeCode)
        assertNull(persistence.record)
        assertNull(persistence.payload)
    }

    @Test
    public fun navigationAndSessionDraftSurviveRestartAndClearAfterCommit(): Unit = runBlocking {
        val persistence = MemoryMobileWorkPersistence()
        val first = MobileWorkController(FakeMobileHost(), identities(), persistence = persistence)
        first.boot()
        first.openSession("session-1")
        first.editDraft("Continue the exact investigation")
        first.selectDestination(MobileDestination.SETTINGS)

        val restored = MobileWorkController(FakeMobileHost(), identities(), persistence = persistence)
        val booted = restored.boot()

        assertEquals(MobileDestination.SETTINGS, booted.destination)
        assertEquals("session-1", booted.selectedSessionId)
        assertEquals("Continue the exact investigation", restored.openSession("session-1").draft)

        restored.sendTurn("Continue the exact investigation")
        val afterCommit = MobileWorkController(FakeMobileHost(), identities(), persistence = persistence).boot()
        assertEquals("", afterCommit.draft)

        restored.signOut()
        assertNull(persistence.preferences)
    }

    private fun identities(): CommandIdentitySource {
        var next = 0
        return CommandIdentitySource { "command-${++next}" }
    }
}

private class MemoryMobileWorkPersistence(
    var record: String? = null,
    var payload: String? = null,
    var preferences: String? = null,
) : MobileWorkPersistence {
    override fun readPreferencesRecord(): String? = preferences
    override fun writePreferencesRecord(value: String?) { preferences = value }
    override fun readPendingRecord(): String? = record
    override fun writePendingRecord(value: String?) { record = value }
    override fun readPendingPayload(): String? = payload
    override fun writePendingPayload(value: String?) { payload = value }
}

private class FakeMobileHost(
    private var failFirstStart: Boolean = false,
    private val failDefinitionsWith: HostClientError? = null,
) : MobileHost {
    val startCommandIds: MutableList<String> = mutableListOf()
    var continuationInput: String? = null
    var continuationIsJson: Boolean? = null
    private var created: Boolean = false

    override suspend fun agentDefinitions(): AgentDefinitionPageV1 {
        failDefinitionsWith?.let { throw HostClientException(it) }
        return AgentDefinitionPageV1(
            api_version = "v1",
            definitions = listOf(
                AgentDefinitionSummaryV1(
                    api_version = "v1",
                    definition_id = "definition-main",
                    definition_revision = "revision-1",
                    capabilities = listOf("work"),
                ),
            ),
        )
    }

    override suspend fun sessions(limit: Int): SessionPageV1 = SessionPageV1(
        api_version = "v1",
        sessions = if (created) listOf(summary("session-new", "running")) else listOf(summary("session-1", "suspended")),
    )

    override suspend fun session(sessionId: String): SessionViewV1 = SessionViewV1(
        api_version = "v1",
        session = summary(sessionId, if (sessionId == "session-1") "suspended" else "running"),
        observed_max_position = 9,
    )

    override suspend fun timeline(sessionId: String, afterPosition: Long, limit: Int): TurnTimelinePageV1 =
        TurnTimelinePageV1(
            api_version = "v1",
            session_id = sessionId,
            items = if (sessionId == "session-1") listOf(suspendedTurn()) else listOf(runningTurn()),
            scanned_through_position = 9,
            observed_max_position = 9,
            has_more = false,
        )

    override suspend fun createSession(commandId: String, definitionId: String): CreateSessionResponseV1 {
        created = true
        return CreateSessionResponseV1("session-new", "agent-1", 1)
    }

    override suspend fun startTurn(
        commandId: String,
        sessionId: String,
        text: String,
    ): TurnCommandResponseV1 {
        startCommandIds += commandId
        if (failFirstStart) {
            failFirstStart = false
            throw HostClientException(HostClientError.TRANSPORT_FAILURE)
        }
        return TurnCommandResponseV1(sessionId, "turn-new", "execution-new", 2)
    }

    override suspend fun cancelTurn(
        commandId: String,
        sessionId: String,
        turnId: String,
        requestedThroughPosition: Long,
    ): TurnCommandResponseV1 = TurnCommandResponseV1(sessionId, turnId, "execution-1", 10)

    override suspend fun continueTurn(
        commandId: String,
        sessionId: String,
        turnId: String,
        suspensionId: String,
        expectedSessionVersion: Long,
        input: String,
        inputJson: Boolean,
    ): TurnCommandResponseV1 {
        continuationInput = input
        continuationIsJson = inputJson
        return TurnCommandResponseV1(sessionId, turnId, "execution-2", 10)
    }

    override suspend fun followUntilTerminal(sessionId: String, afterPosition: Long): HostView =
        HostView(cursor = 10, terminal = HostTerminalKind.COMPLETED, text = "done")

    private fun summary(sessionId: String, state: String): SessionSummaryV1 = SessionSummaryV1(
        api_version = "v1",
        session_id = sessionId,
        agent_instance_id = "agent-1",
        definition_id = "definition-main",
        definition_revision = "revision-1",
        opened_at = "2026-08-30T00:00:00Z",
        latest_position = 9,
        latest_turn_id = "turn-1",
        latest_turn_state = state,
        turn_count = 1,
    )

    private fun suspendedTurn(): TurnTimelineItemV1 = TurnTimelineItemV1(
        turn_id = "turn-1",
        started_position = 2,
        latest_position = 9,
        state = "suspended",
        user_text = "Inspect the repository",
        suspension = SuspensionViewV1(
            suspension_id = "suspension-1",
            session_version = 3,
            kind = "approval_required",
        ),
        activities = listOf(
            HostActivityV1(
                api_version = "v1",
                activity_id = "activity-1",
                kind = "tool",
                label_key = "agent.activity.read_file",
                state = "waiting_for_input",
                source_position = 8,
                terminal = false,
            ),
        ),
    )

    private fun runningTurn(): TurnTimelineItemV1 = TurnTimelineItemV1(
        turn_id = "turn-new",
        started_position = 2,
        latest_position = 9,
        state = "running",
        user_text = "Ship the mobile client",
    )
}
