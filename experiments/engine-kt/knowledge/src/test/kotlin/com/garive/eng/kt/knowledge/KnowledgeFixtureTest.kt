package com.garive.eng.kt.knowledge

import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class KnowledgeFixtureTest {
    private val root: JsonObject by lazy {
        val repo = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(repo.resolve("spec/fixtures/agent/knowledge-retrieval-v1.json").readText()).jsonObject
    }

    @Test
    fun `request digest filters and descriptor are exact`() {
        val request = request(FreshnessRequirement.CachedAllowed)
        assertEquals(root.obj("request").text("expected_request_digest"), request.requestDigest().success())
        assertEquals(root.obj("request").text("expected_filters_digest"), request.filtersBinding().digest)
        assertEquals(null, request.validateSource(source()))
        val duplicate = filters() + KnowledgeFilter(
            "year", KnowledgeFilterOperator.EQUAL, KnowledgeFilterValue.IntegerValue(2025),
        )
        assertEquals(
            KnowledgeErrorCode.INVALID_QUERY,
            assertIs<KnowledgeContractResult.Failure>(
                KnowledgeRequest.create(
                    "bad", "docs", "1", KnowledgeQueryMode.KEYWORD, ContentBinding.fromInline("x"),
                    duplicate, 1uL, 1u, 1uL, 1uL, FreshnessRequirement.CachedAllowed,
                ),
            ).code,
        )
    }

    @Test
    fun `ordering freshness and bounds follow shared vectors`() {
        val request = request(FreshnessRequirement.CachedAllowed)
        root.getValue("ordering_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val result = completeKnowledge(
                request, source(), evidence(KnowledgeFreshness.FRESH, null),
                case.getValue("connector_order_stable").jsonPrimitive.content.toBoolean(),
            ).success()
            assertEquals(case.strings("expected_ids"), result.evidence.map { it.evidenceId }, case.text("name"))
            assertEquals(case.getValue("truncated").jsonPrimitive.content.toBoolean(), result.truncated)
        }
        val stale = completeKnowledge(
            request(FreshnessRequirement.Revalidate), source(),
            evidence(KnowledgeFreshness.STALE, null), false,
        )
        assertEquals(KnowledgeErrorCode.INVALID_QUERY, assertIs<KnowledgeContractResult.Failure>(stale).code)
        val exact = request(FreshnessRequirement.ExactSnapshot("c".repeat(64)))
        assertTrue(
            completeKnowledge(exact, source(), evidence(KnowledgeFreshness.CACHED, "c".repeat(64)), false)
                is KnowledgeContractResult.Success,
        )
        val wrong = completeKnowledge(
            exact, source(), evidence(KnowledgeFreshness.CACHED, "d".repeat(64)), false,
        )
        assertEquals(KnowledgeErrorCode.INVALID_QUERY, assertIs<KnowledgeContractResult.Failure>(wrong).code)
    }

    private fun source(): KnowledgeSourceDescriptor {
        val value = root.obj("source")
        return KnowledgeSourceDescriptor.create(
            value.text("source_id"), value.text("source_revision"), KnowledgeSourceKind.DOCUMENTATION,
            value.text("content_domain"), KnowledgeTrustClass.CURATED,
            listOf(KnowledgeQueryMode.KEYWORD, KnowledgeQueryMode.SEMANTIC),
            value.text("freshness_policy_digest"), CitationScheme.URI_FRAGMENT,
            value.text("capability_metadata_digest"),
        ).success()
    }

    private fun request(freshness: FreshnessRequirement): KnowledgeRequest {
        val value = root.obj("request")
        return KnowledgeRequest.create(
            value.text("request_id"), value.text("source_id"), value.text("source_revision"),
            KnowledgeQueryMode.KEYWORD, content(value.obj("query")), filters(), value.ulong("through_position"),
            value.ulong("max_chunks").toUInt(), value.ulong("max_total_bytes"),
            value.ulong("deadline_budget_ms"), freshness,
        ).success()
    }

    private fun filters(): List<KnowledgeFilter> = root.obj("request").getValue("filters").jsonArray.map { element ->
        val value = element.jsonObject
        val operator = when (value.text("operator")) {
            "equal" -> KnowledgeFilterOperator.EQUAL
            "greater_than_or_equal" -> KnowledgeFilterOperator.GREATER_THAN_OR_EQUAL
            else -> error("unknown operator")
        }
        val primitive = value.getValue("value").jsonPrimitive
        val item = primitive.content.toLongOrNull()?.let(KnowledgeFilterValue::IntegerValue)
            ?: KnowledgeFilterValue.StringValue(primitive.content)
        KnowledgeFilter(value.text("field"), operator, item)
    }

    private fun evidence(freshness: KnowledgeFreshness, snapshot: String?): List<KnowledgeEvidence> =
        root.getValue("evidence").jsonArray.map { element ->
            val value = element.jsonObject
            val content = content(value.obj("content"))
            KnowledgeEvidence(
                value.text("evidence_id"), "docs", "1", snapshot, content,
                value.ulong("content_byte_length"),
                Citation.create(
                    CitationScheme.URI_FRAGMENT, value.text("locator"), null,
                    "https://example.test/${value.text("evidence_id")}", content.digest,
                ).success(),
                "2026-08-29T00:00:00Z", freshness, KnowledgeTrustClass.CURATED,
                value.ulong("rank_basis_points").toInt(),
            )
        }

    private fun content(value: JsonObject): ContentBinding =
        ContentBinding.inline(value.text("digest"), value.text("inline_utf8")).success()
}

private fun JsonObject.obj(key: String): JsonObject = getValue(key).jsonObject
private fun JsonObject.text(key: String): String = getValue(key).jsonPrimitive.content
private fun JsonObject.ulong(key: String): ULong = text(key).toULong()
private fun JsonObject.strings(key: String): List<String> = getValue(key).jsonArray.map { it.jsonPrimitive.content }
private fun <T> KnowledgeContractResult<T>.success(): T = assertIs<KnowledgeContractResult.Success<T>>(this).value
