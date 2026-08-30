package com.garive.eng.kt.memory

import java.time.OffsetDateTime
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.JsonUnquotedLiteral
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer

/** Complete non-zero bounds for one in-memory snapshot package validation. */
public data class MemorySnapshotLimits(
    public val maxEntries: Int,
    public val maxTotalBytes: Int,
    public val document: MemoryDocumentLimits,
)

/** Runtime-observed file input for the pure package validator. */
public data class MemorySnapshotFile(
    public val fileName: String,
    public val bytes: ByteArray,
    public val storageIdentity: String,
    public val regular: Boolean,
) {
    public override fun equals(other: Any?): Boolean = other is MemorySnapshotFile &&
        fileName == other.fileName && bytes.contentEquals(other.bytes) &&
        storageIdentity == other.storageIdentity && regular == other.regular
    public override fun hashCode(): Int = 31 * fileName.hashCode() + bytes.contentHashCode()
}

/** One exact exported manifest entry. */
public data class MemorySnapshotEntry(
    public val recordId: String,
    public val revisionId: String,
    public val fileName: String,
    public val authority: String,
    public val memoryType: String,
    public val memoryRole: String,
    public val scope: String,
    public val scopeOwnerId: String,
    public val lifecycle: String,
    public val sensitivity: String,
    public val contentDigest: String,
    public val documentDigest: String,
)

/** Canonical M2 snapshot manifest v1. */
public data class MemorySnapshotManifest(
    public val exportId: String,
    public val namespaceId: String,
    public val throughRevision: ULong,
    public val exportedAt: String,
    public val entries: List<MemorySnapshotEntry>,
    public val manifestDigest: String,
)

/** One generated or verified M2 snapshot package. */
public data class MemorySnapshot(
    public val manifest: MemorySnapshotManifest,
    public val manifestJson: ByteArray,
    public val documents: List<Pair<String, MemoryControlDocument>>,
) {
    public override fun equals(other: Any?): Boolean = other is MemorySnapshot &&
        manifest == other.manifest && manifestJson.contentEquals(other.manifestJson) && documents == other.documents
    public override fun hashCode(): Int = 31 * manifest.hashCode() + manifestJson.contentHashCode()
}

/** Projects current documents into one canonical M2 manifest and package. */
@OptIn(ExperimentalSerializationApi::class)
public fun projectMemorySnapshot(
    exportId: String,
    namespaceId: String,
    throughRevision: ULong,
    exportedAt: String,
    documents: List<MemoryControlDocument>,
): MemoryControlResult<MemorySnapshot> {
    if (!validHeader(exportId, namespaceId, throughRevision, exportedAt)) return invalidSnapshot()
    val pairs = documents.map { document ->
        val reference = document.recordRef as? MemoryRecordRef.Existing ?: return invalidSnapshot()
        if (document.eraseRequested) return invalidSnapshot()
        val entry = MemorySnapshotEntry(
            reference.recordId, reference.revisionId, expectedFileName(reference.recordId),
            document.authority.wireName, document.memoryType.wireName, document.memoryRole.wireName,
            document.scope.wireName, document.scopeOwnerId, document.lifecycle.wireName,
            document.sensitivity.wireName, document.contentDigest, document.documentDigest,
        )
        entry to document
    }.sortedWith(compareBy({ it.first.recordId }, { it.first.revisionId }))
    val entries = pairs.map(Pair<MemorySnapshotEntry, MemoryControlDocument>::first)
    if (!validEntries(entries)) return invalidSnapshot()
    val preimage = manifestJson(exportId, namespaceId, throughRevision, exportedAt, entries, null)
    val digest = canonicalDigest(preimage) ?: return invalidSnapshot()
    val manifest = MemorySnapshotManifest(exportId, namespaceId, throughRevision, exportedAt, entries, digest)
    val canonical = canonicalBytes(manifestJson(exportId, namespaceId, throughRevision, exportedAt, entries, digest))
        ?: return invalidSnapshot()
    return MemoryControlResult.Success(
        MemorySnapshot(manifest, canonical, pairs.map { it.first.fileName to it.second }),
    )
}

/** Validates canonical manifest bytes, exact layout, digests, aliases, and bounds. */
@OptIn(ExperimentalSerializationApi::class)
public fun parseMemorySnapshot(
    manifestJson: ByteArray,
    files: List<MemorySnapshotFile>,
    limits: MemorySnapshotLimits,
): MemoryControlResult<MemorySnapshot> {
    if (limits.maxEntries <= 0 || limits.maxTotalBytes <= 0) return invalidSnapshot()
    if (files.size > limits.maxEntries || manifestJson.size + files.sumOf { it.bytes.size } > limits.maxTotalBytes) {
        return MemoryControlResult.Failure(MemoryControlError.BOUND_EXCEEDED)
    }
    val root = runCatching { Json.parseToJsonElement(manifestJson.decodeToString()).jsonObject }.getOrNull()
        ?: return invalidSnapshot()
    if (canonicalBytes(root)?.contentEquals(manifestJson) != true || root.keys != MANIFEST_KEYS) return invalidSnapshot()
    val entriesJson = root["entries"]?.jsonArray ?: return invalidSnapshot()
    val entries = entriesJson.map { parseEntry(it.jsonObject) ?: return invalidSnapshot() }
    val exportId = root.text("export_id") ?: return invalidSnapshot()
    val namespaceId = root.text("namespace_id") ?: return invalidSnapshot()
    val through = root.ulong("through_revision") ?: return invalidSnapshot()
    val exportedAt = root.text("exported_at") ?: return invalidSnapshot()
    val digest = root.text("manifest_digest") ?: return invalidSnapshot()
    if (root.ulong("schema_version") != 1uL || !validHeader(exportId, namespaceId, through, exportedAt) ||
        !validEntries(entries) || canonicalDigest(manifestJson(exportId, namespaceId, through, exportedAt, entries, null)) != digest
    ) return invalidSnapshot()
    val names = mutableSetOf<String>()
    val folded = mutableSetOf<String>()
    val storage = mutableSetOf<String>()
    val documents = mutableListOf<Pair<String, MemoryControlDocument>>()
    for (file in files) {
        if (!file.regular || file.storageIdentity.isEmpty() || !storage.add(file.storageIdentity) ||
            !names.add(file.fileName) || !folded.add(file.fileName.lowercase()) || !validFileName(file.fileName)
        ) return invalidSnapshot()
        val document = (parseMemoryDocument(file.bytes, limits.document) as? MemoryControlResult.Success)?.value
            ?: return invalidSnapshot()
        if (!document.render().encodeToByteArray().contentEquals(file.bytes)) return invalidSnapshot()
        when (val reference = document.recordRef) {
            is MemoryRecordRef.Existing -> {
                val entry = entries.find { it.recordId == reference.recordId } ?: return invalidSnapshot()
                if (entry.revisionId != reference.revisionId || entry.fileName != file.fileName ||
                    entry.fileName != expectedFileName(reference.recordId) || !entryMatches(entry, document)
                ) return invalidSnapshot()
            }
            is MemoryRecordRef.New -> if (file.fileName != "entries/new-${reference.draftToken}.md") return invalidSnapshot()
        }
        documents += file.fileName to document
    }
    if (entries.any { it.fileName !in names }) return invalidSnapshot()
    val manifest = MemorySnapshotManifest(exportId, namespaceId, through, exportedAt, entries, digest)
    return MemoryControlResult.Success(MemorySnapshot(manifest, manifestJson, documents.sortedBy(Pair<String, MemoryControlDocument>::first)))
}

@OptIn(ExperimentalSerializationApi::class)
private fun manifestJson(
    exportId: String, namespaceId: String, through: ULong, exportedAt: String,
    entries: List<MemorySnapshotEntry>, digest: String?,
): JsonObject = JsonObject(buildMap {
    put("schema_version", JsonPrimitive(1)); put("export_id", JsonPrimitive(exportId))
    put("namespace_id", JsonPrimitive(namespaceId)); put("through_revision", JsonUnquotedLiteral(through.toString()))
    put("exported_at", JsonPrimitive(exportedAt)); put("entries", JsonArray(entries.map(MemorySnapshotEntry::json)))
    if (digest != null) put("manifest_digest", JsonPrimitive(digest))
})

private fun MemorySnapshotEntry.json(): JsonObject = JsonObject(mapOf(
    "record_id" to JsonPrimitive(recordId), "revision_id" to JsonPrimitive(revisionId),
    "file_name" to JsonPrimitive(fileName), "authority" to JsonPrimitive(authority),
    "memory_type" to JsonPrimitive(memoryType), "memory_role" to JsonPrimitive(memoryRole),
    "scope" to JsonPrimitive(scope), "scope_owner_id" to JsonPrimitive(scopeOwnerId),
    "lifecycle" to JsonPrimitive(lifecycle), "sensitivity" to JsonPrimitive(sensitivity),
    "content_digest" to JsonPrimitive(contentDigest), "document_digest" to JsonPrimitive(documentDigest),
))

private fun parseEntry(value: JsonObject): MemorySnapshotEntry? {
    if (value.keys != ENTRY_KEYS) return null
    return MemorySnapshotEntry(
        value.text("record_id") ?: return null, value.text("revision_id") ?: return null,
        value.text("file_name") ?: return null, value.text("authority") ?: return null,
        value.text("memory_type") ?: return null, value.text("memory_role") ?: return null,
        value.text("scope") ?: return null, value.text("scope_owner_id") ?: return null,
        value.text("lifecycle") ?: return null, value.text("sensitivity") ?: return null,
        value.text("content_digest") ?: return null, value.text("document_digest") ?: return null,
    )
}

private fun validEntries(entries: List<MemorySnapshotEntry>): Boolean =
    entries.zipWithNext().all { (a, b) ->
        a.recordId < b.recordId || a.recordId == b.recordId && a.revisionId < b.revisionId
    } &&
        entries.map { it.recordId }.distinct().size == entries.size &&
        listOf(entries.map { it.fileName }, entries.map { it.contentDigest }, entries.map { it.documentDigest })
            .all { it.distinct().size == it.size }

private fun entryMatches(entry: MemorySnapshotEntry, value: MemoryControlDocument): Boolean =
    entry.authority == value.authority.wireName && entry.memoryType == value.memoryType.wireName &&
        entry.memoryRole == value.memoryRole.wireName && entry.scope == value.scope.wireName &&
        entry.scopeOwnerId == value.scopeOwnerId && entry.lifecycle == value.lifecycle.wireName &&
        entry.sensitivity == value.sensitivity.wireName && entry.contentDigest == value.contentDigest &&
        entry.documentDigest == value.documentDigest

private fun validHeader(export: String, namespace: String, revision: ULong, at: String): Boolean =
    export.isNotEmpty() && namespace.isNotEmpty() && revision > 0uL && runCatching { OffsetDateTime.parse(at) }.isSuccess
private fun expectedFileName(record: String): String = "entries/${sha256(record.encodeToByteArray())}.md"
private fun validFileName(value: String): Boolean = value.startsWith("entries/") && value.endsWith(".md") &&
    ".." !in value && '\\' !in value && value.count { it == '/' } == 1
private fun canonicalBytes(value: JsonObject): ByteArray? = runCatching { JsonCanonicalizer(value.toString()).encodedUTF8 }.getOrNull()
private fun canonicalDigest(value: JsonObject): String? = canonicalBytes(value)?.let(::sha256)
private fun JsonObject.text(key: String): String? = get(key)?.jsonPrimitive?.takeIf(JsonPrimitive::isString)?.content
private fun JsonObject.ulong(key: String): ULong? = get(key)?.jsonPrimitive?.content?.toULongOrNull()
private fun invalidSnapshot(): MemoryControlResult.Failure = MemoryControlResult.Failure(MemoryControlError.INVALID_SNAPSHOT)

private val MANIFEST_KEYS = setOf("schema_version", "export_id", "namespace_id", "through_revision", "exported_at", "entries", "manifest_digest")
private val ENTRY_KEYS = setOf("record_id", "revision_id", "file_name", "authority", "memory_type", "memory_role", "scope", "scope_owner_id", "lifecycle", "sensitivity", "content_digest", "document_digest")
