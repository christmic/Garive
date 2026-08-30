package com.garive.eng.kt.ledger

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

private const val FIRST_DIGEST = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
private const val SECOND_DIGEST = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

class EffectBatchTransitionTest {
    private fun fact(
        id: String,
        kind: String,
        schema: UInt = 1u,
        tool: String? = null,
        payload: JsonElement = runtimePayload(kind, schema),
    ): FactDraft = FactDraft(
        FactId.of(id),
        if (kind != "session.opened") TurnId.of("turn") else null,
        if (kind != "session.opened" && !kind.startsWith("turn.")) ExecutionId.of("execution") else null,
        null,
        tool?.let(ToolInvocationId::of),
        FactKind.of(kind),
        schema,
        assertIs<CanonicalPayloadResult.Success>(CanonicalPayload.fromValue(payload)).payload,
        "2026-08-30T00:00:00Z",
    )

    private fun content(value: JsonElement): JsonObject {
        val canonical = assertIs<CanonicalPayloadResult.Success>(CanonicalPayload.fromValue(value)).payload
        return buildJsonObject { put("digest", canonical.sha256); put("inline_utf8", canonical.json) }
    }

    private fun prepared(digest: String, call: String): JsonObject = mutate(runtimePayload("effect.prepared", 2u)) {
        put("prepared_digest", JsonPrimitive(digest)); put("model_call_id", JsonPrimitive(call))
    }

    private fun authorized(digest: String, grant: String): JsonObject = mutate(runtimePayload("effect.authorized")) {
        put("prepared_digest", JsonPrimitive(digest)); put("grant_id", JsonPrimitive(grant))
    }

    private fun prefix(authorizeSecond: Boolean = true): MutableList<FactDraft> = mutableListOf(
        fact("open", "session.opened"),
        fact("turn", "turn.started"),
        fact("execution", "execution.started"),
        fact("prepared-first", "effect.prepared", 2u, "tool-first", prepared(FIRST_DIGEST, "call-first")),
        fact("authorized-first", "effect.authorized", tool = "tool-first", payload = authorized(FIRST_DIGEST, "grant")),
        fact("prepared-second", "effect.prepared", 2u, "tool-second", prepared(SECOND_DIGEST, "call-second")),
    ).also {
        if (authorizeSecond) it += fact(
            "authorized-second", "effect.authorized", tool = "tool-second",
            payload = authorized(SECOND_DIGEST, "grant-second"),
        )
    }

    private fun plan(indexes: List<Int> = listOf(0, 1), buffer: ULong = 1024u): FactDraft {
        val digests = buildJsonArray { add(JsonPrimitive(FIRST_DIGEST)); add(JsonPrimitive(SECOND_DIGEST)) }
        val steps = buildJsonArray {
            add(buildJsonObject {
                put("kind", "parallel_read_group")
                put("intent_indexes", JsonArray(indexes.map(::JsonPrimitive)))
            })
        }
        val payload = mutate(runtimePayload("execution.effect_batch_planned")) {
            put("ordered_prepared_digests", content(digests)); put("steps", content(steps))
            put("max_buffered_result_bytes", JsonPrimitive(buffer))
        }
        return fact("plan", "execution.effect_batch_planned", payload = payload)
    }

    private fun started(id: String, tool: String, digest: String, grant: String): FactDraft = fact(
        id, "effect.started", tool = tool,
        payload = mutate(runtimePayload("effect.started")) {
            put("prepared_digest", JsonPrimitive(digest)); put("grant_id", JsonPrimitive(grant))
        },
    )

    private fun commit(facts: List<FactDraft>): LedgerResult<CommitResult> =
        LedgerState().commit(SessionId.of("session"), 0u, facts)

    @Test
    fun `planned v2 effects start once in model order`() {
        val facts = prefix()
        facts += plan()
        facts += started("started-first", "tool-first", FIRST_DIGEST, "grant")
        facts += started("started-second", "tool-second", SECOND_DIGEST, "grant-second")
        assertIs<LedgerResult.Success<CommitResult>>(commit(facts))
    }

    @Test
    fun `authorization plan order coverage and buffer fail closed`() {
        val invalid = listOf(
            prefix(false).also { it += plan() },
            prefix().also { it += started("started", "tool-first", FIRST_DIGEST, "grant") },
            prefix().also { it += plan(); it += started("started-second", "tool-second", SECOND_DIGEST, "grant-second") },
            prefix().also { it += plan(listOf(0)) },
            prefix().also { it += plan(buffer = 1023u) },
        )
        invalid.forEach {
            assertEquals(LedgerError.InvalidTransition, assertIs<LedgerResult.Failure>(commit(it)).error)
        }
    }
}

private fun mutate(value: JsonElement, block: MutableMap<String, JsonElement>.() -> Unit): JsonObject =
    JsonObject((value as JsonObject).toMutableMap().apply(block))
