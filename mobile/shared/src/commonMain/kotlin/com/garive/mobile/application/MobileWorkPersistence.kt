package com.garive.mobile.application

import com.garive.mobile.model.MobilePendingCommand
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import okio.ByteString.Companion.encodeUtf8
import kotlin.time.Clock
import kotlin.time.ExperimentalTime

/** Platform-owned storage for one bounded ambiguous mutation and its exact input. */
public interface MobileWorkPersistence {
    public fun readPendingRecord(): String?
    public fun writePendingRecord(value: String?)
    public fun readPendingPayload(): String?
    public fun writePendingPayload(value: String?)
}

/** Default used by tests or embedders that intentionally do not retain work across restart. */
public object EphemeralMobileWorkPersistence : MobileWorkPersistence {
    override fun readPendingRecord(): String? = null
    override fun writePendingRecord(value: String?): Unit = Unit
    override fun readPendingPayload(): String? = null
    override fun writePendingPayload(value: String?): Unit = Unit
}

@OptIn(ExperimentalTime::class)
internal fun pendingTimestamp(): Long = Clock.System.now().toEpochMilliseconds()

internal fun savePending(persistence: MobileWorkPersistence, operation: PendingOperation): Unit {
    val payload = operation.payload()
    persistence.writePendingPayload(payload)
    persistence.writePendingRecord(encodePending(operation, payload))
}

internal fun clearPending(persistence: MobileWorkPersistence): Unit {
    persistence.writePendingRecord(null)
    persistence.writePendingPayload(null)
}

internal fun restorePending(
    persistence: MobileWorkPersistence,
    maxInputBytes: Int,
): PendingOperation? {
    val record = persistence.readPendingRecord()
    val payload = persistence.readPendingPayload()
    if (record == null) {
        if (payload != null) clearPending(persistence)
        return null
    }
    return try {
        decodePending(record, payload, maxInputBytes)
    } catch (_: IllegalArgumentException) {
        clearPending(persistence)
        null
    }
}

internal sealed class PendingOperation {
    abstract val sessionId: String?
    abstract val createdAtEpochMs: Long
    abstract fun publicValue(): MobilePendingCommand
    abstract fun payload(): String?

    class CreateAndStart(
        val definitionId: String,
        val text: String,
        val createCommandId: String,
        val startCommandId: String,
        override var sessionId: String? = null,
        override val createdAtEpochMs: Long = pendingTimestamp(),
    ) : PendingOperation() {
        override fun publicValue(): MobilePendingCommand = MobilePendingCommand(
            if (sessionId == null) "create" else "start",
            if (sessionId == null) createCommandId else startCommandId,
            sessionId,
            null,
        )
        override fun payload(): String = text
    }

    class Start(
        override val sessionId: String,
        val text: String,
        val commandId: String,
        override val createdAtEpochMs: Long = pendingTimestamp(),
    ) : PendingOperation() {
        override fun publicValue(): MobilePendingCommand =
            MobilePendingCommand("start", commandId, sessionId, null)
        override fun payload(): String = text
    }

    class Cancel(
        override val sessionId: String,
        val turnId: String,
        val position: Long,
        val commandId: String,
        override val createdAtEpochMs: Long = pendingTimestamp(),
    ) : PendingOperation() {
        override fun publicValue(): MobilePendingCommand =
            MobilePendingCommand("cancel", commandId, sessionId, turnId)
        override fun payload(): String? = null
    }

    class Continue(
        override val sessionId: String,
        val turnId: String,
        val suspensionId: String,
        val sessionVersion: Long,
        val input: String,
        val inputJson: Boolean,
        val commandId: String,
        override val createdAtEpochMs: Long = pendingTimestamp(),
    ) : PendingOperation() {
        override fun publicValue(): MobilePendingCommand =
            MobilePendingCommand("continue", commandId, sessionId, turnId)
        override fun payload(): String = input
    }
}

private fun encodePending(operation: PendingOperation, payload: String?): String {
    val semantic = semanticDocument(operation, payload)
    return buildJsonObject {
        put("schema_version", 1)
        put("kind", operation.publicValue().kind)
        put("created_at_epoch_ms", operation.createdAtEpochMs)
        put("semantic_digest", semantic.encodeUtf8().sha256().hex())
        when (operation) {
            is PendingOperation.CreateAndStart -> {
                put("definition_id", operation.definitionId)
                put("create_command_id", operation.createCommandId)
                put("start_command_id", operation.startCommandId)
                operation.sessionId?.let { put("session_id", it) }
            }
            is PendingOperation.Start -> {
                put("command_id", operation.commandId); put("session_id", operation.sessionId)
            }
            is PendingOperation.Cancel -> {
                put("command_id", operation.commandId); put("session_id", operation.sessionId)
                put("turn_id", operation.turnId); put("position", operation.position)
            }
            is PendingOperation.Continue -> {
                put("command_id", operation.commandId); put("session_id", operation.sessionId)
                put("turn_id", operation.turnId); put("suspension_id", operation.suspensionId)
                put("session_version", operation.sessionVersion); put("input_json", operation.inputJson)
            }
        }
    }.toString()
}

private fun semanticDocument(operation: PendingOperation, payload: String?): String = buildJsonObject {
    put("kind", operation.publicValue().kind)
    put("command_id", operation.publicValue().commandId)
    put("created_at_epoch_ms", operation.createdAtEpochMs)
    operation.sessionId?.let { put("session_id", it) }
    when (operation) {
        is PendingOperation.CreateAndStart -> {
            put("definition_id", operation.definitionId)
            put("create_command_id", operation.createCommandId)
            put("start_command_id", operation.startCommandId)
        }
        is PendingOperation.Start -> Unit
        is PendingOperation.Cancel -> {
            put("turn_id", operation.turnId); put("position", operation.position)
        }
        is PendingOperation.Continue -> {
            put("turn_id", operation.turnId); put("suspension_id", operation.suspensionId)
            put("session_version", operation.sessionVersion); put("input_json", operation.inputJson)
        }
    }
    payload?.let { put("input_sha256", it.encodeUtf8().sha256().hex()) }
}.toString()

private fun decodePending(record: String, payload: String?, maxInputBytes: Int): PendingOperation {
    if (record.encodeToByteArray().size > MAX_RECORD_BYTES || maxInputBytes <= 0) invalid()
    val value = try { Json.parseToJsonElement(record).jsonObject } catch (_: Throwable) { invalid() }
    if (value.keys.any { it !in RECORD_KEYS } || value.long("schema_version") != 1L) invalid()
    val createdAt = value.long("created_at_epoch_ms").also { if (it <= 0) invalid() }
    val kind = value.text("kind")
    val operation = when (kind) {
        "create", "start" -> if (value["create_command_id"] != null) {
            requirePayload(payload, maxInputBytes)
            PendingOperation.CreateAndStart(
                value.id("definition_id"), payload!!, value.id("create_command_id"),
                value.id("start_command_id"), value.optionalId("session_id"), createdAt,
            ).also { if ((kind == "create") != (it.sessionId == null)) invalid() }
        } else {
            if (kind != "start") invalid()
            requirePayload(payload, maxInputBytes)
            PendingOperation.Start(value.id("session_id"), payload!!, value.id("command_id"), createdAt)
        }
        "cancel" -> {
            if (payload != null) invalid()
            PendingOperation.Cancel(
                value.id("session_id"), value.id("turn_id"),
                value.long("position").also { if (it < 0) invalid() }, value.id("command_id"), createdAt,
            )
        }
        "continue" -> {
            requirePayload(payload, maxInputBytes)
            PendingOperation.Continue(
                value.id("session_id"), value.id("turn_id"), value.id("suspension_id"),
                value.long("session_version").also { if (it <= 0) invalid() }, payload!!,
                value.boolean("input_json"), value.id("command_id"), createdAt,
            )
        }
        else -> invalid()
    }
    val digest = value.text("semantic_digest")
    if (!digest.matches(HEX_DIGEST) || semanticDocument(operation, payload).encodeUtf8().sha256().hex() != digest) invalid()
    validateShape(value, operation)
    return operation
}

private fun validateShape(value: JsonObject, operation: PendingOperation): Unit {
    val required = BASE_KEYS + when (operation) {
        is PendingOperation.CreateAndStart -> setOf("definition_id", "create_command_id", "start_command_id") +
            if (operation.sessionId == null) emptySet() else setOf("session_id")
        is PendingOperation.Start -> setOf("command_id", "session_id")
        is PendingOperation.Cancel -> setOf("command_id", "session_id", "turn_id", "position")
        is PendingOperation.Continue -> setOf(
            "command_id", "session_id", "turn_id", "suspension_id", "session_version", "input_json",
        )
    }
    if (value.keys != required) invalid()
}

private fun requirePayload(value: String?, maxBytes: Int): Unit {
    if (value.isNullOrBlank() || value.encodeToByteArray().size > maxBytes) invalid()
}
private fun JsonObject.text(key: String): String = this[key]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotEmpty() } ?: invalid()
private fun JsonObject.id(key: String): String = text(key).also(::validateId)
private fun JsonObject.optionalId(key: String): String? = this[key]?.jsonPrimitive?.contentOrNull?.also(::validateId)
private fun JsonObject.long(key: String): Long = this[key]?.jsonPrimitive?.longOrNull ?: invalid()
private fun JsonObject.boolean(key: String): Boolean = when (text(key)) { "true" -> true; "false" -> false; else -> invalid() }
private fun validateId(value: String): Unit {
    if (value.encodeToByteArray().size !in 1..128 || value.any { it.code !in 0x21..0x7e }) invalid()
}
private fun invalid(): Nothing = throw IllegalArgumentException("invalid_mobile_pending_record")

private const val MAX_RECORD_BYTES: Int = 16_384
private val HEX_DIGEST: Regex = Regex("[0-9a-f]{64}")
private val BASE_KEYS: Set<String> = setOf("schema_version", "kind", "created_at_epoch_ms", "semantic_digest")
private val RECORD_KEYS: Set<String> = BASE_KEYS + setOf(
    "definition_id", "create_command_id", "start_command_id", "command_id", "session_id", "turn_id",
    "position", "suspension_id", "session_version", "input_json",
)
