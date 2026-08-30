package com.garive.mobile.application

import com.garive.mobile.model.*
import com.garive.mobile.preferences.*
import kotlin.test.*
import kotlinx.coroutines.runBlocking

public class ProductControllerBoundaryTest {
    private val digest: String = "a".repeat(64)

    @Test
    public fun navigationPreservesInflightMutationAndInvalidatesOnlyReads(): Unit {
        var state = ready(listOf("session-a", "session-b"), "session-a")
        state = reduceApp(state, AppIntent.EditDraft("session-a", "hello")).state
        val started = reduceApp(state, AppIntent.SubmitDraft("session-a", "command-a", digest))
        val navigated = reduceApp(started.state, AppIntent.SelectSession("session-b"))
        assertEquals(listOf("command-a"), navigated.state.pending.map { it.commandId })
        assertTrue(navigated.state.outstanding.any { it.kind == EffectKind.START_TURN })
        assertTrue(navigated.state.outstanding.any { it.kind == EffectKind.LOAD_TIMELINE && it.sessionId == "session-b" })
    }

    @Test
    public fun forgedCorrelationCoordinatesCannotCompleteMutation(): Unit {
        var state = ready(listOf("session-a"), "session-a")
        state = reduceApp(state, AppIntent.EditDraft("session-a", "hello")).state
        val started = reduceApp(state, AppIntent.SubmitDraft("session-a", "command-a", digest))
        val effect = started.effects.first()
        val forged = reduceApp(started.state, AppIntent.EffectResult(effect.effectId, effect.generation,
            "session-other", effect.requestDigest, AppEffectPayload.CommandSucceeded("session-a", "turn-a", 4)))
        assertSame(started.state, forged.state)
        assertTrue(forged.effects.isEmpty())
    }

    @Test
    public fun oneMutationPerSessionAndUtf8BytesAreEnforced(): Unit {
        val limits = ControllerLimits(3, 2)
        var state = ready(listOf("session-a"), "session-a")
        state = reduceApp(state, AppIntent.EditDraft("session-a", "ok"), limits).state
        state = reduceApp(state, AppIntent.SubmitDraft("session-a", "command-a", digest), limits).state
        val second = reduceApp(state, AppIntent.CancelTurn("session-a", "turn-a", "command-b", "b".repeat(64)), limits)
        assertEquals("command_not_admitted", second.state.notice?.code)
        val oversized = reduceApp(second.state, AppIntent.EditDraft("session-a", "🦀"), limits)
        assertEquals("draft_too_large", oversized.state.notice?.code)
    }

    @Test
    public fun pendingRecordRoundTripsAndCorruptionClearsOnlyPending(): Unit = runBlocking {
        val port = MemoryPort()
        val adapter = JsonPreferenceAdapter(port, PreferenceLimits(1_024, 4, 128, 256))
        adapter.savePending(PendingCommand(CommandKind.START_TURN, "command-a", digest, 3,
            "session-a", status = PendingStatus.UNKNOWN))
        assertEquals("command-a", adapter.load().pending?.commandId)
        port.pending = "{\"schema_version\":2}".encodeToByteArray()
        val loaded = adapter.load()
        assertTrue(loaded.reset); assertNull(loaded.pending); assertNull(port.pending)
    }

    private fun ready(ids: List<String>, selected: String): AppViewState = initialAppViewState().copy(
        shell = ShellState.READY, generation = 1, sessions = ids.map(::SessionItem), selectedSessionId = selected,
    )

    private class MemoryPort : PreferenceBytesPort {
        var preferences: ByteArray? = null
        var pending: ByteArray? = null
        override suspend fun readPreferences(): ByteArray? = preferences
        override suspend fun writePreferences(value: ByteArray): Unit { preferences = value }
        override suspend fun readPendingCommand(): ByteArray? = pending
        override suspend fun writePendingCommand(value: ByteArray?): Unit { pending = value }
    }
}
