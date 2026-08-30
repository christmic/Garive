package com.garive.mobile.host

import com.garive.host.v1.AgentDefinitionPageV1
import com.garive.host.v1.AgentDefinitionSummaryV1
import com.garive.host.v1.HostActivityV1
import com.garive.host.v1.SessionPageV1
import com.garive.host.v1.SessionSummaryV1
import com.garive.host.v1.SessionViewV1
import com.garive.host.v1.SuspensionViewV1
import com.garive.host.v1.TurnTimelineItemV1
import com.garive.host.v1.TurnTimelinePageV1
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import okio.ByteString.Companion.encodeUtf8

/** Maps strict H2/H3 JSON responses into Wire-generated public values. */
internal fun decodeAgentDefinitionPage(value: JsonObject): AgentDefinitionPageV1 {
    value.requireApiVersion()
    return AgentDefinitionPageV1(
        api_version = API_VERSION,
        definitions = value.array("definitions").map { raw ->
            val definition = raw.jsonObject.also { it.requireApiVersion() }
            AgentDefinitionSummaryV1(
                api_version = API_VERSION,
                definition_id = definition.text("definition_id"),
                definition_revision = definition.text("definition_revision"),
                capabilities = definition.array("capabilities").map { it.jsonPrimitive.content },
            )
        },
    )
}

/** Maps a bounded Session page into generated public values. */
internal fun decodeSessionPage(value: JsonObject): SessionPageV1 {
    value.requireApiVersion()
    return SessionPageV1(
        api_version = API_VERSION,
        sessions = value.array("sessions").map { decodeSessionSummary(it.jsonObject) },
        next_before = value.optionalText("next_before"),
    )
}

/** Maps one exact Session response into generated public values. */
internal fun decodeSessionView(value: JsonObject): SessionViewV1 {
    value.requireApiVersion()
    return SessionViewV1(
        api_version = API_VERSION,
        session = decodeSessionSummary(value.objectValue("session")),
        observed_max_position = value.positiveLong("observed_max_position"),
    )
}

/** Maps one bounded Turn timeline response into generated public values. */
internal fun decodeTimelinePage(value: JsonObject): TurnTimelinePageV1 {
    value.requireApiVersion()
    return TurnTimelinePageV1(
        api_version = API_VERSION,
        session_id = value.text("session_id"),
        items = value.array("items").map { raw ->
            val item = raw.jsonObject
            TurnTimelineItemV1(
                turn_id = item.text("turn_id"),
                started_position = item.positiveLong("started_position"),
                latest_position = item.positiveLong("latest_position"),
                state = item.text("state"),
                user_text = item.text("user_text"),
                completion_text = item.optionalText("completion_text"),
                suspension = item.optionalObject("suspension")?.let(::decodeSuspension),
                content_truncated = item.booleanValue("content_truncated"),
                activities = item.array("activities").map { decodeActivity(it.jsonObject) },
            )
        },
        scanned_through_position = value.positiveLong("scanned_through_position"),
        observed_max_position = value.positiveLong("observed_max_position"),
        has_more = value.booleanValue("has_more"),
    )
}

private fun decodeSessionSummary(value: JsonObject): SessionSummaryV1 {
    value.requireApiVersion()
    return SessionSummaryV1(
        api_version = API_VERSION,
        session_id = value.text("session_id"),
        agent_instance_id = value.text("agent_instance_id"),
        definition_id = value.text("definition_id"),
        definition_revision = value.text("definition_revision"),
        opened_at = value.text("opened_at"),
        latest_position = value.positiveLong("latest_position"),
        latest_turn_id = value.optionalText("latest_turn_id"),
        latest_turn_state = value.optionalText("latest_turn_state"),
        turn_count = value.nonNegativeLong("turn_count"),
    )
}

private fun decodeSuspension(value: JsonObject): SuspensionViewV1 {
    val prompt = value.text("prompt_json").encodeUtf8()
    val promptDigest = value.text("prompt_digest")
    val response = value.optionalText("response_schema_json")?.encodeUtf8()
    val responseDigest = value.optionalText("response_schema_digest")
    if (value.text("prompt_schema") != "garive.public-suspension-prompt.v1" ||
        prompt.sha256().hex() != promptDigest ||
        (response == null) != (responseDigest == null) ||
        response != null && response.sha256().hex() != responseDigest
    ) fail(HostClientError.INVALID_EVENT)
    return SuspensionViewV1(
        suspension_id = value.text("suspension_id"),
        session_version = value.positiveLong("session_version"),
        kind = value.text("kind"),
        prompt_schema = "garive.public-suspension-prompt.v1",
        prompt_json = prompt,
        prompt_digest = promptDigest,
        response_schema_json = response,
        response_schema_digest = responseDigest,
    )
}

private fun decodeActivity(value: JsonObject): HostActivityV1 {
    value.requireApiVersion()
    return HostActivityV1(
        api_version = API_VERSION,
        activity_id = value.text("activity_id"),
        kind = value.text("kind"),
        label_key = value.text("label_key"),
        state = value.text("state"),
        source_position = value.positiveLong("source_position"),
        terminal = value.booleanValue("terminal"),
        safe_code = value.optionalText("safe_code"),
    )
}

private fun JsonObject.requireApiVersion(): Unit {
    if (text("api_version") != API_VERSION) fail(HostClientError.INVALID_EVENT)
}

private fun JsonObject.text(key: String): String = try {
    getValue(key).jsonPrimitive.content.also { if (it.isEmpty()) fail(HostClientError.INVALID_EVENT) }
} catch (error: HostClientException) {
    throw error
} catch (_: Throwable) {
    fail(HostClientError.INVALID_EVENT)
}

private fun JsonObject.optionalText(key: String): String? = try {
    val element = this[key] ?: return null
    if (element is JsonNull) return null
    element.jsonPrimitive.contentOrNull?.also { if (it.isEmpty()) fail(HostClientError.INVALID_EVENT) }
} catch (error: HostClientException) {
    throw error
} catch (_: Throwable) {
    fail(HostClientError.INVALID_EVENT)
}

private fun JsonObject.array(key: String): JsonArray = try {
    getValue(key).jsonArray
} catch (_: Throwable) {
    fail(HostClientError.INVALID_EVENT)
}

private fun JsonObject.objectValue(key: String): JsonObject = try {
    getValue(key).jsonObject
} catch (_: Throwable) {
    fail(HostClientError.INVALID_EVENT)
}

private fun JsonObject.optionalObject(key: String): JsonObject? = try {
    val element = this[key] ?: return null
    if (element is JsonNull) null else element.jsonObject
} catch (_: Throwable) {
    fail(HostClientError.INVALID_EVENT)
}

private fun JsonObject.positiveLong(key: String): Long =
    nonNegativeLong(key).also { if (it == 0L) fail(HostClientError.INVALID_EVENT) }

private fun JsonObject.nonNegativeLong(key: String): Long = try {
    getValue(key).jsonPrimitive.long.also { if (it < 0) fail(HostClientError.INVALID_EVENT) }
} catch (error: HostClientException) {
    throw error
} catch (_: Throwable) {
    fail(HostClientError.INVALID_EVENT)
}

private fun JsonObject.booleanValue(key: String): Boolean = try {
    getValue(key).jsonPrimitive.boolean
} catch (_: Throwable) {
    fail(HostClientError.INVALID_EVENT)
}
