package com.garive.mobile.application

import com.garive.mobile.model.MobileDestination
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put

internal data class RestoredMobilePreferences(
    val destination: MobileDestination = MobileDestination.WORK,
    val selectedSessionId: String? = null,
    val theme: String = "system",
    val drafts: LinkedHashMap<String, String> = linkedMapOf(),
)

internal fun saveMobilePreferences(
    persistence: MobileWorkPersistence,
    destination: MobileDestination,
    selectedSessionId: String?,
    theme: String,
    drafts: Map<String, String>,
) {
    require(theme in THEMES)
    require(drafts.size <= MAX_DRAFTS)
    persistence.writePreferencesRecord(buildJsonObject {
        put("schema_version", 1)
        put("selected_destination", destination.name.lowercase())
        selectedSessionId?.let { put("selected_session_id", checkedId(it)) }
        put("theme", theme)
        put("notification_preview", "status_only")
        put("drafts", buildJsonArray {
            drafts.entries.toList().takeLast(MAX_DRAFTS).forEach { (sessionId, text) ->
                require(text.encodeToByteArray().size <= MAX_DRAFT_BYTES)
                add(buildJsonObject {
                    put("session_id", checkedId(sessionId))
                    put("text", text)
                })
            }
        })
    }.toString())
}

internal fun restoreMobilePreferences(persistence: MobileWorkPersistence): RestoredMobilePreferences {
    val record = persistence.readPreferencesRecord() ?: return RestoredMobilePreferences()
    return try {
        decodeMobilePreferences(record)
    } catch (_: IllegalArgumentException) {
        persistence.writePreferencesRecord(null)
        RestoredMobilePreferences()
    }
}

private fun decodeMobilePreferences(record: String): RestoredMobilePreferences {
    if (record.encodeToByteArray().size > MAX_PREFERENCES_BYTES) invalidPreferences()
    val value = try { Json.parseToJsonElement(record).jsonObject } catch (_: Throwable) { invalidPreferences() }
    if (value.keys !in listOf(PREFERENCE_KEYS, PREFERENCE_KEYS - "selected_session_id") ||
        value.long("schema_version") != 1L || value.text("notification_preview") != "status_only"
    ) invalidPreferences()
    val destination = MobileDestination.entries.firstOrNull {
        it.name.lowercase() == value.text("selected_destination")
    } ?: invalidPreferences()
    val theme = value.text("theme").takeIf { it in THEMES } ?: invalidPreferences()
    val selected = value.optionalText("selected_session_id")?.also(::checkedId)
    val rawDrafts = value["drafts"] as? JsonArray ?: invalidPreferences()
    if (rawDrafts.size > MAX_DRAFTS) invalidPreferences()
    val drafts = linkedMapOf<String, String>()
    rawDrafts.forEach { raw ->
        val draft = raw as? JsonObject ?: invalidPreferences()
        if (draft.keys != DRAFT_KEYS) invalidPreferences()
        val sessionId = checkedId(draft.text("session_id"))
        val text = draft.optionalText("text") ?: invalidPreferences()
        if (text.encodeToByteArray().size > MAX_DRAFT_BYTES || drafts.put(sessionId, text) != null) {
            invalidPreferences()
        }
    }
    return RestoredMobilePreferences(destination, selected, theme, drafts)
}

private fun JsonObject.text(key: String): String =
    this[key]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotEmpty() } ?: invalidPreferences()
private fun JsonObject.optionalText(key: String): String? = this[key]?.jsonPrimitive?.contentOrNull
private fun JsonObject.long(key: String): Long = this[key]?.jsonPrimitive?.longOrNull ?: invalidPreferences()
private fun checkedId(value: String): String = value.also {
    if (it.encodeToByteArray().size !in 1..128 || it.any { character -> character.code !in 0x21..0x7e }) {
        invalidPreferences()
    }
}
private fun invalidPreferences(): Nothing = throw IllegalArgumentException("invalid_mobile_preferences")

private const val MAX_DRAFTS: Int = 20
private const val MAX_DRAFT_BYTES: Int = 16_384
private const val MAX_PREFERENCES_BYTES: Int = 350_000
private val THEMES: Set<String> = setOf("system", "light", "dark")
private val PREFERENCE_KEYS: Set<String> = setOf(
    "schema_version", "selected_destination", "selected_session_id", "theme",
    "notification_preview", "drafts",
)
private val DRAFT_KEYS: Set<String> = setOf("session_id", "text")
