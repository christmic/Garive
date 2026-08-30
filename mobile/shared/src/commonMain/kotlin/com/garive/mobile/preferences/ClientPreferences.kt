package com.garive.mobile.preferences

import com.garive.mobile.model.CommandKind
import com.garive.mobile.model.PendingCommand
import com.garive.mobile.model.PendingStatus
import kotlinx.serialization.json.*

/** Session rail presentation preference. */
public enum class SessionRail(public val wireName: String) { EXPANDED("expanded"), COLLAPSED("collapsed") }
/** Activity inspector presentation preference. */
public enum class InspectorState(public val wireName: String) { OPEN("open"), CLOSED("closed") }
/** Theme preference. */
public enum class Theme(public val wireName: String) { SYSTEM("system"), LIGHT("light"), DARK("dark") }

/** One bounded local-only draft. */
public data class PreferenceDraft(public val sessionId: String, public val text: String)

/** Strict non-secret local UI preferences. */
public data class ClientPreferencesV1(
    public val selectedSessionId: String? = null,
    public val sessionRail: SessionRail = SessionRail.EXPANDED,
    public val activityInspector: InspectorState = InspectorState.CLOSED,
    public val theme: Theme = Theme.SYSTEM,
    public val composerDrafts: List<PreferenceDraft> = emptyList(),
)

/** Explicit parser and persistence bounds. */
public data class PreferenceLimits(
    public val maxDocumentBytes: Int, public val maxDrafts: Int,
    public val maxIdBytes: Int, public val maxDraftBytes: Int,
)

/** Platform-owned private byte persistence; no browser/OS mechanism is implied. */
public interface PreferenceBytesPort {
    public suspend fun readPreferences(): ByteArray?
    public suspend fun writePreferences(value: ByteArray): Unit
    public suspend fun readPendingCommand(): ByteArray?
    public suspend fun writePendingCommand(value: ByteArray?): Unit
}

/** Preference load result that distinguishes safe reset from an absent document. */
public data class PreferenceLoadResult(
    public val preferences: ClientPreferencesV1,
    public val reset: Boolean,
    public val pending: PendingCommand? = null,
)

/** Strict JSON adapter over an injected platform persistence port. */
public class JsonPreferenceAdapter(
    private val port: PreferenceBytesPort,
    private val limits: PreferenceLimits,
) {
    init { require(validLimits(limits)) { "invalid_preference_limits" } }

    /** Loads preferences, resetting only disposable local state on corruption. */
    public suspend fun load(): PreferenceLoadResult {
        val rawPreferences = port.readPreferences()
        val rawPending = port.readPendingCommand()
        var preferences = ClientPreferencesV1()
        var reset = false
        if (rawPreferences != null) {
            try { preferences = decodePreferences(rawPreferences, limits) } catch (_: IllegalArgumentException) { reset = true }
        }
        var pending: PendingCommand? = null
        if (rawPending != null) {
            try { pending = decodePendingCommand(rawPending, limits) } catch (_: IllegalArgumentException) {
                reset = true; port.writePendingCommand(null)
            }
        }
        return PreferenceLoadResult(preferences, reset, pending)
    }

    /** Writes one fully validated preference document. */
    public suspend fun save(preferences: ClientPreferencesV1): Unit = port.writePreferences(encodePreferences(preferences, limits))

    /** Writes or explicitly clears the separate pending-command record. */
    public suspend fun savePending(command: PendingCommand?): Unit =
        port.writePendingCommand(command?.let { encodePendingCommand(it, limits) })
}

/** Decodes one strict preference document. */
public fun decodePreferences(bytes: ByteArray, limits: PreferenceLimits): ClientPreferencesV1 {
    validateSize(bytes, limits)
    val value = parseObject(bytes); exactKeys(value, PREF_KEYS)
    if (value["schema_version"]?.jsonPrimitive?.intOrNull != 1) invalid()
    val rail = enumValue<SessionRail>(requiredText(value, "session_rail")) { it.wireName }
    val inspector = enumValue<InspectorState>(requiredText(value, "activity_inspector")) { it.wireName }
    val theme = enumValue<Theme>(requiredText(value, "theme")) { it.wireName }
    val selected = value["selected_session_id"]?.let { requiredId(it.jsonPrimitive.content, limits) }
    val rawDrafts = value["composer_drafts"] as? JsonArray ?: invalid()
    if (rawDrafts.size > limits.maxDrafts) invalid()
    val seen = mutableSetOf<String>()
    val drafts = rawDrafts.map { raw ->
        val draft = raw as? JsonObject ?: invalid(); exactKeys(draft, DRAFT_KEYS)
        val sessionId = requiredId(requiredText(draft, "session_id"), limits)
        val text = requiredTextAllowEmpty(draft, "text")
        if (!seen.add(sessionId) || text.encodeToByteArray().size > limits.maxDraftBytes) invalid()
        PreferenceDraft(sessionId, text)
    }
    return ClientPreferencesV1(selected, rail, inspector, theme, drafts)
}

/** Encodes one validated preference document. */
public fun encodePreferences(value: ClientPreferencesV1, limits: PreferenceLimits): ByteArray {
    val document = buildJsonObject {
        put("schema_version", 1)
        value.selectedSessionId?.let { put("selected_session_id", it) }
        put("session_rail", value.sessionRail.wireName)
        put("activity_inspector", value.activityInspector.wireName)
        put("theme", value.theme.wireName)
        putJsonArray("composer_drafts") { value.composerDrafts.forEach { draft ->
            addJsonObject { put("session_id", draft.sessionId); put("text", draft.text) }
        } }
    }.toString().encodeToByteArray()
    decodePreferences(document, limits); return document
}

/** Decodes the separate exact pending-command record. */
public fun decodePendingCommand(bytes: ByteArray, limits: PreferenceLimits): PendingCommand {
    validateSize(bytes, limits)
    val value = parseObject(bytes); exactKeys(value, PENDING_KEYS)
    if (value["schema_version"]?.jsonPrimitive?.intOrNull != 1) invalid()
    val kind = enumValue<CommandKind>(requiredText(value, "kind")) { it.wireName }
    val status = enumValue<PendingStatus>(requiredText(value, "status")) { it.wireName }
    val digest = requiredText(value, "semantic_request_digest")
    val generation = value["issued_generation"]?.jsonPrimitive?.longOrNull ?: invalid()
    if (!HEX_DIGEST.matches(digest) || generation < 0) invalid()
    return PendingCommand(kind, requiredId(requiredText(value, "command_id"), limits), digest, generation,
        optionalId(value, "session_id", limits), optionalId(value, "turn_id", limits), status)
}

/** Encodes the separate exact pending-command record. */
public fun encodePendingCommand(value: PendingCommand, limits: PreferenceLimits): ByteArray {
    val document = buildJsonObject {
        put("schema_version", 1); put("kind", value.kind.wireName); put("command_id", value.commandId)
        put("semantic_request_digest", value.requestDigest); value.sessionId?.let { put("session_id", it) }
        value.turnId?.let { put("turn_id", it) }; put("issued_generation", value.generation)
        put("status", value.status.wireName)
    }.toString().encodeToByteArray()
    decodePendingCommand(document, limits); return document
}

private inline fun <reified T : Enum<T>> enumValue(value: String, wire: (T) -> String): T =
    enumValues<T>().firstOrNull { wire(it) == value } ?: invalid()
private fun validateSize(bytes: ByteArray, limits: PreferenceLimits): Unit {
    if (!validLimits(limits) || bytes.size > limits.maxDocumentBytes) invalid()
}
private fun parseObject(bytes: ByteArray): JsonObject = try {
    Json.parseToJsonElement(bytes.decodeToString(throwOnInvalidSequence = true)) as? JsonObject ?: invalid()
} catch (_: Throwable) { invalid() }
private fun exactKeys(value: JsonObject, allowed: Set<String>): Unit { if (value.keys.any { it !in allowed }) invalid() }
private fun requiredText(value: JsonObject, key: String): String =
    value[key]?.jsonPrimitive?.content?.takeIf { it.isNotEmpty() } ?: invalid()
private fun requiredTextAllowEmpty(value: JsonObject, key: String): String = value[key]?.jsonPrimitive?.content ?: invalid()
private fun optionalId(value: JsonObject, key: String, limits: PreferenceLimits): String? =
    value[key]?.let { requiredId(it.jsonPrimitive.content, limits) }
private fun requiredId(value: String, limits: PreferenceLimits): String {
    if (value.isEmpty() || value.encodeToByteArray().size > limits.maxIdBytes || value.any { it.code in 0..31 || it.code == 127 }) invalid()
    return value
}
private fun validLimits(value: PreferenceLimits): Boolean = value.maxDocumentBytes > 0 && value.maxDrafts > 0 && value.maxIdBytes > 0 && value.maxDraftBytes > 0
private fun invalid(): Nothing = throw IllegalArgumentException("invalid_local_preference")
private val HEX_DIGEST: Regex = Regex("[0-9a-f]{64}")
private val PREF_KEYS: Set<String> = setOf("schema_version", "selected_session_id", "session_rail", "activity_inspector", "theme", "composer_drafts")
private val DRAFT_KEYS: Set<String> = setOf("session_id", "text")
private val PENDING_KEYS: Set<String> = setOf("schema_version", "kind", "command_id", "semantic_request_digest", "session_id", "turn_id", "issued_generation", "status")
