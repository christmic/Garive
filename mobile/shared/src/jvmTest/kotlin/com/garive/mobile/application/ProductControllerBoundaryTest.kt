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
    public fun mismatchedMutationResultPreservesExactRetryIdentity(): Unit {
        var state = ready(listOf("session-a"), "session-a")
        state = reduceApp(state, AppIntent.EditDraft("session-a", "hello")).state
        val started = reduceApp(state, AppIntent.SubmitDraft("session-a", "command-a", digest))
        val effect = started.effects.first { it.kind == EffectKind.START_TURN }
        val rejected = reduceApp(started.state, AppIntent.EffectResult(effect.effectId, effect.generation,
            effect.sessionId, effect.requestDigest, AppEffectPayload.PreferencesSaved))
        assertEquals(AppError(AppErrorKind.COMMAND_UNKNOWN, "mutation_outcome_unknown"), rejected.state.notice)
        assertEquals("command-a", rejected.state.pending.single().commandId)
        assertEquals(PendingStatus.UNKNOWN, rejected.state.pending.single().status)
    }

    @Test
    public fun oneCrashSafeMutationGloballyAndUtf8BytesAreEnforced(): Unit {
        val limits = ControllerLimits(3, 2)
        var state = ready(listOf("session-a"), "session-a")
        state = reduceApp(state, AppIntent.EditDraft("session-a", "ok"), limits).state
        state = reduceApp(state, AppIntent.SubmitDraft("session-a", "command-a", digest), limits).state
        val second = reduceApp(state, AppIntent.SubmitDraft("session-a", "command-b", "b".repeat(64)), limits)
        assertEquals("command_not_admitted", second.state.notice?.code)
        val oversized = reduceApp(second.state, AppIntent.EditDraft("session-a", "🦀"), limits)
        assertEquals("draft_too_large", oversized.state.notice?.code)
        val fixtureSized = reduceApp(ready(listOf("session-a"), "session-a"),
            AppIntent.EditDraft("session-a", "this draft is deliberately over thirty-two bytes"), ControllerLimits(32, 3))
        assertEquals("draft_too_large", fixtureSized.state.notice?.code)
    }

    @Test
    public fun crashInterruptedMutationRestoresAsUnknownWithExactIdentity(): Unit {
        val booted = reduceApp(initialAppViewState(), AppIntent.Boot)
        val effect = booted.effects.first { it.kind == EffectKind.LOAD_PREFERENCES }
        val pending = PendingCommand(CommandKind.START_TURN, "command-a", digest, 3,
            "session-a", status = PendingStatus.PENDING)
        val restored = reduceApp(booted.state, AppIntent.EffectResult(effect.effectId,
            effect.generation, result = AppEffectPayload.PreferencesLoaded(null,
                listOf(Draft("session-a", "restore me")), pending)))
        assertEquals("command-a", restored.state.pending.single().commandId)
        assertEquals(PendingStatus.UNKNOWN, restored.state.pending.single().status)
        val retried = reduceApp(restored.state, AppIntent.RetryPending("session-a"))
        assertEquals("command-a", retried.effects.single().commandId)
        assertEquals(digest, retried.effects.single().requestDigest)
        assertEquals("restore me", retried.effects.single().text)
    }

    @Test
    public fun preferenceWritesAreCoalescedWithoutLosingLatestState(): Unit {
        val first = reduceApp(ready(listOf("session-a"), "session-a"), AppIntent.EditDraft("session-a", "first"))
        val second = reduceApp(first.state, AppIntent.EditDraft("session-a", "latest"))
        assertTrue(second.effects.isEmpty())
        assertEquals(1, second.state.outstanding.count { it.kind == EffectKind.SAVE_PREFERENCES })
        val save = first.effects.first()
        val completed = reduceApp(second.state, AppIntent.EffectResult(save.effectId, save.generation,
            result = AppEffectPayload.PreferencesSaved))
        assertEquals(listOf(EffectKind.SAVE_PREFERENCES), completed.effects.map { it.kind })
        assertEquals(listOf(Draft("session-a", "latest")), completed.state.drafts)
        assertFalse(completed.state.preferenceDirty)
    }

    @Test
    public fun pendingRecordRoundTripsAndCorruptionClearsOnlyPending(): Unit = runBlocking {
        val port = MemoryPort()
        val adapter = JsonPreferenceAdapter(port, PreferenceLimits(1_024, 4, 128, 256))
        adapter.savePending(PendingCommand(CommandKind.START_TURN, "command-a", digest, 3,
            "session-a", status = PendingStatus.UNKNOWN))
        assertEquals("command-a", adapter.load().pending?.commandId)
        adapter.savePending(PendingCommand(CommandKind.CREATE_SESSION, "command-create", digest, 3,
            status = PendingStatus.UNKNOWN, definitionId = "definition-a"))
        assertEquals("definition-a", adapter.load().pending?.definitionId)
        adapter.savePending(PendingCommand(CommandKind.CANCEL_TURN, "command-cancel", digest, 3,
            "session-a", "turn-a", PendingStatus.UNKNOWN, afterPosition = 0))
        assertEquals(0, adapter.load().pending?.afterPosition)
        adapter.savePending(PendingCommand(CommandKind.CONTINUE_TURN, "command-continue", digest, 3,
            "session-a", "turn-a", PendingStatus.UNKNOWN, suspensionId = "suspension-a", sessionVersion = 4,
            responseSchemaDigest = digest, continuationValueKind = ContinuationValueKind.JSON_BOOLEAN))
        assertEquals("suspension-a", adapter.load().pending?.suspensionId)
        assertEquals(ContinuationValueKind.JSON_BOOLEAN, adapter.load().pending?.continuationValueKind)
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
