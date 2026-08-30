package com.garive.mobile.application

import com.garive.mobile.model.*
import com.garive.mobile.preferences.*
import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.*
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.*

public class ClientProductExperienceFixtureTest {
    private val fixture: JsonObject by lazy {
        val root = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(root.resolve("spec/fixtures/host/client-product-experience-v1.json").readText()).jsonObject
    }
    private val families: List<String> = listOf(
        "bootstrap_cases", "navigation_cases", "command_cases", "follow_cases",
        "suspension_cases", "activity_cases", "preference_cases", "failure_cases",
    )

    @Test
    public fun consumesEveryOrderedStateMachineScenario(): Unit {
        validateFixture()
        families.take(6).forEach { family -> fixture.array(family).forEach { runControllerCase(it.jsonObject) } }
    }

    @Test
    public fun strictlyResetsEveryInvalidPreferenceDocument(): Unit = runBlocking {
        fixture.array("preference_cases").forEach { raw ->
            val test = raw.jsonObject
            val port = MemoryPort(test.getValue("document").toString().encodeToByteArray())
            val loaded = JsonPreferenceAdapter(port, preferenceLimits()).load()
            assertEquals(test.boolean("expected_reset"), loaded.reset, test.text("name"))
            assertEquals(test.long("expected_draft_count").toInt(), loaded.preferences.composerDrafts.size, test.text("name"))
            if (!loaded.reset) JsonPreferenceAdapter(port, preferenceLimits()).save(loaded.preferences)
        }
    }

    @Test
    public fun preservesCompleteSafeErrorVocabulary(): Unit {
        val actual = fixture.array("failure_cases").map { raw ->
            val test = raw.jsonObject; val error = test.obj("error")
            assertEquals(setOf("kind", "code"), error.keys)
            assertEquals(test.text("expected_public_kind"), error.text("kind"))
            assertFalse(error.toString().contains("raw_body"))
            error.text("kind")
        }
        assertEquals(listOf("configuration", "validation", "command_unknown", "host", "transport", "protocol", "local_preference"), actual)
    }

    private fun runControllerCase(test: JsonObject): Unit {
        var state = decodeState(test.obj("initial_state"))
        val emitted = mutableListOf<AppEffect>(); val aliases = mutableMapOf<String, AppEffect>()
        test.array("steps").forEach { rawStep ->
            val step = rawStep.jsonObject
            val reduction = when {
                "intent" in step -> reduceApp(state, decodeIntent(step.obj("intent")), controllerLimits())
                "seed_effect" in step -> {
                    val seed = step.obj("seed_effect")
                    val effect = AppEffect("effect-${state.nextEffect}", effectKind(seed.text("kind")), state.generation,
                        sessionId = seed.optionalText("session_id"), afterPosition = seed.optionalLong("after_position"))
                    state = state.copy(nextEffect = state.nextEffect + 1, outstanding = state.outstanding + effect)
                    emitted += effect
                    return@forEach
                }
                else -> {
                    val resolve = step.obj("resolve")
                    val effect = resolve.optionalText("alias")?.let { aliases[it] }
                        ?: state.outstanding.firstOrNull { it.kind.wireName == resolve.text("effect_kind") }
                        ?: error("missing effect in ${test.text("name")}")
                    reduceApp(state, AppIntent.EffectResult(effect.effectId, effect.generation, effect.sessionId,
                        effect.requestDigest, decodeResult(resolve.obj("result"))), controllerLimits())
                }
            }
            state = reduction.state; emitted += reduction.effects
            step["capture"]?.jsonObject?.let { capture ->
                val effect = reduction.effects.firstOrNull { it.kind.wireName == capture.text("effect_kind") }
                    ?: error("capture effect missing")
                aliases[capture.text("as")] = effect
            }
        }
        assertEquals(test.array("expected_effects").map { it.jsonPrimitive.content }, emitted.map { it.kind.wireName }, test.text("name"))
        assertEquals(test.getValue("expected_state"), project(state), test.text("name"))
        test.optionalText("expected_retried_command_id")?.let { expected ->
            val starts = emitted.filter { it.kind == EffectKind.START_TURN }
            assertEquals(2, starts.size); assertEquals(expected, starts[1].commandId)
            assertEquals(test.text("expected_retried_request_digest"), starts[1].requestDigest)
        }
        test["expected_effect_binding"]?.jsonObject?.let { binding ->
            val effect = emitted.first { it.kind == EffectKind.CONTINUE_TURN }
            assertEquals(binding.text("suspension_id"), effect.suspensionId)
            assertEquals(binding.long("session_version"), effect.sessionVersion)
            assertEquals(binding.text("response_schema_digest"), effect.responseSchemaDigest)
        }
    }

    private fun decodeState(raw: JsonObject): AppViewState = initialAppViewState(configuration(raw.text("configuration"))).copy(
        shell = shell(raw.text("shell")), generation = raw.long("generation"),
        definitionIds = raw.array("definition_ids").map { it.jsonPrimitive.content },
        sessions = raw.array("session_ids").map { SessionItem(it.jsonPrimitive.content) },
        selectedSessionId = raw.nullableText("selected_session_id"),
        timelineSessionId = raw.nullableText("selected_session_id"),
        timeline = raw.array("timeline").map { decodeTimeline(it.jsonObject) }, cursor = raw.long("cursor"),
        drafts = raw.array("drafts").map { Draft(it.jsonObject.text("session_id"), it.jsonObject.text("text")) },
        execution = execution(raw.text("execution")),
        pending = raw.array("pending").map { decodePending(it.jsonObject) },
        activities = raw.array("activities").map { decodeActivity(it.jsonObject) },
        notice = raw["notice"]?.takeUnless { it is JsonNull }?.jsonObject?.let { decodeError(it) },
    )

    private fun decodeIntent(raw: JsonObject): AppIntent = when (raw.text("type")) {
        "boot" -> AppIntent.Boot
        "select_session" -> AppIntent.SelectSession(raw.text("session_id"))
        "edit_draft" -> AppIntent.EditDraft(raw.text("session_id"), raw.text("text"))
        "submit_draft" -> AppIntent.SubmitDraft(raw.text("session_id"), raw.text("command_id"), raw.text("request_digest"))
        "retry_pending" -> AppIntent.RetryPending(raw.optionalText("session_id"))
        "reconnect" -> AppIntent.Reconnect(raw.text("session_id"))
        "continue_suspension" -> AppIntent.ContinueSuspension(raw.text("session_id"), raw.text("turn_id"),
            raw.text("input"), raw.text("command_id"), raw.text("request_digest"))
        else -> error("unknown fixture intent")
    }

    private fun decodeResult(raw: JsonObject): AppEffectPayload = when (raw.text("type")) {
        "preferences_loaded" -> AppEffectPayload.PreferencesLoaded(raw.nullableText("selected_session_id"),
            raw.array("drafts").map { Draft(it.jsonObject.text("session_id"), it.jsonObject.text("text")) })
        "definitions_loaded" -> AppEffectPayload.DefinitionsLoaded(raw.array("definition_ids").map { it.jsonPrimitive.content })
        "session_page_loaded" -> AppEffectPayload.SessionPageLoaded(raw.array("sessions").map { SessionItem(it.jsonObject.text("session_id")) })
        "timeline_loaded" -> AppEffectPayload.TimelineLoaded(raw.array("items").map { decodeTimeline(it.jsonObject) },
            raw.long("cursor"), raw.array("activities").map { decodeActivity(it.jsonObject) })
        "command_succeeded" -> AppEffectPayload.CommandSucceeded(raw.text("session_id"), raw.optionalText("turn_id"), raw.long("committed_position"))
        "host_event" -> AppEffectPayload.HostEvent(raw.text("event"), raw.long("position"), raw.optionalText("turn_id"),
            raw["activity"]?.jsonObject?.let { decodeActivity(it) })
        "event_stream_ended" -> AppEffectPayload.EventStreamEnded
        "failed" -> AppEffectPayload.Failed(decodeError(raw.obj("error")))
        else -> error("unknown fixture result")
    }

    private fun project(state: AppViewState): JsonObject = buildJsonObject {
        put("configuration", state.configuration.wireName); put("shell", state.shell.wireName); put("generation", state.generation)
        putJsonArray("definition_ids") { state.definitionIds.forEach { add(it) } }
        putJsonArray("session_ids") { state.sessions.forEach { add(it.sessionId) } }
        putNullable("selected_session_id", state.selectedSessionId)
        putJsonArray("timeline") { state.timeline.forEach { item -> addJsonObject {
            put("turn_id", item.turnId); put("state", item.state); put("latest_position", item.latestPosition)
            item.suspensionId?.let { put("suspension_id", it); put("session_version", item.sessionVersion!!)
                put("response_schema_digest", item.responseSchemaDigest!!) }
        } } }
        put("cursor", state.cursor)
        putJsonArray("drafts") { state.drafts.forEach { addJsonObject { put("session_id", it.sessionId); put("text", it.text) } } }
        put("execution", state.execution.wireName)
        putJsonArray("pending") { state.pending.forEach { item -> addJsonObject {
            put("kind", item.kind.wireName); put("command_id", item.commandId); put("request_digest", item.requestDigest)
            putNullable("session_id", item.sessionId); putNullable("turn_id", item.turnId); put("status", item.status.wireName)
        } } }
        putJsonArray("activities") { state.activities.forEach { item -> addJsonObject {
            put("activity_id", item.activityId); put("kind", item.kind); put("state", item.state)
            item.turnId?.let { put("turn_id", it) }; put("position", item.position); put("neutral", item.neutral)
        } } }
        state.notice?.let { putJsonObject("notice") { put("kind", it.kind.wireName); put("code", it.code) } }
            ?: put("notice", JsonNull)
    }

    private fun decodeTimeline(raw: JsonObject): TimelineItem = TimelineItem(raw.text("turn_id"), raw.text("state"),
        raw.long("latest_position"), raw.optionalText("suspension_id"), raw.optionalLong("session_version"),
        raw.optionalText("response_schema_digest"))
    private fun decodeActivity(raw: JsonObject): ActivityItem = ActivityItem(raw.text("activity_id"), raw.text("kind"),
        raw.text("state"), raw.optionalText("turn_id"), raw.long("position"), raw["neutral"]?.jsonPrimitive?.booleanOrNull ?: false)
    private fun decodePending(raw: JsonObject): PendingCommand = PendingCommand(commandKind(raw.text("kind")),
        raw.text("command_id"), raw.text("request_digest"), 0, raw.nullableText("session_id"), raw.nullableText("turn_id"),
        PendingStatus.entries.first { it.wireName == raw.text("status") })
    private fun decodeError(raw: JsonObject): AppError = AppError(AppErrorKind.entries.first { it.wireName == raw.text("kind") }, raw.text("code"))

    private fun validateFixture(): Unit {
        assertEquals(1, fixture.long("schema_version")); assertEquals("client-product-experience-v1", fixture.text("contract"))
        assertEquals(setOf("schema_version", "contract", "limits") + families, fixture.keys)
        families.forEach { family ->
            val names = fixture.array(family).map { it.jsonObject.text("name") }
            assertTrue(names.isNotEmpty(), family); assertEquals(names.size, names.toSet().size, family)
        }
    }

    private fun controllerLimits(): ControllerLimits = ControllerLimits(fixture.obj("limits").long("max_draft_bytes").toInt(),
        fixture.obj("limits").long("max_activities").toInt())
    private fun preferenceLimits(): PreferenceLimits = fixture.obj("limits").let {
        PreferenceLimits(it.long("max_preference_bytes").toInt(), it.long("max_drafts").toInt(),
            it.long("max_id_bytes").toInt(), it.long("max_draft_bytes").toInt())
    }

    private class MemoryPort(private var preferences: ByteArray?) : PreferenceBytesPort {
        private var pending: ByteArray? = null
        override suspend fun readPreferences(): ByteArray? = preferences
        override suspend fun writePreferences(value: ByteArray): Unit { preferences = value }
        override suspend fun readPendingCommand(): ByteArray? = pending
        override suspend fun writePendingCommand(value: ByteArray?): Unit { pending = value }
    }
}

private fun JsonObject.obj(key: String): JsonObject = getValue(key).jsonObject
private fun JsonObject.array(key: String): JsonArray = getValue(key).jsonArray
private fun JsonObject.text(key: String): String = getValue(key).jsonPrimitive.content
private fun JsonObject.long(key: String): Long = getValue(key).jsonPrimitive.long
private fun JsonObject.boolean(key: String): Boolean = getValue(key).jsonPrimitive.boolean
private fun JsonObject.optionalText(key: String): String? = get(key)?.takeUnless { it is JsonNull }?.jsonPrimitive?.content
private fun JsonObject.nullableText(key: String): String? = optionalText(key)
private fun JsonObject.optionalLong(key: String): Long? = get(key)?.takeUnless { it is JsonNull }?.jsonPrimitive?.long
private fun JsonObjectBuilder.putNullable(key: String, value: String?): Unit { if (value == null) put(key, JsonNull) else put(key, value) }
private fun configuration(value: String): AppConfiguration = AppConfiguration.entries.first { it.wireName == value }
private fun shell(value: String): ShellState = ShellState.entries.first { it.wireName == value }
private fun execution(value: String): ExecutionState = ExecutionState.entries.first { it.wireName == value }
private fun commandKind(value: String): CommandKind = CommandKind.entries.first { it.wireName == value }
private fun effectKind(value: String): EffectKind = EffectKind.entries.first { it.wireName == value }
