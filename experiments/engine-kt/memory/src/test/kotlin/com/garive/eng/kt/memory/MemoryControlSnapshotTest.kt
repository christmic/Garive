package com.garive.eng.kt.memory

import java.util.Base64
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertIs

class MemoryControlSnapshotTest {
    @Test
    fun `projection and validation match Rust canonical manifest`() {
        val projected = success(projectMemorySnapshot(
            "export-1", "namespace-1", 7uL, "2026-08-30T12:00:00Z",
            listOf(document("mem-b", "rev-b", "second"), document("mem-a", "rev-a", "first")),
        ))
        assertEquals("mem-a", projected.manifest.entries.first().recordId)
        assertEquals("79bc86a14a215dd48e41496ce418ffb8719025f79b095a5da055faab8a4db341", projected.manifest.manifestDigest)
        val files = projected.documents.mapIndexed { index, (name, document) ->
            MemorySnapshotFile(name, document.render().encodeToByteArray(), "storage-$index", true)
        } + MemorySnapshotFile(
            "entries/new-draft_1.md", newDocument("draft_1", "third").render().encodeToByteArray(),
            "storage-new", true,
        )
        val verified = success(parseMemorySnapshot(projected.manifestJson, files, limits()))
        assertEquals(3, verified.documents.size)
        assertContentEquals(projected.manifestJson, verified.manifestJson)
    }

    @Test
    fun `aliases traversal noncanonical JSON and bounds fail closed`() {
        val projected = success(projectMemorySnapshot(
            "export-1", "namespace-1", 7uL, "2026-08-30T12:00:00Z",
            listOf(document("mem-a", "rev-a", "first")),
        ))
        val original = projected.documents.mapIndexed { index, (name, document) ->
            MemorySnapshotFile(name, document.render().encodeToByteArray(), "storage-$index", true)
        }
        assertInvalid(projected.manifestJson, listOf(original.single().copy(regular = false)))
        assertInvalid(projected.manifestJson, listOf(original.single().copy(fileName = "entries/../escape.md")))
        assertInvalid(" ${projected.manifestJson.decodeToString()}".encodeToByteArray(), original)
        val alias = original + MemorySnapshotFile(
            "entries/new-draft.md", newDocument("draft", "new").render().encodeToByteArray(),
            original.single().storageIdentity, true,
        )
        assertInvalid(projected.manifestJson, alias)
        assertEquals(
            MemoryControlError.BOUND_EXCEEDED,
            failure(parseMemorySnapshot(projected.manifestJson, original, MemorySnapshotLimits(1, 8, documentLimits()))),
        )
    }

    private fun limits(): MemorySnapshotLimits = MemorySnapshotLimits(16, 64 * 1024, documentLimits())
    private fun documentLimits(): MemoryDocumentLimits =
        assertIs<MemoryControlResult.Success<MemoryDocumentLimits>>(
            MemoryDocumentLimits.create(4_096, 2_048, 128),
        ).value

    private fun assertInvalid(manifest: ByteArray, files: List<MemorySnapshotFile>) {
        assertEquals(MemoryControlError.INVALID_SNAPSHOT, failure(parseMemorySnapshot(manifest, files, limits())))
    }

    private fun <T> success(value: MemoryControlResult<T>): T = assertIs<MemoryControlResult.Success<T>>(value).value
    private fun failure(value: MemoryControlResult<MemorySnapshot>): MemoryControlError =
        assertIs<MemoryControlResult.Failure>(value).error

    private fun document(record: String, revision: String, content: String): MemoryControlDocument =
        parse("existing.${encoded(record)}.${encoded(revision)}", content)
    private fun newDocument(token: String, content: String): MemoryControlDocument = parse("new.$token", content)
    private fun parse(reference: String, content: String): MemoryControlDocument {
        val value = "---\nschema_version: 1\nrecord_ref: $reference\nauthority: user_declared\nmemory_type: semantic\nmemory_role: preference\nscope: agent_instance\nscope_owner_b64: YWdlbnQtMDE\nlifecycle: active\nsensitivity: ordinary\n---\n$content\n"
        return success(parseMemoryDocument(value.encodeToByteArray(), documentLimits()))
    }
    private fun encoded(value: String): String =
        Base64.getUrlEncoder().withoutPadding().encodeToString(value.encodeToByteArray())
}
