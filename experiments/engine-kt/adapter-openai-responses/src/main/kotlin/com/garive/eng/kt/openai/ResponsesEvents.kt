package com.garive.eng.kt.openai

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Core Responses event discriminators from the pinned official SDK. */
public enum class PortableEventKind(public val wireName: String) {
    CREATED("response.created"),
    QUEUED("response.queued"),
    IN_PROGRESS("response.in_progress"),
    COMPLETED("response.completed"),
    FAILED("response.failed"),
    INCOMPLETE("response.incomplete"),
    ERROR("error"),
    OUTPUT_ITEM_ADDED("response.output_item.added"),
    OUTPUT_ITEM_DONE("response.output_item.done"),
    CONTENT_PART_ADDED("response.content_part.added"),
    CONTENT_PART_DONE("response.content_part.done"),
    OUTPUT_TEXT_DELTA("response.output_text.delta"),
    OUTPUT_TEXT_DONE("response.output_text.done"),
    REFUSAL_DELTA("response.refusal.delta"),
    REFUSAL_DONE("response.refusal.done"),
    FUNCTION_ARGUMENTS_DELTA("response.function_call_arguments.delta"),
    FUNCTION_ARGUMENTS_DONE("response.function_call_arguments.done"),
    REASONING_SUMMARY_PART_ADDED("response.reasoning_summary_part.added"),
    REASONING_SUMMARY_PART_DONE("response.reasoning_summary_part.done"),
    REASONING_SUMMARY_TEXT_DELTA("response.reasoning_summary_text.delta"),
    REASONING_SUMMARY_TEXT_DONE("response.reasoning_summary_text.done"),
    REASONING_TEXT_DELTA("response.reasoning_text.delta"),
    REASONING_TEXT_DONE("response.reasoning_text.done"),
    OUTPUT_TEXT_ANNOTATION_ADDED("response.output_text.annotation.added"),
    ;

    public companion object {
        /** Finds a portable kind by exact wire discriminator. */
        public fun fromWireName(value: String): PortableEventKind? = entries.firstOrNull { it.wireName == value }
    }
}

/** Typed portable event or lossless hosted/future extension event. */
public sealed interface ResponseStreamEvent {
    /** Returns the exact wire discriminator. */
    public val discriminator: String
    /** Returns a sequence number when the event supplies one. */
    public val sequenceNumber: ULong?
    /** Returns the complete original JSON object. */
    public val raw: JsonObject

    /** Validated portable event. */
    public data class Portable(
        public val kind: PortableEventKind,
        override val sequenceNumber: ULong,
        override val raw: JsonObject,
    ) : ResponseStreamEvent {
        override val discriminator: String = kind.wireName
    }

    /** Hosted or future event retained without semantic promotion. */
    public data class Extension(
        override val discriminator: String,
        override val sequenceNumber: ULong?,
        override val raw: JsonObject,
    ) : ResponseStreamEvent

    public companion object {
        /** Parses and validates one complete Responses event object. */
        public fun parse(value: JsonObject): ResponseStreamEvent {
            val discriminator = value.requiredText("type")
            val kind = PortableEventKind.fromWireName(discriminator)
                ?: return Extension(discriminator, value.optionalULong("sequence_number"), value)
            val sequence = value.requiredULong("sequence_number")
            validatePayload(kind, value)
            return Portable(kind, sequence, value)
        }
    }
}

private fun validatePayload(kind: PortableEventKind, value: JsonObject): Unit {
    when (kind) {
        PortableEventKind.CREATED,
        PortableEventKind.QUEUED,
        PortableEventKind.IN_PROGRESS,
        PortableEventKind.COMPLETED,
        PortableEventKind.FAILED,
        PortableEventKind.INCOMPLETE,
        -> ResponseEnvelope.parse(value.getValue("response").jsonObject)

        PortableEventKind.OUTPUT_ITEM_ADDED,
        PortableEventKind.OUTPUT_ITEM_DONE,
        -> {
            value.requiredULong("output_index")
            validateItem(value.getValue("item").jsonObject)
        }

        PortableEventKind.CONTENT_PART_ADDED,
        PortableEventKind.CONTENT_PART_DONE,
        -> {
            value.requiredULong("output_index"); value.requiredULong("content_index")
            value.requiredText("item_id"); value.getValue("part").jsonObject.requiredText("type")
        }

        PortableEventKind.OUTPUT_TEXT_DELTA,
        PortableEventKind.REFUSAL_DELTA,
        -> validateContentText(value, "delta")

        PortableEventKind.OUTPUT_TEXT_DONE -> validateContentText(value, "text")
        PortableEventKind.REFUSAL_DONE -> validateContentText(value, "refusal")

        PortableEventKind.FUNCTION_ARGUMENTS_DELTA,
        PortableEventKind.FUNCTION_ARGUMENTS_DONE,
        -> {
            value.requiredULong("output_index"); value.requiredText("item_id")
            value.requiredText(if (kind == PortableEventKind.FUNCTION_ARGUMENTS_DELTA) "delta" else "arguments")
        }

        PortableEventKind.REASONING_SUMMARY_PART_ADDED,
        PortableEventKind.REASONING_SUMMARY_PART_DONE,
        -> {
            value.requiredULong("output_index"); value.requiredULong("summary_index")
            value.requiredText("item_id"); value.getValue("part").jsonObject.requiredText("type")
        }

        PortableEventKind.REASONING_SUMMARY_TEXT_DELTA,
        PortableEventKind.REASONING_SUMMARY_TEXT_DONE,
        -> {
            value.requiredULong("output_index"); value.requiredULong("summary_index")
            value.requiredText("item_id")
            value.requiredText(if (kind == PortableEventKind.REASONING_SUMMARY_TEXT_DELTA) "delta" else "text")
        }

        PortableEventKind.REASONING_TEXT_DELTA,
        PortableEventKind.REASONING_TEXT_DONE,
        -> {
            value.requiredULong("output_index"); value.requiredULong("content_index")
            value.requiredText("item_id")
            value.requiredText(if (kind == PortableEventKind.REASONING_TEXT_DELTA) "delta" else "text")
        }

        PortableEventKind.OUTPUT_TEXT_ANNOTATION_ADDED -> {
            value.requiredULong("output_index"); value.requiredULong("content_index")
            value.requiredULong("annotation_index"); value.requiredText("item_id")
            require(value.containsKey("annotation"))
        }

        PortableEventKind.ERROR -> value.requiredText("message")
    }
}

private fun validateContentText(value: JsonObject, field: String): Unit {
    value.requiredULong("output_index"); value.requiredULong("content_index")
    value.requiredText("item_id"); value.requiredText(field)
}

private fun validateItem(value: JsonObject): Unit {
    val type = value.requiredText("type")
    value.requiredText("id")
    when (type) {
        "message" -> {
            require(value.requiredText("role") == "assistant")
            require(value["content"] is kotlinx.serialization.json.JsonArray)
        }
        "function_call" -> {
            value.requiredText("call_id"); value.requiredText("name")
            require(value.containsKey("arguments"))
        }
        "reasoning" -> require(value["summary"] is kotlinx.serialization.json.JsonArray)
    }
}

internal fun JsonObject.requiredText(name: String): String =
    getValue(name).jsonPrimitive.contentOrNull?.also { require(it.isNotEmpty()) }
        ?: throw IllegalArgumentException("$name must be non-empty text")

internal fun JsonObject.requiredULong(name: String): ULong =
    getValue(name).jsonPrimitive.content.toULong()

internal fun JsonObject.optionalULong(name: String): ULong? =
    (get(name) as? JsonPrimitive)?.contentOrNull?.toULongOrNull()

internal val RESPONSES_JSON: Json = Json { ignoreUnknownKeys = false }
