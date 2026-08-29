package com.garive.eng.kt.skill

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** Supported trusted activation mode in S0 v1. */
public enum class ActivationMode(public val wireName: String) {
    /** Activate one exact enabled Skill identity. */
    EXPLICIT("explicit"),
    /** Match only trusted Runtime-supplied tags. */
    TAGGED("tagged");

    public companion object {
        /** Parses a stable wire name and rejects future or semantic modes. */
        public fun fromWire(value: String): SkillContractResult<ActivationMode> = when (value) {
            EXPLICIT.wireName -> SkillContractResult.Success(EXPLICIT)
            TAGGED.wireName -> SkillContractResult.Success(TAGGED)
            else -> failure(SkillErrorCode.ACTIVATION_MODE_UNSUPPORTED)
        }
    }
}

/** Durable reason one exact Skill entered the model context. */
public enum class ActivationReason(public val wireName: String) {
    /** Selected by a trusted explicit request. */
    EXPLICIT("explicit"),
    /** Selected by trusted tag intersection. */
    TAG_MATCH("tag_match"),
}

/** Validated activation request scoped to one Kernel iteration. */
public class SkillActivationRequest private constructor(
    public val activationId: String,
    public val turnId: String,
    public val executionId: String,
    public val iteration: Long,
    public val mode: ActivationMode,
    public val requestedSkillId: String?,
    public val trustedTags: List<String>,
    public val throughPosition: Long,
    public val maxActiveSkills: Long,
    public val maxTotalInstructionBytes: Long,
) {
    /** Computes the exact S0 v1 digest without the outer activation identity. */
    public fun requestDigest(): SkillContractResult<String> = canonicalDigest(
        JsonObject(
            mapOf(
                "contract" to JsonPrimitive(REQUEST_CONTRACT),
                "version" to JsonPrimitive(CONTRACT_VERSION),
                "request" to JsonObject(
                    mapOf(
                        "turn_id" to JsonPrimitive(turnId),
                        "execution_id" to JsonPrimitive(executionId),
                        "iteration" to JsonPrimitive(iteration),
                        "mode" to JsonPrimitive(mode.wireName),
                        "requested_skill_id" to (requestedSkillId?.let(::JsonPrimitive) ?: JsonNull),
                        "trusted_tags" to JsonArray(trustedTags.map(::JsonPrimitive)),
                        "through_position" to JsonPrimitive(throughPosition),
                        "max_active_skills" to JsonPrimitive(maxActiveSkills),
                        "max_total_instruction_bytes" to JsonPrimitive(maxTotalInstructionBytes),
                    ),
                ),
            ),
        ),
    )

    public companion object {
        /** Validates a complete request and its mode-dependent shape. */
        @Suppress("LongParameterList")
        public fun create(
            activationId: String,
            turnId: String,
            executionId: String,
            iteration: Long,
            mode: ActivationMode,
            requestedSkillId: String?,
            trustedTags: List<String>,
            throughPosition: Long,
            maxActiveSkills: Long,
            maxTotalInstructionBytes: Long,
        ): SkillContractResult<SkillActivationRequest> {
            val explicitShape = requestedSkillId != null && trustedTags.isEmpty()
            val taggedShape = requestedSkillId == null
            if (!validText(activationId, MAX_ID_BYTES) || !validText(turnId, MAX_ID_BYTES) ||
                !validText(executionId, MAX_ID_BYTES) || iteration <= 0 || throughPosition < 0 ||
                maxActiveSkills <= 0 || maxActiveSkills > UInt.MAX_VALUE.toLong() ||
                maxTotalInstructionBytes <= 0 || requestedSkillId?.let { !validText(it, MAX_ID_BYTES) } == true ||
                trustedTags.any { !validText(it, MAX_TAG_BYTES) } || !orderedUnique(trustedTags) ||
                mode == ActivationMode.EXPLICIT && !explicitShape || mode == ActivationMode.TAGGED && !taggedShape
            ) {
                return failure(SkillErrorCode.INVALID_SKILL)
            }
            return SkillContractResult.Success(
                SkillActivationRequest(
                    activationId, turnId, executionId, iteration, mode, requestedSkillId,
                    trustedTags.toList(), throughPosition, maxActiveSkills, maxTotalInstructionBytes,
                ),
            )
        }
    }
}

/** Exact activated Skill content and narrowed tool surface. */
public data class ActivatedSkill(
    public val skillId: String,
    public val skillRevision: String,
    public val definitionDigest: String,
    public val instructions: String,
    public val instructionDigest: String,
    public val reason: ActivationReason,
    public val allowedToolReferences: List<ExactToolReference>,
)

/** Deterministic S0 activation result. */
public sealed interface SkillActivationResult {
    /** Exact Skills activated in canonical prefix order. */
    public data class Activated(
        public val orderedSkills: List<ActivatedSkill>,
        public val truncated: Boolean,
    ) : SkillActivationResult

    /** No trusted tags matched an enabled Skill. */
    public data object None : SkillActivationResult
}

/** Selects exact enabled Skills without I/O, authority, model calls, or mutation. */
public fun activateSkills(
    enabled: List<SkillDefinition>,
    availableCapabilities: Set<CapabilityReference>,
    availableTools: Set<ExactToolReference>,
    request: SkillActivationRequest,
): SkillContractResult<SkillActivationResult> {
    val definitions = linkedMapOf<Pair<String, String>, SkillDefinition>()
    for (definition in enabled) {
        val key = definition.skillId to definition.skillRevision
        val previous = definitions.put(key, definition)
        if (previous != null && digestValue(previous) != digestValue(definition)) {
            return failure(SkillErrorCode.ACTIVATION_CONFLICT)
        }
    }
    val candidates = when (request.mode) {
        ActivationMode.EXPLICIT -> {
            val matching = definitions.values.filter { it.skillId == request.requestedSkillId }
            when (matching.size) {
                0 -> return failure(SkillErrorCode.SKILL_NOT_ENABLED)
                1 -> listOf(Candidate(matching.single(), 0, ActivationReason.EXPLICIT))
                else -> return failure(SkillErrorCode.SKILL_REVISION_MISMATCH)
            }
        }
        ActivationMode.TAGGED -> definitions.values.mapNotNull { definition ->
            val count = when (val policy = definition.activation) {
                ActivationPolicy.ExplicitOnly -> 0
                is ActivationPolicy.Tagged -> policy.tags.count(request.trustedTags.toSet()::contains)
            }
            if (count == 0) null else Candidate(definition, count, ActivationReason.TAG_MATCH)
        }
    }.sortedWith(compareByDescending<Candidate> { it.matchedTags }
        .thenBy { it.definition.skillId }.thenBy { it.definition.skillRevision })

    val activated = mutableListOf<ActivatedSkill>()
    var totalBytes = 0L
    var truncated = false
    for (candidate in candidates) {
        val definition = candidate.definition
        if (definition.requiredCapabilities.any { it !in availableCapabilities }) {
            return failure(SkillErrorCode.REQUIRED_CAPABILITY_UNAVAILABLE)
        }
        if (definition.allowedToolReferences.any { it !in availableTools }) {
            return failure(SkillErrorCode.SKILL_NOT_ENABLED)
        }
        val nextBytes = totalBytes + definition.instructions.byteLength
        if (activated.size.toLong() == request.maxActiveSkills || nextBytes > request.maxTotalInstructionBytes) {
            if (request.mode == ActivationMode.EXPLICIT) return failure(SkillErrorCode.INSTRUCTION_LIMIT_EXCEEDED)
            truncated = true
            break
        }
        totalBytes = nextBytes
        activated += ActivatedSkill(
            definition.skillId, definition.skillRevision, digestValue(definition),
            definition.instructions.inlineUtf8, definition.instructions.digest, candidate.reason,
            definition.allowedToolReferences.toList(),
        )
    }
    return SkillContractResult.Success(
        if (activated.isEmpty() && !truncated) SkillActivationResult.None
        else SkillActivationResult.Activated(activated.toList(), truncated),
    )
}

private data class Candidate(
    val definition: SkillDefinition,
    val matchedTags: Int,
    val reason: ActivationReason,
)

private fun digestValue(definition: SkillDefinition): String =
    when (val result = definition.definitionDigest()) {
        is SkillContractResult.Success -> result.value
        is SkillContractResult.Failure -> error("validated definition is canonical")
    }
