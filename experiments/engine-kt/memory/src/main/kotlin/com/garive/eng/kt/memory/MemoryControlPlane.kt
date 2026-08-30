package com.garive.eng.kt.memory

import java.util.Base64

/** Stable M2 validation or planning failure. */
public enum class MemoryControlError(public val wireName: String) {
    INVALID_LIMITS("memory_snapshot_invalid"),
    BOUND_EXCEEDED("memory_control_bound_exceeded"),
    INVALID_SNAPSHOT("memory_snapshot_invalid"),
    FORBIDDEN_CHANGE("memory_import_forbidden_change"),
    STALE_SNAPSHOT("stale_memory_snapshot"),
}

/** Result of a pure M2 validation or planning operation. */
public sealed interface MemoryControlResult<out T> {
    /** Successful validated value. */
    public data class Success<T>(public val value: T) : MemoryControlResult<T>

    /** Stable validation or planning failure. */
    public data class Failure(public val error: MemoryControlError) : MemoryControlResult<Nothing>
}

/** Explicit bounds for one M2 entry document. */
@ConsistentCopyVisibility
public data class MemoryDocumentLimits private constructor(
    public val maxDocumentBytes: Int,
    public val maxContentBytes: Int,
    public val maxIdBytes: Int,
) {
    public companion object {
        /** Rejects zero bounds and constructs document limits. */
        public fun create(
            maxDocumentBytes: Int,
            maxContentBytes: Int,
            maxIdBytes: Int,
        ): MemoryControlResult<MemoryDocumentLimits> =
            if (maxDocumentBytes <= 0 || maxContentBytes <= 0 || maxIdBytes <= 0) {
                MemoryControlResult.Failure(MemoryControlError.INVALID_LIMITS)
            } else {
                MemoryControlResult.Success(
                    MemoryDocumentLimits(maxDocumentBytes, maxContentBytes, maxIdBytes),
                )
            }
    }
}

/** Exact existing M0 identity or bounded new-entry correlation token. */
public sealed interface MemoryRecordRef {
    /** Existing immutable record and revision identities. */
    public data class Existing(
        public val recordId: String,
        public val revisionId: String,
    ) : MemoryRecordRef

    /** User-created document awaiting Runtime identity allocation. */
    public data class New(public val draftToken: String) : MemoryRecordRef
}

/** One normalized, user-auditable M2 Memory entry. */
public data class MemoryControlDocument(
    public val recordRef: MemoryRecordRef,
    public val authority: MemoryAuthority,
    public val memoryType: MemoryType,
    public val memoryRole: MemoryKind,
    public val scope: MemoryScopeClass,
    public val scopeOwnerId: String,
    public val lifecycle: HypothesisState,
    public val sensitivity: MemorySensitivity,
    public val eraseRequested: Boolean,
    public val content: String,
) {
    /** Lowercase SHA-256 over normalized content bytes. */
    public val contentDigest: String get() = sha256(content.encodeToByteArray())

    /** Lowercase SHA-256 over canonical Markdown bytes. */
    public val documentDigest: String get() = sha256(render().encodeToByteArray())

    /** Renders the unique canonical M2 Markdown representation. */
    public fun render(): String = buildString {
        append("---\n")
        append("schema_version: 1\n")
        append("record_ref: ${recordRef.render()}\n")
        append("authority: ${authority.wireName}\n")
        append("memory_type: ${memoryType.wireName}\n")
        append("memory_role: ${memoryRole.wireName}\n")
        append("scope: ${scope.wireName}\n")
        append("scope_owner_b64: ${encodeIdentity(scopeOwnerId)}\n")
        append("lifecycle: ${lifecycle.wireName}\n")
        append("sensitivity: ${sensitivity.wireName}\n")
        if (eraseRequested) append("erase: true\n")
        append("---\n")
        append(content)
    }

    public companion object {
        /** Builds one canonical current document from admitted repository fields. */
        @Suppress("LongParameterList")
        public fun fromRepositoryRecord(
            recordId: String,
            revisionId: String,
            authority: MemoryAuthority,
            memoryType: MemoryType,
            memoryRole: MemoryKind,
            scope: MemoryScopeClass,
            scopeOwnerId: String,
            lifecycle: HypothesisState,
            sensitivity: MemorySensitivity,
            content: String,
            limits: MemoryDocumentLimits,
        ): MemoryControlResult<MemoryControlDocument> {
            if (!validDecodedIdentity(recordId, limits.maxIdBytes) ||
                !validDecodedIdentity(revisionId, limits.maxIdBytes) ||
                !validDecodedIdentity(scopeOwnerId, limits.maxIdBytes) ||
                ('\r' in content && '\r' in content.replace("\r\n", "\n"))
            ) return failureControl(MemoryControlError.INVALID_SNAPSHOT)
            val normalized = content.replace("\r\n", "\n").trimEnd('\n') + "\n"
            if (normalized == "\n") return failureControl(MemoryControlError.INVALID_SNAPSHOT)
            if (normalized.encodeToByteArray().size > limits.maxContentBytes) {
                return failureControl(MemoryControlError.BOUND_EXCEEDED)
            }
            val document = MemoryControlDocument(
                MemoryRecordRef.Existing(recordId, revisionId), authority, memoryType, memoryRole,
                scope, scopeOwnerId, lifecycle, sensitivity, false, normalized,
            )
            return if (document.render().encodeToByteArray().size > limits.maxDocumentBytes) {
                failureControl(MemoryControlError.BOUND_EXCEEDED)
            } else MemoryControlResult.Success(document)
        }
    }
}

/** Parses strict M2 front matter and normalizes CRLF/content termination. */
public fun parseMemoryDocument(
    bytes: ByteArray,
    limits: MemoryDocumentLimits,
): MemoryControlResult<MemoryControlDocument> {
    if (bytes.size > limits.maxDocumentBytes) return failureControl(MemoryControlError.BOUND_EXCEEDED)
    val raw = bytes.decodeToString(throwOnInvalidSequence = true)
    if ('\r' in raw && '\r' in raw.replace("\r\n", "\n")) {
        return failureControl(MemoryControlError.INVALID_SNAPSHOT)
    }
    val normalized = raw.replace("\r\n", "\n")
    if (!normalized.startsWith("---\n")) return failureControl(MemoryControlError.INVALID_SNAPSHOT)
    val separator = normalized.indexOf("\n---\n", startIndex = 4)
    if (separator < 0) return failureControl(MemoryControlError.INVALID_SNAPSHOT)
    val front = normalized.substring(4, separator)
    val rawContent = normalized.substring(separator + 5)
    if ("\n---" in rawContent) return failureControl(MemoryControlError.INVALID_SNAPSHOT)
    val lines = front.lines()
    if (lines.size !in setOf(BASE_KEYS.size, BASE_KEYS.size + 1)) {
        return failureControl(MemoryControlError.INVALID_SNAPSHOT)
    }
    val values = mutableListOf<String>()
    lines.forEachIndexed { index, line ->
        val split = line.indexOf(": ")
        if (split <= 0) return failureControl(MemoryControlError.INVALID_SNAPSHOT)
        val key = line.substring(0, split)
        val value = line.substring(split + 2)
        val expected = BASE_KEYS.getOrElse(index) { "erase" }
        if (key != expected || !validControlToken(value, 512)) {
            return failureControl(MemoryControlError.INVALID_SNAPSHOT)
        }
        values += value
    }
    if (values[0] != "1") return failureControl(MemoryControlError.INVALID_SNAPSHOT)
    val recordRef = parseRecordRef(values[1], limits.maxIdBytes)
        ?: return failureControl(MemoryControlError.INVALID_SNAPSHOT)
    val scopeOwner = decodeIdentity(values[6], limits.maxIdBytes)
        ?: return failureControl(MemoryControlError.INVALID_SNAPSHOT)
    val erase = values.getOrNull(9)?.let { if (it == "true") true else null }
    if (values.size == 10 && erase == null) return failureControl(MemoryControlError.INVALID_SNAPSHOT)
    val content = rawContent.trimEnd('\n') + "\n"
    if (content == "\n") return failureControl(MemoryControlError.INVALID_SNAPSHOT)
    if (content.encodeToByteArray().size > limits.maxContentBytes) {
        return failureControl(MemoryControlError.BOUND_EXCEEDED)
    }
    val document = MemoryControlDocument(
        recordRef = recordRef,
        authority = enumByWire<MemoryAuthority>(values[2])
            ?: return failureControl(MemoryControlError.INVALID_SNAPSHOT),
        memoryType = enumByWire<MemoryType>(values[3])
            ?: return failureControl(MemoryControlError.INVALID_SNAPSHOT),
        memoryRole = enumByWire<MemoryKind>(values[4])
            ?: return failureControl(MemoryControlError.INVALID_SNAPSHOT),
        scope = enumByWire<MemoryScopeClass>(values[5])
            ?: return failureControl(MemoryControlError.INVALID_SNAPSHOT),
        scopeOwnerId = scopeOwner,
        lifecycle = enumByWire<HypothesisState>(values[7])
            ?: return failureControl(MemoryControlError.INVALID_SNAPSHOT),
        sensitivity = enumByWire<MemorySensitivity>(values[8])
            ?: return failureControl(MemoryControlError.INVALID_SNAPSHOT),
        eraseRequested = erase == true,
        content = content,
    )
    return if (document.render().encodeToByteArray().size > limits.maxDocumentBytes) {
        failureControl(MemoryControlError.BOUND_EXCEEDED)
    } else {
        MemoryControlResult.Success(document)
    }
}

private val BASE_KEYS: List<String> = listOf(
    "schema_version", "record_ref", "authority", "memory_type", "memory_role",
    "scope", "scope_owner_b64", "lifecycle", "sensitivity",
)

private fun parseRecordRef(value: String, maxIdBytes: Int): MemoryRecordRef? = when {
    value.startsWith("existing.") -> {
        val parts = value.removePrefix("existing.").split('.')
        if (parts.size != 2) null else {
            val record = decodeIdentity(parts[0], maxIdBytes)
            val revision = decodeIdentity(parts[1], maxIdBytes)
            if (record == null || revision == null) null else MemoryRecordRef.Existing(record, revision)
        }
    }
    value.startsWith("new.") -> value.removePrefix("new.").takeIf {
        validControlToken(it, 64)
    }?.let(MemoryRecordRef::New)
    else -> null
}

private fun MemoryRecordRef.render(): String = when (this) {
    is MemoryRecordRef.Existing -> "existing.${encodeIdentity(recordId)}.${encodeIdentity(revisionId)}"
    is MemoryRecordRef.New -> "new.$draftToken"
}

private fun encodeIdentity(value: String): String =
    Base64.getUrlEncoder().withoutPadding().encodeToString(value.encodeToByteArray())

private fun decodeIdentity(value: String, maxBytes: Int): String? = try {
    val bytes = Base64.getUrlDecoder().decode(value)
    val decoded = bytes.decodeToString(throwOnInvalidSequence = true)
    decoded.takeIf {
        it.isNotEmpty() && bytes.size <= maxBytes && it.trim() == it && encodeIdentity(it) == value
    }
} catch (_: IllegalArgumentException) {
    null
}

private fun validDecodedIdentity(value: String, maxBytes: Int): Boolean =
    value.isNotEmpty() && value.encodeToByteArray().size <= maxBytes && value.trim() == value

private fun validControlToken(value: String, maxBytes: Int): Boolean =
    value.isNotEmpty() && value.length <= maxBytes && value.all {
        it.isLetterOrDigit() && it.code < 128 || it == '_' || it == '-' || it == '.'
    }

private inline fun <reified T : Enum<T>> enumByWire(value: String): T? = enumValues<T>().find {
    when (it) {
        is MemoryAuthority -> it.wireName
        is MemoryType -> it.wireName
        is MemoryKind -> it.wireName
        is MemoryScopeClass -> it.wireName
        is HypothesisState -> it.wireName
        is MemorySensitivity -> it.wireName
        else -> null
    } == value
}

private fun failureControl(error: MemoryControlError): MemoryControlResult.Failure =
    MemoryControlResult.Failure(error)
