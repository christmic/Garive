package com.garive.eng.kt.anthropic

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Portable Messages stream event discriminator. */
public enum class PortableEventKind(public val wireName: String) {
    MESSAGE_START("message_start"),
    CONTENT_BLOCK_START("content_block_start"),
    CONTENT_BLOCK_DELTA("content_block_delta"),
    CONTENT_BLOCK_STOP("content_block_stop"),
    MESSAGE_DELTA("message_delta"),
    MESSAGE_STOP("message_stop"),
    PING("ping"),
    ERROR("error"),
    ;

    public companion object {
        /** Finds a portable event by exact wire discriminator. */
        public fun fromWireName(value: String): PortableEventKind? = entries.firstOrNull { it.wireName == value }
    }
}

/** Portable content-block delta discriminator. */
public enum class DeltaKind(public val wireName: String) {
    TEXT("text_delta"),
    INPUT_JSON("input_json_delta"),
    THINKING("thinking_delta"),
    SIGNATURE("signature_delta"),
    CITATION("citations_delta"),
    ;

    public companion object {
        /** Finds a portable delta by exact wire discriminator. */
        public fun fromWireName(value: String): DeltaKind? = entries.firstOrNull { it.wireName == value }
    }
}

/** Typed Messages event with a lossless original object. */
public sealed interface StreamEvent {
    /** Returns the exact event discriminator. */
    public val discriminator: String
    /** Returns the complete original event object. */
    public val raw: JsonObject

    /** Validated portable event. */
    public data class Portable(
        public val kind: PortableEventKind,
        public val deltaKind: DeltaKind?,
        override val raw: JsonObject,
    ) : StreamEvent {
        override val discriminator: String = kind.wireName
    }

    /** Future event retained without semantic promotion. */
    public data class Extension(
        override val discriminator: String,
        override val raw: JsonObject,
    ) : StreamEvent

    public companion object {
        /** Parses and validates one complete event object. */
        public fun parse(value: JsonObject): StreamEvent {
            val discriminator = value.requiredText(MessageFields.TYPE)
            val kind = PortableEventKind.fromWireName(discriminator)
                ?: return Extension(discriminator, value)
            val deltaKind = validatePayload(kind, value)
            return Portable(kind, deltaKind, value)
        }
    }
}

private fun validatePayload(kind: PortableEventKind, value: JsonObject): DeltaKind? = when (kind) {
    PortableEventKind.MESSAGE_START -> {
        MessageResponse.parse(value.getValue("message").jsonObject)
        null
    }
    PortableEventKind.CONTENT_BLOCK_START -> {
        value.requiredUInt("index")
        validateStartBlock(value.getValue(MessageFields.CONTENT_BLOCK).jsonObject)
        null
    }
    PortableEventKind.CONTENT_BLOCK_DELTA -> {
        value.requiredUInt("index")
        validateDelta(value.getValue(MessageFields.DELTA).jsonObject)
    }
    PortableEventKind.CONTENT_BLOCK_STOP -> {
        value.requiredUInt("index")
        null
    }
    PortableEventKind.MESSAGE_DELTA -> {
        require(value["delta"] is JsonObject)
        value.getValue("usage").jsonObject.requiredULong("output_tokens")
        null
    }
    PortableEventKind.ERROR -> {
        ErrorEnvelope.parse(value)
        null
    }
    PortableEventKind.MESSAGE_STOP,
    PortableEventKind.PING,
    -> null
}

private fun validateStartBlock(block: JsonObject): Unit {
    when (block.requiredText(MessageFields.TYPE)) {
        MessageKinds.TEXT -> require(block["text"] is JsonPrimitive)
        MessageKinds.THINKING -> {
            require(block["thinking"] is JsonPrimitive)
            require(block["signature"] is JsonPrimitive)
        }
        MessageKinds.REDACTED_THINKING -> block.requiredText("data")
        MessageKinds.TOOL_USE -> {
            block.requiredText("id"); block.requiredText("name")
            require(block["input"] is JsonObject)
        }
    }
}

private fun validateDelta(delta: JsonObject): DeltaKind? {
    val discriminator = delta.requiredText(MessageFields.TYPE)
    val kind = DeltaKind.fromWireName(discriminator) ?: return null
    when (kind) {
        DeltaKind.TEXT -> require(delta["text"] is JsonPrimitive)
        DeltaKind.INPUT_JSON -> require(delta[MessageFields.PARTIAL_JSON] is JsonPrimitive)
        DeltaKind.THINKING -> require(delta["thinking"] is JsonPrimitive)
        DeltaKind.SIGNATURE -> require(delta["signature"] is JsonPrimitive)
        DeltaKind.CITATION -> require(delta.containsKey("citation"))
    }
    return kind
}

internal fun JsonObject.requiredText(name: String): String =
    getValue(name).jsonPrimitive.contentOrNull?.also { require(it.isNotEmpty()) }
        ?: throw IllegalArgumentException("$name must be non-empty text")

internal fun JsonObject.requiredUInt(name: String): UInt = getValue(name).jsonPrimitive.content.toUInt()
internal fun JsonObject.requiredULong(name: String): ULong = getValue(name).jsonPrimitive.content.toULong()
internal val MESSAGES_JSON: Json = Json { ignoreUnknownKeys = false }
