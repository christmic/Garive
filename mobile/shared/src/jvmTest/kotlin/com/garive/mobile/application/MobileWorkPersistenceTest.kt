package com.garive.mobile.application

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

public class MobileWorkPersistenceTest {
    @Test
    public fun everyPendingShapeRoundTripsWithExactPayload(): Unit {
        val values = listOf(
            PendingOperation.CreateAndStart("definition-a", "create input", "create-a", "start-a", createdAtEpochMs = 1),
            PendingOperation.CreateAndStart(
                "definition-a", "created input", "create-b", "start-b", "session-a", createdAtEpochMs = 2,
            ),
            PendingOperation.Start("session-a", "start input", "command-a", 3),
            PendingOperation.Cancel("session-a", "turn-a", 9, "command-b", 4),
            PendingOperation.Continue(
                "session-a", "turn-a", "suspension-a", 3, "true", true, "command-c", 5,
            ),
        )

        values.forEach { expected ->
            val persistence = MemoryPersistence()
            savePending(persistence, expected)

            val restored = restorePending(persistence, 4_096)

            assertEquals(expected.publicValue(), restored?.publicValue())
            assertEquals(expected.payload(), restored?.payload())
            assertEquals(expected.createdAtEpochMs, restored?.createdAtEpochMs)
        }
    }

    @Test
    public fun payloadTamperingRejectsAndClearsBothValues(): Unit {
        val persistence = MemoryPersistence()
        savePending(
            persistence,
            PendingOperation.Start("session-a", "original", "command-a", 1),
        )
        persistence.payload = "tampered"

        assertNull(restorePending(persistence, 4_096))
        assertNull(persistence.record)
        assertNull(persistence.payload)
    }

    private class MemoryPersistence : MobileWorkPersistence {
        var record: String? = null
        var payload: String? = null
        override fun readPendingRecord(): String? = record
        override fun writePendingRecord(value: String?) { record = value }
        override fun readPendingPayload(): String? = payload
        override fun writePendingPayload(value: String?) { payload = value }
    }
}
