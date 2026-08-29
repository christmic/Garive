package com.garive.eng.kt.tools

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.longOrNull

internal object PortableSchema {
    private const val DIALECT: String = "https://json-schema.org/draft/2020-12/schema"
    private val keywords: Set<String> = setOf(
        "\$schema", "\$id", "title", "description", "default", "examples", "deprecated",
        "readOnly", "writeOnly", "format", "type", "enum", "const", "properties",
        "required", "additionalProperties", "items", "minItems", "maxItems", "minLength",
        "maxLength", "minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum",
        "multipleOf", "allOf", "anyOf", "oneOf", "not",
    )
    private val types: Set<String> = setOf("object", "array", "string", "number", "integer", "boolean", "null")

    fun validateDefinition(schema: JsonElement): PreparationError? {
        val root = schema as? JsonObject ?: return invalidDefinition()
        if (root["type"]?.text() != "object" || root["properties"] !is JsonObject ||
            root["additionalProperties"] !is JsonPrimitive && root["additionalProperties"] !is JsonObject
        ) {
            return invalidDefinition()
        }
        return validateSchemaNode(root)
    }

    fun validateValueDefinition(schema: JsonElement): PreparationError? = validateSchemaNode(schema)

    private fun validateSchemaNode(schema: JsonElement): PreparationError? {
        val value = schema as? JsonObject ?: return invalidDefinition()
        if (value.keys.any { it !in keywords }) return PreparationError(PreparationErrorCode.UNSUPPORTED_SCHEMA_KEYWORD)
        if (value["\$schema"]?.text()?.let { it != DIALECT } == true) return invalidDefinition()
        if (value["type"]?.text()?.let { it !in types } == true) return invalidDefinition()
        val required = value["required"]
        if (required != null) {
            val names = (required as? JsonArray)?.mapNotNull { it.text() } ?: return invalidDefinition()
            if (names.size != required.size || names.distinct().size != names.size) return invalidDefinition()
        }
        for (keyword in listOf("minItems", "maxItems", "minLength", "maxLength")) {
            if (value[keyword]?.asNonNegativeLong() == null && value.containsKey(keyword)) return invalidDefinition()
        }
        for (keyword in listOf("minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum")) {
            if (value[keyword]?.number() == null && value.containsKey(keyword)) return invalidDefinition()
        }
        value["multipleOf"]?.let { if ((it.number() ?: return invalidDefinition()) <= 0.0) return invalidDefinition() }
        value["enum"]?.let {
            val entries = it as? JsonArray ?: return invalidDefinition()
            if (entries.isEmpty() || entries.distinct().size != entries.size) return invalidDefinition()
        }
        (value["properties"] as? JsonObject)?.values?.forEach { validateSchemaNode(it)?.let { error -> return error } }
        value["additionalProperties"]?.let {
            if (it is JsonObject) validateSchemaNode(it)?.let { error -> return error }
            else if ((it as? JsonPrimitive)?.booleanOrNull == null) return invalidDefinition()
        }
        value["items"]?.let { validateSchemaNode(it)?.let { error -> return error } }
        for (keyword in listOf("allOf", "anyOf", "oneOf")) {
            value[keyword]?.let {
                val branches = it as? JsonArray ?: return invalidDefinition()
                if (branches.isEmpty()) return invalidDefinition()
                branches.forEach { branch -> validateSchemaNode(branch)?.let { error -> return error } }
            }
        }
        value["not"]?.let { validateSchemaNode(it)?.let { error -> return error } }
        return null
    }

    fun validateArguments(schema: JsonElement, instance: JsonElement): List<SchemaFailure> {
        val failures = mutableListOf<SchemaFailure>()
        validateNode(schema as JsonObject, instance, "", "", failures)
        return failures.sortedWith(compareBy(SchemaFailure::instancePath, SchemaFailure::schemaPath, SchemaFailure::keyword))
    }

    private fun validateNode(schema: JsonObject, instance: JsonElement, ip: String, sp: String, out: MutableList<SchemaFailure>) {
        schema["type"]?.text()?.let { if (!matchesType(instance, it)) { out += failure(ip, sp, "type"); return } }
        (schema["enum"] as? JsonArray)?.let { if (instance !in it) out += failure(ip, sp, "enum") }
        schema["const"]?.let { if (it != instance) out += failure(ip, sp, "const") }
        validateObject(schema, instance, ip, sp, out)
        validateArray(schema, instance, ip, sp, out)
        validateString(schema, instance, ip, sp, out)
        validateNumber(schema, instance, ip, sp, out)
        validateComposition(schema, instance, ip, sp, out)
    }

    private fun validateObject(schema: JsonObject, instance: JsonElement, ip: String, sp: String, out: MutableList<SchemaFailure>) {
        val value = instance as? JsonObject ?: return
        (schema["required"] as? JsonArray)?.mapNotNull { it.text() }?.forEach { if (it !in value) out += failure(ip, sp, "required") }
        val properties = schema["properties"] as? JsonObject
        properties?.forEach { (name, child) -> value[name]?.let { validateNode(child as JsonObject, it, join(ip, name), join(join(sp, "properties"), name), out) } }
        value.forEach { (name, item) ->
            if (properties?.containsKey(name) == true) return@forEach
            when (val additional = schema["additionalProperties"]) {
                is JsonPrimitive -> if (additional.booleanOrNull == false) out += failure(join(ip, name), sp, "additionalProperties")
                is JsonObject -> validateNode(additional, item, join(ip, name), join(sp, "additionalProperties"), out)
                else -> Unit
            }
        }
    }

    private fun validateArray(schema: JsonObject, instance: JsonElement, ip: String, sp: String, out: MutableList<SchemaFailure>) {
        val value = instance as? JsonArray ?: return
        boundary(schema, "minItems", value.size.toLong(), { a, b -> a >= b }, ip, sp, out)
        boundary(schema, "maxItems", value.size.toLong(), { a, b -> a <= b }, ip, sp, out)
        (schema["items"] as? JsonObject)?.let { child -> value.forEachIndexed { index, item -> validateNode(child, item, join(ip, index.toString()), join(sp, "items"), out) } }
    }

    private fun validateString(schema: JsonObject, instance: JsonElement, ip: String, sp: String, out: MutableList<SchemaFailure>) {
        val value = (instance as? JsonPrimitive)?.takeIf { it.isString }?.content ?: return
        val count = value.codePointCount(0, value.length).toLong()
        boundary(schema, "minLength", count, { a, b -> a >= b }, ip, sp, out)
        boundary(schema, "maxLength", count, { a, b -> a <= b }, ip, sp, out)
    }

    private fun validateNumber(schema: JsonObject, instance: JsonElement, ip: String, sp: String, out: MutableList<SchemaFailure>) {
        val value = instance.number() ?: return
        val checks = listOf(
            "minimum" to { bound: Double -> value < bound }, "maximum" to { bound: Double -> value > bound },
            "exclusiveMinimum" to { bound: Double -> value <= bound }, "exclusiveMaximum" to { bound: Double -> value >= bound },
        )
        checks.forEach { (keyword, check) -> schema[keyword]?.number()?.let { if (check(it)) out += failure(ip, sp, keyword) } }
        schema["multipleOf"]?.number()?.let { unit ->
            val quotient = value / unit
            if (kotlin.math.abs(quotient - kotlin.math.round(quotient)) > Math.ulp(quotient) * 4.0) out += failure(ip, sp, "multipleOf")
        }
    }

    private fun validateComposition(schema: JsonObject, instance: JsonElement, ip: String, sp: String, out: MutableList<SchemaFailure>) {
        for (keyword in listOf("allOf", "anyOf", "oneOf")) {
            val branches = schema[keyword] as? JsonArray ?: continue
            val results = branches.map { branch -> mutableListOf<SchemaFailure>().also { validateNode(branch as JsonObject, instance, ip, join(sp, keyword), it) } }
            val successes = results.count { it.isEmpty() }
            if (keyword == "allOf") results.forEach(out::addAll)
            else if (keyword == "anyOf" && successes == 0 || keyword == "oneOf" && successes != 1) out += failure(ip, sp, keyword)
        }
        (schema["not"] as? JsonObject)?.let { child ->
            val inner = mutableListOf<SchemaFailure>()
            validateNode(child, instance, ip, join(sp, "not"), inner)
            if (inner.isEmpty()) out += failure(ip, sp, "not")
        }
    }

    private fun matchesType(value: JsonElement, type: String): Boolean = when (type) {
        "object" -> value is JsonObject; "array" -> value is JsonArray
        "string" -> value is JsonPrimitive && value.isString
        "number" -> value.number() != null
        "integer" -> value.number()?.rem(1.0) == 0.0
        "boolean" -> (value as? JsonPrimitive)?.booleanOrNull != null
        "null" -> value is JsonNull
        else -> false
    }

    private fun boundary(schema: JsonObject, keyword: String, actual: Long, check: (Long, Long) -> Boolean, ip: String, sp: String, out: MutableList<SchemaFailure>) {
        schema[keyword]?.asNonNegativeLong()?.let { if (!check(actual, it)) out += failure(ip, sp, keyword) }
    }
    private fun failure(ip: String, sp: String, keyword: String): SchemaFailure = SchemaFailure(ip, join(sp, keyword), keyword)
    private fun join(base: String, token: String): String = "$base/${token.replace("~", "~0").replace("/", "~1")}"
    private fun invalidDefinition(): PreparationError = PreparationError(PreparationErrorCode.INVALID_TOOL_DEFINITION)
    private fun JsonElement.text(): String? = (this as? JsonPrimitive)?.takeIf { it.isString }?.contentOrNull
    private fun JsonElement.number(): Double? = (this as? JsonPrimitive)?.takeIf { !it.isString }?.doubleOrNull
    private fun JsonElement.asNonNegativeLong(): Long? = (this as? JsonPrimitive)?.takeIf { !it.isString }?.longOrNull?.takeIf { it >= 0 }
}
