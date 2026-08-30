package com.garive.eng.kt.memory

import java.util.Base64
import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class MemoryControlPlannerTest {
    @Test
    fun `all variants have canonical order counts and Rust-identical digest`() {
        val originals = listOf(
            document("mem-a", "rev-a", "user_declared", "active", false, "old a"),
            document("mem-b", "rev-b", "agent_learned", "active", false, "old b"),
            document("mem-c", "rev-c", "user_declared", "active", false, "old c"),
            document("mem-d", "rev-d", "user_declared", "active", false, "old d"),
            document("mem-e", "rev-e", "user_declared", "active", false, "old e"),
        )
        val current = originals.mapIndexed { index, value ->
            current(value, if (index == 1) MemoryAuthority.AGENT_LEARNED else MemoryAuthority.USER_DECLARED)
        }
        val documents = listOf(
            newDocument("draft-1", "new value"),
            document("mem-e", "rev-e", "user_declared", "active", false, "old e"),
            document("mem-d", "rev-d", "user_declared", "active", true, "old d"),
            document("mem-c", "rev-c", "user_declared", "archived", false, "old c"),
            document("mem-b", "rev-b", "user_declared", "active", false, "edited b"),
            document("mem-a", "rev-a", "user_declared", "active", false, "edited a"),
        )
        val allocations = listOf(
            MemoryIdentityAllocation.Supersede("mem-a", "rev-a2"),
            MemoryIdentityAllocation.Supersede("mem-b", "rev-b2"),
            MemoryIdentityAllocation.Add("draft-1", "mem-f", "rev-f1"),
        )
        val plan = assertIs<MemoryControlResult.Success<MemoryImportPlan>>(
            prepareMemoryImport(
                "export-1", "namespace-1", 7uL, DIGEST, 7uL, documents, current,
                listOf(MemoryAuthorizedScope(MemoryScopeClass.AGENT_INSTANCE, "agent-01")),
                allocations,
            ),
        ).value
        assertEquals(listOf(1uL, 2uL, 1uL, 1uL), listOf(
            plan.addCount, plan.supersedeCount, plan.archiveCount, plan.eraseCount,
        ))
        assertEquals(fixture()["plan_vector"]!!.jsonObject["plan_digest"]!!.jsonPrimitive.content, plan.planDigest)
        assertTrue(plan.operations[0] is MemoryImportOperation.Supersede)
        assertTrue(plan.operations[2] is MemoryImportOperation.Archive)
        assertTrue(plan.operations[3] is MemoryImportOperation.Erase)
        assertTrue(plan.operations[4] is MemoryImportOperation.Add)
    }

    @Test
    fun `stale and metadata changes fail closed`() {
        val original = document("mem-a", "rev-a", "user_declared", "active", false, "old")
        val current = listOf(current(original, MemoryAuthority.USER_DECLARED))
        assertEquals(
            MemoryControlError.STALE_SNAPSHOT,
            failure(prepareMemoryImport("export-1", "namespace-1", 7uL, DIGEST, 8uL, listOf(original), current, emptyList(), emptyList())),
        )
        val widened = parse(documentText(
            "existing.bWVtLWE.cmV2LWE", "user_declared", "active", false, "old",
        ).replace("scope: agent_instance", "scope: platform"))
        assertEquals(
            MemoryControlError.FORBIDDEN_CHANGE,
            failure(prepareMemoryImport("export-1", "namespace-1", 7uL, DIGEST, 7uL, listOf(widened), current, emptyList(), emptyList())),
        )
    }

    private fun failure(value: MemoryControlResult<MemoryImportPlan>): MemoryControlError =
        assertIs<MemoryControlResult.Failure>(value).error

    private fun current(value: MemoryControlDocument, authority: MemoryAuthority): MemoryCurrentEntry {
        val reference = assertIs<MemoryRecordRef.Existing>(value.recordRef)
        return MemoryCurrentEntry(
            reference.recordId, reference.revisionId, authority, value.memoryType, value.memoryRole,
            value.scope, value.scopeOwnerId, value.lifecycle, value.sensitivity, value.contentDigest,
        )
    }

    private fun document(
        record: String, revision: String, authority: String, lifecycle: String,
        erase: Boolean, content: String,
    ): MemoryControlDocument = parse(documentText(
        "existing.${encoded(record)}.${encoded(revision)}", authority, lifecycle, erase, content,
    ))

    private fun newDocument(token: String, content: String): MemoryControlDocument =
        parse(documentText("new.$token", "user_declared", "active", false, content))

    private fun parse(value: String): MemoryControlDocument {
        val limits = assertIs<MemoryControlResult.Success<MemoryDocumentLimits>>(
            MemoryDocumentLimits.create(4_096, 2_048, 128),
        ).value
        return assertIs<MemoryControlResult.Success<MemoryControlDocument>>(
            parseMemoryDocument(value.encodeToByteArray(), limits),
        ).value
    }

    private fun documentText(
        recordRef: String, authority: String, lifecycle: String, erase: Boolean, content: String,
    ): String = "---\nschema_version: 1\nrecord_ref: $recordRef\nauthority: $authority\nmemory_type: semantic\nmemory_role: preference\nscope: agent_instance\nscope_owner_b64: YWdlbnQtMDE\nlifecycle: $lifecycle\nsensitivity: ordinary\n${if (erase) "erase: true\n" else ""}---\n$content\n"

    private fun encoded(value: String): String =
        Base64.getUrlEncoder().withoutPadding().encodeToString(value.encodeToByteArray())

    private fun fixture() = Json.parseToJsonElement(
        Path.of(System.getProperty("garive.repo.root"))
            .resolve("spec/fixtures/agent/memory-control-plane-v1.json").readText(),
    ).jsonObject

    private companion object {
        const val DIGEST: String = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    }
}
