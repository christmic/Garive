package com.garive.eng.kt.ledger

import java.math.BigInteger
import java.security.MessageDigest
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull

/** Canonical UTF-8 JSON bytes and the lowercase SHA-256 digest binding them. */
public class CanonicalPayload private constructor(
    public val json: String,
    public val sha256: String,
) {
    /** Recomputes the digest and reports persisted/in-memory corruption. */
    public fun verify(): CanonicalPayloadError? =
        if (digest(json.encodeToByteArray()) == sha256) null else CanonicalPayloadError.DIGEST_MISMATCH

    internal fun withDigestForCorruptionTest(value: String): CanonicalPayload = CanonicalPayload(json, value)

    public override fun equals(other: Any?): Boolean =
        other is CanonicalPayload && json == other.json && sha256 == other.sha256

    public override fun hashCode(): Int = 31 * json.hashCode() + sha256.hashCode()

    public companion object {
        /** Canonicalizes an admitted JSON value and computes its digest. */
        public fun fromValue(value: JsonElement): CanonicalPayloadResult {
            val output = StringBuilder()
            val error = encode(value, output)
            if (error != null) return CanonicalPayloadResult.Failure(error)
            val json = output.toString()
            return CanonicalPayloadResult.Success(CanonicalPayload(json, digest(json.encodeToByteArray())))
        }

        /** Validates stored JSON canonical form and its separately stored digest. */
        public fun fromStoredJson(json: String, sha256: String): CanonicalPayloadResult {
            val value = try {
                Json.parseToJsonElement(json)
            } catch (_: SerializationException) {
                return CanonicalPayloadResult.Failure(CanonicalPayloadError.INVALID_JSON)
            } catch (_: IllegalArgumentException) {
                return CanonicalPayloadResult.Failure(CanonicalPayloadError.INVALID_JSON)
            }
            return when (val canonical = fromValue(value)) {
                is CanonicalPayloadResult.Failure -> canonical
                is CanonicalPayloadResult.Success -> when {
                    canonical.payload.json != json ->
                        CanonicalPayloadResult.Failure(CanonicalPayloadError.NON_CANONICAL)
                    canonical.payload.sha256 != sha256 ->
                        CanonicalPayloadResult.Failure(CanonicalPayloadError.DIGEST_MISMATCH)
                    else -> canonical
                }
            }
        }
    }
}

/** Success/failure envelope for canonical payload construction. */
public sealed interface CanonicalPayloadResult {
    public data class Success(public val payload: CanonicalPayload) : CanonicalPayloadResult
    public data class Failure(public val error: CanonicalPayloadError) : CanonicalPayloadResult
}

/** Stable validation or integrity failure for canonical JSON. */
public enum class CanonicalPayloadError { INVALID_JSON, UNSUPPORTED_NUMBER, NON_CANONICAL, DIGEST_MISMATCH }

private val minimumInteger = BigInteger.valueOf(Long.MIN_VALUE)
private val maximumInteger = BigInteger("18446744073709551615")
private val integerPattern = Regex("-?(0|[1-9][0-9]*)")

private fun encode(value: JsonElement, output: StringBuilder): CanonicalPayloadError? {
    when (value) {
        JsonNull -> output.append("null")
        is JsonPrimitive -> {
            if (value.isString) {
                encodeString(value.content, output)
            } else if (value.booleanOrNull != null) {
                output.append(value.content)
            } else {
                if (!integerPattern.matches(value.content)) return CanonicalPayloadError.UNSUPPORTED_NUMBER
                val integer = value.content.toBigInteger()
                if (integer < minimumInteger || integer > maximumInteger) {
                    return CanonicalPayloadError.UNSUPPORTED_NUMBER
                }
                output.append(integer)
            }
        }
        is JsonArray -> {
            output.append('[')
            value.forEachIndexed { index, item ->
                if (index != 0) output.append(',')
                encode(item, output)?.let { return it }
            }
            output.append(']')
        }
        is JsonObject -> {
            output.append('{')
            value.keys.sortedWith(::compareScalarValues).forEachIndexed { index, key ->
                if (index != 0) output.append(',')
                encodeString(key, output)
                output.append(':')
                encode(value.getValue(key), output)?.let { return it }
            }
            output.append('}')
        }
    }
    return null
}

private fun compareScalarValues(left: String, right: String): Int {
    val leftValues = left.codePoints().iterator()
    val rightValues = right.codePoints().iterator()
    while (leftValues.hasNext() && rightValues.hasNext()) {
        val comparison = leftValues.nextInt().compareTo(rightValues.nextInt())
        if (comparison != 0) return comparison
    }
    return leftValues.hasNext().compareTo(rightValues.hasNext())
}

private fun encodeString(value: String, output: StringBuilder) {
    output.append('"')
    value.codePoints().forEach { codePoint ->
        when (codePoint) {
            0x22 -> output.append("\\\"")
            0x5c -> output.append("\\\\")
            0x08 -> output.append("\\b")
            0x0c -> output.append("\\f")
            0x0a -> output.append("\\n")
            0x0d -> output.append("\\r")
            0x09 -> output.append("\\t")
            in 0x00..0x1f -> output.append("\\u%04x".format(codePoint))
            else -> output.appendCodePoint(codePoint)
        }
    }
    output.append('"')
}

private fun digest(value: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(value).joinToString("") { "%02x".format(it) }
