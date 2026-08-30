package com.garive.eng.kt.tools

import java.security.MessageDigest
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer

/** Stable pure C5b planner failure classification. */
public enum class EffectBatchErrorCode(public val wireName: String) {
    /** A call is not Prepared v2 or lacks a valid exact access set. */
    EFFECT_ACCESS_INVALID("effect_access_invalid"),
    /** An input, group, access, or buffer bound is exceeded. */
    EFFECT_BATCH_BOUND_EXCEEDED("effect_batch_bound_exceeded"),
}

/** Typed deterministic C5b planner failure. */
public data class EffectBatchError(public val code: EffectBatchErrorCode)

/** Typed success or failure returned by the pure C5b planner. */
public sealed interface EffectBatchResult<out T> {
    /** Successful immutable value. */
    public data class Success<T>(public val value: T) : EffectBatchResult<T>
    /** Stable planner failure. */
    public data class Failure(public val error: EffectBatchError) : EffectBatchResult<Nothing>
}

/** Explicit non-zero planner and group bounds. */
public class EffectBatchLimitsV1 private constructor(
    internal val maxIntents: Int,
    internal val maxAccessesPerIntent: Int,
    internal val maxTotalAccesses: Int,
    internal val maxParallelReads: Int,
    internal val maxBufferedResultBytes: Long,
) {
    public companion object {
        /** Validates and constructs the complete v1 limit snapshot. */
        @Suppress("LongParameterList")
        public fun create(
            maxIntents: Int,
            maxAccessesPerIntent: Int,
            maxTotalAccesses: Int,
            maxParallelReads: Int,
            maxBufferedResultBytes: Long,
        ): EffectBatchResult<EffectBatchLimitsV1> =
            if (listOf(maxIntents, maxAccessesPerIntent, maxTotalAccesses, maxParallelReads).any { it <= 0 } ||
                maxBufferedResultBytes <= 0
            ) {
                boundFailure()
            } else {
                EffectBatchResult.Success(
                    EffectBatchLimitsV1(
                        maxIntents,
                        maxAccessesPerIntent,
                        maxTotalAccesses,
                        maxParallelReads,
                        maxBufferedResultBytes,
                    ),
                )
            }
    }
}

/** One ordered deterministic execution step. */
public sealed interface EffectBatchStep {
    /** One call that Runtime must execute sequentially. */
    public data class SequentialStep(public val intentIndex: Int) : EffectBatchStep
    /** One contiguous bounded non-conflicting read-only group. */
    public data class ParallelReadGroup(public val intentIndexes: List<Int>) : EffectBatchStep
}

/** One Prepared Call plus an admitted interaction boundary decision. */
public data class EffectBatchIntent(
    public val prepared: PreparedToolCall,
    public val suspensionBoundary: Boolean,
)

/** Canonical conflict graph and ordered execution plan. */
public class EffectBatchPlanV1 internal constructor(
    orderedPreparedDigests: List<String>,
    conflictGraphBytes: List<Int>,
    public val conflictGraphDigest: String,
    steps: List<EffectBatchStep>,
    public val planDigest: String,
) {
    /** Ordered exact Prepared Call digests. */
    public val orderedPreparedDigests: List<String> = orderedPreparedDigests.toList()
    /** Upper-triangle graph bytes in ascending index-pair order. */
    public val conflictGraphBytes: List<Int> = conflictGraphBytes.toList()
    /** Ordered complete plan steps. */
    public val steps: List<EffectBatchStep> = steps.toList()
}

/** Plans Prepared v2 calls with no suspension boundaries. */
public fun planEffectBatch(
    prepared: List<PreparedToolCall>,
    limits: EffectBatchLimitsV1,
): EffectBatchResult<EffectBatchPlanV1> = planEffectBatchIntents(
    prepared.map { EffectBatchIntent(it, false) },
    limits,
)

/** Plans Prepared v2 calls with explicit admitted suspension boundaries. */
public fun planEffectBatchIntents(
    intents: List<EffectBatchIntent>,
    limits: EffectBatchLimitsV1,
): EffectBatchResult<EffectBatchPlanV1> {
    if (intents.isEmpty() || intents.size > limits.maxIntents) return boundFailure()
    var totalAccesses = 0
    intents.forEach { intent ->
        val accesses = intent.prepared.invocationAccesses ?: return accessPlanFailure()
        if (intent.prepared.contractVersion != 2 || accesses.values.isEmpty()) return accessPlanFailure()
        if (accesses.values.size > limits.maxAccessesPerIntent) return boundFailure()
        totalAccesses = addExact(totalAccesses, accesses.values.size) ?: return boundFailure()
    }
    if (totalAccesses > limits.maxTotalAccesses) return boundFailure()

    val graph = conflictGraph(intents)
    val steps = mutableListOf<EffectBatchStep>()
    var group = mutableListOf<Int>()
    var groupAccesses = 0
    var groupBytes = 0L
    intents.forEachIndexed { index, intent ->
        val call = intent.prepared
        val accesses = call.invocationAccesses ?: return accessPlanFailure()
        val resultBytes = call.maxResultBytes ?: return accessPlanFailure()
        val conflicts = group.any { member -> graphEdge(graph, intents.size, member, index) }
        val nextAccesses = addExact(groupAccesses, accesses.values.size) ?: return boundFailure()
        val nextBytes = addExact(groupBytes, resultBytes) ?: return boundFailure()
        val joins = call.replayClass == ReplayClass.READ_ONLY &&
            !intent.suspensionBoundary && !conflicts &&
            group.size < limits.maxParallelReads &&
            nextAccesses <= limits.maxTotalAccesses &&
            nextBytes <= limits.maxBufferedResultBytes
        if (joins) {
            group.add(index)
            groupAccesses = nextAccesses
            groupBytes = nextBytes
        } else {
            if (group.isNotEmpty()) steps.add(EffectBatchStep.ParallelReadGroup(group.toList()))
            group = mutableListOf()
            groupAccesses = 0
            groupBytes = 0
            if (call.replayClass == ReplayClass.READ_ONLY && !intent.suspensionBoundary) {
                if (resultBytes > limits.maxBufferedResultBytes) return boundFailure()
                group.add(index)
                groupAccesses = accesses.values.size
                groupBytes = resultBytes
            } else {
                steps.add(EffectBatchStep.SequentialStep(index))
            }
        }
    }
    if (group.isNotEmpty()) steps.add(EffectBatchStep.ParallelReadGroup(group.toList()))

    val digests = intents.map { it.prepared.inputDigest }
    val graphDigest = sha256(graph.map(Int::toByte).toByteArray())
    val preimage = JsonObject(
        mapOf(
            "schema_version" to JsonPrimitive(1),
            "prepared_contract_version" to JsonPrimitive(2),
            "ordered_prepared_digests" to JsonArray(digests.map(::JsonPrimitive)),
            "conflict_graph_digest" to JsonPrimitive(graphDigest),
            "steps" to JsonArray(steps.map(::stepJson)),
        ),
    )
    val canonical = runCatching { JsonCanonicalizer(preimage.toString()).encodedUTF8 }.getOrNull()
        ?: return accessPlanFailure()
    return EffectBatchResult.Success(
        EffectBatchPlanV1(digests, graph, graphDigest, steps, sha256(canonical)),
    )
}

private fun conflictGraph(intents: List<EffectBatchIntent>): List<Int> = buildList {
    intents.indices.forEach { left ->
        ((left + 1) until intents.size).forEach { right ->
            val conflict = intents[left].prepared.invocationAccesses!!.values.any { leftAccess ->
                intents[right].prepared.invocationAccesses!!.values.any { rightAccess ->
                    accessesConflict(leftAccess, rightAccess)
                }
            }
            add(if (conflict) 1 else 0)
        }
    }
}

private fun accessesConflict(left: ResourceAccess, right: ResourceAccess): Boolean =
    left.namespace == right.namespace &&
        (left.mode == AccessMode.EXCLUSIVE || right.mode == AccessMode.EXCLUSIVE ||
            left.resourceKey == right.resourceKey &&
            (left.mode == AccessMode.WRITE || right.mode == AccessMode.WRITE))

private fun graphEdge(graph: List<Int>, count: Int, left: Int, right: Int): Boolean {
    val offset = left * (2 * count - left - 1) / 2 + (right - left - 1)
    return graph[offset] == 1
}

private fun stepJson(step: EffectBatchStep): JsonElement = when (step) {
    is EffectBatchStep.SequentialStep -> JsonObject(
        mapOf("kind" to JsonPrimitive("sequential_step"), "intent_index" to JsonPrimitive(step.intentIndex)),
    )
    is EffectBatchStep.ParallelReadGroup -> JsonObject(
        mapOf(
            "kind" to JsonPrimitive("parallel_read_group"),
            "intent_indexes" to JsonArray(step.intentIndexes.map(::JsonPrimitive)),
        ),
    )
}

private fun addExact(left: Int, right: Int): Int? =
    runCatching { Math.addExact(left, right) }.getOrNull()

private fun addExact(left: Long, right: Long): Long? =
    runCatching { Math.addExact(left, right) }.getOrNull()

private fun sha256(value: ByteArray): String = MessageDigest.getInstance("SHA-256")
    .digest(value)
    .joinToString(separator = "") { byte -> "%02x".format(byte.toInt() and 0xff) }

private fun accessPlanFailure(): EffectBatchResult.Failure =
    EffectBatchResult.Failure(EffectBatchError(EffectBatchErrorCode.EFFECT_ACCESS_INVALID))

private fun boundFailure(): EffectBatchResult.Failure =
    EffectBatchResult.Failure(EffectBatchError(EffectBatchErrorCode.EFFECT_BATCH_BOUND_EXCEEDED))
