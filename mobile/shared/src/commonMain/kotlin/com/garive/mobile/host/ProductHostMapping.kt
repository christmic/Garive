package com.garive.mobile.host

import com.garive.host.v1.*
import com.garive.mobile.application.AppEffectPayload
import com.garive.mobile.model.*
import kotlinx.serialization.json.*

/** Safe protocol mapping failure for the product composition root. */
public class ProductHostMappingException(public val error: AppError) :
    Exception("${error.kind.wireName}:${error.code}")

/** Maps generated H2 installed definitions into immutable product values. */
@Throws(ProductHostMappingException::class)
public fun AgentDefinitionPageV1.toDefinitionsLoaded(): AppEffectPayload.DefinitionsLoaded {
    version(api_version)
    val definitions = definitions.map { value ->
        version(value.api_version); required(value.definition_id); required(value.definition_revision)
        if (value.capabilities.any { it.isEmpty() } || value.capabilities != value.capabilities.distinct().sorted()) invalid()
        DefinitionItem(value.definition_id, value.definition_revision, value.capabilities)
    }
    if (definitions.map { it.definitionId }.distinct().size != definitions.size) invalid()
    return AppEffectPayload.DefinitionsLoaded(definitions)
}

/** Maps a generated H2 Session page without manufacturing titles or durable state. */
@Throws(ProductHostMappingException::class)
public fun SessionPageV1.toSessionPageLoaded(): AppEffectPayload.SessionPageLoaded {
    version(api_version)
    val mapped = sessions.map { value ->
        version(value.api_version); required(value.session_id); required(value.agent_instance_id)
        required(value.definition_id); required(value.definition_revision); required(value.opened_at)
        if (value.latest_position <= 0 || value.turn_count < 0 ||
            value.latest_turn_id == null != (value.latest_turn_state == null)
        ) invalid()
        SessionItem(value.session_id, value.agent_instance_id, value.definition_id, value.definition_revision,
            value.opened_at, value.latest_position, value.latest_turn_id, value.latest_turn_state, value.turn_count)
    }
    if (mapped.map { it.sessionId }.distinct().size != mapped.size) invalid()
    return AppEffectPayload.SessionPageLoaded(mapped)
}

/** Maps one complete generated H2/H3 timeline page for the expected Session. */
@Throws(ProductHostMappingException::class)
public fun TurnTimelinePageV1.toTimelineLoaded(expectedSessionId: String): AppEffectPayload.TimelineLoaded {
    version(api_version); required(expectedSessionId)
    if (session_id != expectedSessionId || scanned_through_position < 0 || observed_max_position < scanned_through_position) invalid()
    val items = items.map { value ->
        required(value.turn_id); required(value.state); required(value.user_text)
        if (value.started_position <= 0 || value.latest_position < value.started_position) invalid()
        val activity = value.activities.map { it.toProductActivity(value.turn_id) }
        TimelineItem(value.turn_id, value.state, value.latest_position, value.started_position, value.user_text,
            value.completion_text, value.suspension?.toProductSuspension(), value.content_truncated, activity)
    }
    if (items.map { it.turnId }.distinct().size != items.size ||
        items.zipWithNext().any { (left, right) -> left.latestPosition > right.latestPosition }
    ) invalid()
    return AppEffectPayload.TimelineLoaded(items, scanned_through_position, items.flatMap { it.activities })
}

/** Maps one generated H1/H3 event while preserving unknown names neutrally. */
@Throws(ProductHostMappingException::class)
public fun HostEventV1.toProductEvent(expectedSessionId: String): AppEffectPayload.HostEvent {
    version(api_version); required(expectedSessionId); required(event)
    if (session_id != expectedSessionId || position <= 0) invalid()
    val turn = turn_id.takeIf { it.isNotEmpty() }
    return AppEffectPayload.HostEvent(event, position, turn, activity?.toProductActivity(turn))
}

private fun HostActivityV1.toProductActivity(turnId: String?): ActivityItem {
    version(api_version); required(activity_id); required(kind); required(label_key); required(state)
    if (source_position <= 0 || safe_code?.isEmpty() == true) invalid()
    return ActivityItem(activity_id, kind, state, turnId, source_position, false, label_key, terminal, safe_code)
}

private fun SuspensionViewV1.toProductSuspension(): SuspensionItem {
    required(suspension_id); required(kind); required(prompt_schema); required(prompt_digest)
    if (session_version <= 0 || !HEX.matches(prompt_digest) ||
        (response_schema_json == null) != (response_schema_digest == null) ||
        response_schema_digest?.let { !HEX.matches(it) } == true
    ) invalid()
    val prompt = try {
        Json.parseToJsonElement(prompt_json.toByteArray().decodeToString(throwOnInvalidSequence = true)).jsonObject
    } catch (_: Throwable) { invalid() }
    if (prompt.keys.any { it !in PROMPT_KEYS } || prompt["schema_version"]?.jsonPrimitive?.intOrNull != 1) invalid()
    val title = prompt.requiredText("title_key")
    val action = prompt.requiredText("action_label_key")
    val message = prompt.optionalText("message_text")
    val cancel = prompt.optionalText("cancel_label_key")
    return SuspensionItem(suspension_id, session_version, kind, title, message, action, cancel,
        prompt_digest, response_schema_digest)
}

private fun JsonObject.requiredText(key: String): String = optionalText(key)?.takeIf { it.isNotEmpty() } ?: invalid()
private fun JsonObject.optionalText(key: String): String? = try {
    get(key)?.jsonPrimitive?.content
} catch (_: Throwable) { invalid() }
private fun version(value: String): Unit { if (value != "v1") invalid() }
private fun required(value: String): Unit { if (value.isEmpty()) invalid() }
private fun invalid(): Nothing = throw ProductHostMappingException(AppError(AppErrorKind.PROTOCOL, "invalid_host_value"))
private val HEX: Regex = Regex("[0-9a-f]{64}")
private val PROMPT_KEYS: Set<String> = setOf("schema_version", "title_key", "message_text", "action_label_key", "cancel_label_key")
