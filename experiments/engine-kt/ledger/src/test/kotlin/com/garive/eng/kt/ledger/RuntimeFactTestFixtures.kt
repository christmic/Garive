package com.garive.eng.kt.ledger

import java.nio.file.Path
import kotlin.io.path.readText
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

private val runtimeFactCases by lazy {
    val root = Path.of(System.getProperty("garive.repo.root"))
    Json.parseToJsonElement(root.resolve("spec/fixtures/ledger/runtime-facts-v1.json").readText())
        .jsonObject.getValue("valid_cases").jsonArray
}

internal fun runtimePayload(kind: String, schemaVersion: UInt = 1u): JsonElement = runtimeFactCases
    .firstOrNull {
        val value = it.jsonObject
        value.getValue("kind").jsonPrimitive.content == kind &&
            (value["schema_version"]?.jsonPrimitive?.content?.toUIntOrNull() ?: 1u) == schemaVersion
    }
    ?.jsonObject?.getValue("payload")
    ?: JsonObject(emptyMap())
