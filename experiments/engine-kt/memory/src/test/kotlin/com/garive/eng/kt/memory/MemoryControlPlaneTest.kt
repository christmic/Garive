package com.garive.eng.kt.memory

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue

class MemoryControlPlaneTest {
    private val limits = assertIs<MemoryControlResult.Success<MemoryDocumentLimits>>(
        MemoryDocumentLimits.create(2_048, 1_024, 64),
    ).value

    @Test
    fun `exact identities round trip and CRLF normalizes`() {
        val parsed = success(DOCUMENT.replace("\n", "\r\n"))
        assertEquals(MemoryRecordRef.Existing("mem-01", "rev-04"), parsed.recordRef)
        assertEquals(MemoryKind.PREFERENCE, parsed.memoryRole)
        assertEquals("agent-01", parsed.scopeOwnerId)
        assertEquals(DOCUMENT, parsed.render())
        assertEquals(64, parsed.contentDigest.length)
        assertEquals(64, parsed.documentDigest.length)
    }

    @Test
    fun `new and erasure forms are exact`() {
        val added = success(DOCUMENT.replace("existing.bWVtLTAx.cmV2LTA0", "new.draft_1"))
        assertEquals(MemoryRecordRef.New("draft_1"), added.recordRef)
        val erased = success(DOCUMENT.replace("sensitivity: ordinary\n", "sensitivity: ordinary\nerase: true\n"))
        assertTrue(erased.eraseRequested)
    }

    @Test
    fun `aliases malformed identities and bounds fail closed`() {
        listOf(
            DOCUMENT.replaceFirst("record_ref", "unknown"),
            DOCUMENT.replaceFirst("bWVtLTAx", "bWVtLTAx="),
            DOCUMENT.replaceFirst("cmV2LTA0", ""),
            DOCUMENT.replaceFirst("preference", "future"),
            DOCUMENT.replaceFirst("YWdlbnQtMDE", "YWdlbnQtMDE="),
            DOCUMENT.replaceFirst("sensitivity: ordinary\n", "sensitivity: ordinary\nerase: false\n"),
        ).forEach {
            assertEquals(
                MemoryControlError.INVALID_SNAPSHOT,
                assertIs<MemoryControlResult.Failure>(parseMemoryDocument(it.encodeToByteArray(), limits)).error,
            )
        }
        val tiny = assertIs<MemoryControlResult.Success<MemoryDocumentLimits>>(
            MemoryDocumentLimits.create(8, 8, 8),
        ).value
        assertEquals(
            MemoryControlError.BOUND_EXCEEDED,
            assertIs<MemoryControlResult.Failure>(parseMemoryDocument(DOCUMENT.encodeToByteArray(), tiny)).error,
        )
    }

    private fun success(document: String): MemoryControlDocument =
        assertIs<MemoryControlResult.Success<MemoryControlDocument>>(
            parseMemoryDocument(document.encodeToByteArray(), limits),
        ).value

    private companion object {
        const val DOCUMENT: String = "---\nschema_version: 1\nrecord_ref: existing.bWVtLTAx.cmV2LTA0\nauthority: user_declared\nmemory_type: semantic\nmemory_role: preference\nscope: agent_instance\nscope_owner_b64: YWdlbnQtMDE\nlifecycle: active\nsensitivity: ordinary\n---\nPrefer concise status updates.\n"
    }
}
