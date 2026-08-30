package com.garive.eng.kt.memory

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.JsonUnquotedLiteral
import org.erdtman.jcs.JsonCanonicalizer

/** Produces one authority-safe, deterministic M2 import plan without I/O. */
@OptIn(ExperimentalSerializationApi::class)
public fun prepareMemoryImport(
    exportId: String,
    namespaceId: String,
    throughRevision: ULong,
    inputManifestDigest: String,
    currentRepositoryRevision: ULong,
    documents: List<MemoryControlDocument>,
    current: List<MemoryCurrentEntry>,
    authorizedScopes: List<MemoryAuthorizedScope>,
    allocations: List<MemoryIdentityAllocation>,
): MemoryControlResult<MemoryImportPlan> {
    if (exportId.isEmpty() || namespaceId.isEmpty() || throughRevision == 0uL ||
        !validDigest(inputManifestDigest)
    ) return controlFailure(MemoryControlError.INVALID_SNAPSHOT)
    if (throughRevision != currentRepositoryRevision) {
        return controlFailure(MemoryControlError.STALE_SNAPSHOT)
    }
    if (!strictlyOrdered(current.map(MemoryCurrentEntry::recordId)) ||
        !strictlyOrdered(authorizedScopes)
    ) return controlFailure(MemoryControlError.INVALID_SNAPSHOT)
    val references = documents.map {
        when (val reference = it.recordRef) {
            is MemoryRecordRef.Existing -> "existing:${reference.recordId}"
            is MemoryRecordRef.New -> "new:${reference.draftToken}"
        }
    }
    if (references.distinct().size != references.size) {
        return controlFailure(MemoryControlError.INVALID_SNAPSHOT)
    }

    val operations = mutableListOf<MemoryImportOperation>()
    for (document in documents) {
        when (val reference = document.recordRef) {
            is MemoryRecordRef.New -> planAdd(
                document, reference.draftToken, authorizedScopes, allocations, operations,
            )?.let { return controlFailure(it) }
            is MemoryRecordRef.Existing -> {
                val entry = current.find { it.recordId == reference.recordId }
                    ?: return controlFailure(MemoryControlError.STALE_SNAPSHOT)
                if (entry.revisionId != reference.revisionId) {
                    return controlFailure(MemoryControlError.STALE_SNAPSHOT)
                }
                planExisting(document, entry, allocations, operations)?.let {
                    return controlFailure(it)
                }
            }
        }
    }
    operations.sortWith(compareBy(MemoryImportOperation::recordId, MemoryImportOperation::rank))
    if (operations.zipWithNext().any { (left, right) -> left.recordId == right.recordId }) {
        return controlFailure(MemoryControlError.INVALID_SNAPSHOT)
    }
    val addCount = operations.count { it is MemoryImportOperation.Add }.toULong()
    val supersedeCount = operations.count { it is MemoryImportOperation.Supersede }.toULong()
    val archiveCount = operations.count { it is MemoryImportOperation.Archive }.toULong()
    val eraseCount = operations.count { it is MemoryImportOperation.Erase }.toULong()
    val preimage = JsonObject(
        mapOf(
            "schema_version" to JsonPrimitive(1),
            "export_id" to JsonPrimitive(exportId),
            "namespace_id" to JsonPrimitive(namespaceId),
            "through_revision" to JsonUnquotedLiteral(throughRevision.toString()),
            "input_manifest_digest" to JsonPrimitive(inputManifestDigest),
            "expected_repository_revision" to JsonUnquotedLiteral(currentRepositoryRevision.toString()),
            "operations" to JsonArray(operations.map(MemoryImportOperation::json)),
            "add_count" to JsonUnquotedLiteral(addCount.toString()),
            "supersede_count" to JsonUnquotedLiteral(supersedeCount.toString()),
            "archive_count" to JsonUnquotedLiteral(archiveCount.toString()),
            "erase_count" to JsonUnquotedLiteral(eraseCount.toString()),
        ),
    )
    val digest = runCatching { sha256(JsonCanonicalizer(preimage.toString()).encodedUTF8) }
        .getOrElse { return controlFailure(MemoryControlError.INVALID_SNAPSHOT) }
    return MemoryControlResult.Success(
        MemoryImportPlan(
            exportId, namespaceId, throughRevision, inputManifestDigest,
            currentRepositoryRevision, operations, addCount, supersedeCount,
            archiveCount, eraseCount, digest,
        ),
    )
}

private fun planAdd(
    document: MemoryControlDocument,
    draftToken: String,
    authorizedScopes: List<MemoryAuthorizedScope>,
    allocations: List<MemoryIdentityAllocation>,
    operations: MutableList<MemoryImportOperation>,
): MemoryControlError? {
    if (document.authority != MemoryAuthority.USER_DECLARED ||
        document.lifecycle != HypothesisState.ACTIVE || document.eraseRequested ||
        authorizedScopes.none { it.scope == document.scope && it.ownerId == document.scopeOwnerId }
    ) return MemoryControlError.FORBIDDEN_CHANGE
    val matches = allocations.filterIsInstance<MemoryIdentityAllocation.Add>()
        .filter { it.draftToken == draftToken }
    if (matches.size != 1) return MemoryControlError.INVALID_SNAPSHOT
    val allocation = matches.single()
    operations += MemoryImportOperation.Add(
        draftToken, allocation.recordId, allocation.revisionId, true, document.documentDigest,
    )
    return null
}

private fun planExisting(
    document: MemoryControlDocument,
    entry: MemoryCurrentEntry,
    allocations: List<MemoryIdentityAllocation>,
    operations: MutableList<MemoryImportOperation>,
): MemoryControlError? {
    if (document.memoryType != entry.memoryType || document.memoryRole != entry.memoryRole ||
        document.scope != entry.scope || document.scopeOwnerId != entry.scopeOwnerId ||
        document.sensitivity != entry.sensitivity
    ) return MemoryControlError.FORBIDDEN_CHANGE
    when {
        document.eraseRequested -> {
            if (document.contentDigest != entry.contentDigest || document.lifecycle != entry.lifecycle ||
                document.authority != entry.authority ||
                entry.authority == MemoryAuthority.ORGANISATION_PUBLISHED
            ) return MemoryControlError.FORBIDDEN_CHANGE
            operations += MemoryImportOperation.Erase(
                entry.recordId, entry.revisionId, document.documentDigest,
            )
        }
        document.lifecycle != entry.lifecycle -> {
            if (document.lifecycle != HypothesisState.ARCHIVED ||
                document.contentDigest != entry.contentDigest || document.authority != entry.authority ||
                entry.authority == MemoryAuthority.ORGANISATION_PUBLISHED
            ) return MemoryControlError.FORBIDDEN_CHANGE
            operations += MemoryImportOperation.Archive(
                entry.recordId, entry.revisionId, document.documentDigest,
            )
        }
        document.contentDigest != entry.contentDigest -> {
            if (entry.authority == MemoryAuthority.ORGANISATION_PUBLISHED ||
                document.authority != MemoryAuthority.USER_DECLARED
            ) return MemoryControlError.FORBIDDEN_CHANGE
            val matches = allocations.filterIsInstance<MemoryIdentityAllocation.Supersede>()
                .filter { it.recordId == entry.recordId }
            if (matches.size != 1) return MemoryControlError.INVALID_SNAPSHOT
            operations += MemoryImportOperation.Supersede(
                entry.recordId, entry.revisionId, matches.single().revisionId,
                MemoryAuthority.USER_DECLARED, document.documentDigest,
                entry.revisionId.takeIf { entry.authority == MemoryAuthority.AGENT_LEARNED },
            )
        }
        document.authority != entry.authority -> return MemoryControlError.FORBIDDEN_CHANGE
    }
    return null
}

private val MemoryImportOperation.rank: Int get() = when (this) {
    is MemoryImportOperation.Add -> 0
    is MemoryImportOperation.Supersede -> 1
    is MemoryImportOperation.Archive -> 2
    is MemoryImportOperation.Erase -> 3
}

private fun MemoryImportOperation.json(): JsonObject = when (this) {
    is MemoryImportOperation.Add -> jsonObject(
        "operation" to JsonPrimitive("add"),
        "source_draft_token" to JsonPrimitive(sourceDraftToken),
        "record_id" to JsonPrimitive(recordId),
        "revision_id" to JsonPrimitive(revisionId),
        "expected_absent" to JsonPrimitive(expectedAbsent),
        "document_digest" to JsonPrimitive(documentDigest),
    )
    is MemoryImportOperation.Supersede -> jsonObject(
        "operation" to JsonPrimitive("supersede"),
        "record_id" to JsonPrimitive(recordId),
        "expected_active_revision_id" to JsonPrimitive(expectedActiveRevisionId),
        "new_revision_id" to JsonPrimitive(newRevisionId),
        "authority" to JsonPrimitive(authority.wireName),
        "document_digest" to JsonPrimitive(documentDigest),
        *listOfNotNull(
            supersedesLearnedRevisionId?.let {
                "supersedes_learned_revision_id" to JsonPrimitive(it)
            },
        ).toTypedArray(),
    )
    is MemoryImportOperation.Archive -> jsonObject(
        "operation" to JsonPrimitive("archive"),
        "record_id" to JsonPrimitive(recordId),
        "expected_active_revision_id" to JsonPrimitive(expectedActiveRevisionId),
        "document_digest" to JsonPrimitive(documentDigest),
    )
    is MemoryImportOperation.Erase -> jsonObject(
        "operation" to JsonPrimitive("erase"),
        "record_id" to JsonPrimitive(recordId),
        "expected_active_revision_id" to JsonPrimitive(expectedActiveRevisionId),
        "document_digest" to JsonPrimitive(documentDigest),
    )
}

private fun jsonObject(vararg values: Pair<String, JsonPrimitive>): JsonObject = JsonObject(mapOf(*values))

private fun <T : Comparable<T>> strictlyOrdered(values: List<T>): Boolean =
    values.zipWithNext().all { (left, right) -> left < right }

private fun controlFailure(error: MemoryControlError): MemoryControlResult.Failure =
    MemoryControlResult.Failure(error)
