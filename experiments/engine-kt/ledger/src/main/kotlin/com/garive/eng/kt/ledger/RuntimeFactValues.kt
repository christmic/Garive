package com.garive.eng.kt.ledger

import java.security.MessageDigest
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

internal fun JsonObject.exact(required: Set<String>, optional: Set<String> = emptySet()) {
    require(keys.containsAll(required) && keys.all { it in required || it in optional })
}

internal fun JsonObject.text(key: String): String =
    getValue(key).jsonPrimitive.takeIf(JsonPrimitive::isString)?.contentOrNull
        ?: throw IllegalArgumentException()

internal fun JsonObject.nonEmpty(key: String) {
    require(text(key).isNotEmpty())
}

internal fun JsonObject.optionalNonEmpty(key: String) {
    if (key in this) nonEmpty(key)
}

internal fun JsonObject.enum(key: String, allowed: Set<String>): String =
    text(key).also { require(it in allowed) }

internal fun JsonObject.ulong(key: String, nonzero: Boolean = false) {
    val value = getValue(key).jsonPrimitive.content.toULongOrNull() ?: throw IllegalArgumentException()
    require(!nonzero || value != 0uL)
}

internal fun JsonObject.optionalUlong(key: String) {
    if (key in this) ulong(key)
}

internal fun JsonObject.digest(key: String) {
    require(text(key).matches(Regex("[0-9a-f]{64}")))
}

internal fun JsonObject.content(key: String) {
    val binding = getValue(key).jsonObject
    binding.exact(setOf("digest"), setOf("inline_utf8", "reference"))
    binding.digest("digest")
    val inline = binding["inline_utf8"]
    val reference = binding["reference"]
    when {
        inline != null && reference == null -> {
            val text = inline.jsonPrimitive.takeIf(JsonPrimitive::isString)?.contentOrNull
                ?: throw IllegalArgumentException()
            require(binding.text("digest") == sha256(text.encodeToByteArray()))
        }
        inline == null && reference != null -> binding.nonEmpty("reference")
        else -> throw IllegalArgumentException()
    }
}

internal fun JsonObject.optionalContent(key: String) {
    if (key in this) content(key)
}

internal fun JsonObject.usage(key: String) {
    val value = getValue(key).jsonObject
    value.exact(
        setOf("input_tokens", "output_tokens", "source"),
        setOf("cache_read_tokens", "cache_write_tokens"),
    )
    listOf("input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens")
        .filter(value::containsKey)
        .forEach { value.tokenCount(it) }
    value.enum("source", setOf("provider_reported", "estimated"))
}

private fun JsonObject.tokenCount(key: String) {
    val value = getValue(key).jsonObject
    when (value.enum("kind", setOf("known", "unknown"))) {
        "known" -> {
            value.exact(setOf("kind", "value"))
            value.ulong("value")
        }
        else -> value.exact(setOf("kind"))
    }
}

internal fun JsonObject.limits(key: String) {
    val value = getValue(key).jsonObject
    value.exact(
        setOf("max_iterations"),
        setOf("max_input_tokens", "max_output_tokens", "deadline_budget_ms"),
    )
    listOf("max_iterations", "max_input_tokens", "max_output_tokens", "deadline_budget_ms")
        .filter(value::containsKey)
        .forEach { value.ulong(it, true) }
}

internal fun JsonElement.asObject(): JsonObject = this as? JsonObject ?: throw IllegalArgumentException()

private fun sha256(value: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(value).joinToString("") { "%02x".format(it) }
