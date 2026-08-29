package com.garive.eng.kt.skill

/** Stable S0 definition or activation failure classification. */
public enum class SkillErrorCode(public val wireName: String) {
    /** A definition, identifier, tag, reference, or bound is invalid. */
    INVALID_SKILL("invalid_skill"),
    /** The requested Skill is absent from the frozen snapshot. */
    SKILL_NOT_ENABLED("skill_not_enabled"),
    /** One Skill identity resolves to a different frozen revision. */
    SKILL_REVISION_MISMATCH("skill_revision_mismatch"),
    /** Exact instruction bytes do not match their content digest. */
    INSTRUCTION_DIGEST_MISMATCH("instruction_digest_mismatch"),
    /** The requested activation mode is outside S0 v1. */
    ACTIVATION_MODE_UNSUPPORTED("activation_mode_unsupported"),
    /** A required capability is absent from the frozen snapshot. */
    REQUIRED_CAPABILITY_UNAVAILABLE("required_capability_unavailable"),
    /** Required instructions exceed an activation bound. */
    INSTRUCTION_LIMIT_EXCEEDED("instruction_limit_exceeded"),
    /** One identity binds conflicting request or definition semantics. */
    ACTIVATION_CONFLICT("activation_conflict"),
    /** Runtime could not commit the activation fact. */
    DURABILITY_FAILURE("durability_failure"),
    /** Previously committed activation state is invalid. */
    CORRUPT_SKILL_STATE("corrupt_skill_state"),
}

/** Typed S0 failure. */
public data class SkillError(public val code: SkillErrorCode)

/** Result of validating or reducing an S0 contract value. */
public sealed interface SkillContractResult<out T> {
    /** Successful immutable value. */
    public data class Success<T>(public val value: T) : SkillContractResult<T>

    /** Stable contract failure. */
    public data class Failure(public val error: SkillError) : SkillContractResult<Nothing>
}

/** Exact UTF-8 instructions and their lowercase SHA-256 binding. */
public class ContentBinding private constructor(
    public val digest: String,
    public val inlineUtf8: String,
) {
    /** Exact UTF-8 byte length. */
    public val byteLength: Long = inlineUtf8.encodeToByteArray().size.toLong()

    public companion object {
        /** Validates exact inline content against its supplied digest. */
        public fun create(digest: String, inlineUtf8: String): SkillContractResult<ContentBinding> {
            if (!validDigest(digest) || sha256(inlineUtf8.encodeToByteArray()) != digest) {
                return failure(SkillErrorCode.INSTRUCTION_DIGEST_MISMATCH)
            }
            return SkillContractResult.Success(ContentBinding(digest, inlineUtf8))
        }

        /** Computes the exact digest for trusted inline content. */
        public fun fromInline(inlineUtf8: String): ContentBinding =
            ContentBinding(sha256(inlineUtf8.encodeToByteArray()), inlineUtf8)
    }
}

/** Exact capability reference already admitted by D0. */
@ConsistentCopyVisibility
public data class CapabilityReference private constructor(
    public val kind: String,
    public val name: String,
    public val exactRevision: String,
    public val contractVersion: String,
) : Comparable<CapabilityReference> {
    public override fun compareTo(other: CapabilityReference): Int =
        compareValuesBy(this, other, CapabilityReference::kind, CapabilityReference::name,
            CapabilityReference::exactRevision, CapabilityReference::contractVersion)

    public companion object {
        /** Validates a portable exact capability reference. */
        public fun create(
            kind: String,
            name: String,
            exactRevision: String,
            contractVersion: String,
        ): SkillContractResult<CapabilityReference> {
            if (listOf(kind, name, exactRevision, contractVersion).any { !validText(it, MAX_REFERENCE_BYTES) }) {
                return failure(SkillErrorCode.INVALID_SKILL)
            }
            return SkillContractResult.Success(CapabilityReference(kind, name, exactRevision, contractVersion))
        }
    }
}

/** Exact tool reference that may only narrow the D0 tool catalog. */
@ConsistentCopyVisibility
public data class ExactToolReference private constructor(
    public val name: String,
    public val exactRevision: String,
) : Comparable<ExactToolReference> {
    public override fun compareTo(other: ExactToolReference): Int =
        compareValuesBy(this, other, ExactToolReference::name, ExactToolReference::exactRevision)

    public companion object {
        /** Validates an exact tool name and revision. */
        public fun create(name: String, exactRevision: String): SkillContractResult<ExactToolReference> {
            if (!validText(name, MAX_REFERENCE_BYTES) || !validText(exactRevision, MAX_REFERENCE_BYTES)) {
                return failure(SkillErrorCode.INVALID_SKILL)
            }
            return SkillContractResult.Success(ExactToolReference(name, exactRevision))
        }
    }
}

/** Deterministic S0 v1 activation policy. */
public sealed interface ActivationPolicy {
    /** Only a trusted explicit request may activate the Skill. */
    public data object ExplicitOnly : ActivationPolicy

    /** Trusted Runtime tags may activate the Skill. */
    @ConsistentCopyVisibility
    public data class Tagged private constructor(public val tags: List<String>) : ActivationPolicy {
        public companion object {
            /** Validates a non-empty ordered unique tag list. */
            public fun create(tags: List<String>): SkillContractResult<Tagged> {
                if (tags.isEmpty() || tags.any { !validText(it, MAX_TAG_BYTES) } || !orderedUnique(tags)) {
                    return failure(SkillErrorCode.INVALID_SKILL)
                }
                return SkillContractResult.Success(Tagged(tags.toList()))
            }
        }
    }
}

/** Immutable instruction Skill admitted to one effective snapshot. */
public class SkillDefinition private constructor(
    public val skillId: String,
    public val skillRevision: String,
    public val name: String,
    public val description: String,
    public val instructions: ContentBinding,
    public val activation: ActivationPolicy,
    public val requiredCapabilities: List<CapabilityReference>,
    public val allowedToolReferences: List<ExactToolReference>,
    public val maxInstructionBytes: Long,
    public val contractVersion: String,
) {
    /** Computes the RFC 8785 digest over the complete versioned definition. */
    public fun definitionDigest(): SkillContractResult<String> = definitionDigest(this)

    public companion object {
        /** Validates every S0 definition field and exact ordered reference list. */
        @Suppress("LongParameterList")
        public fun create(
            skillId: String,
            skillRevision: String,
            name: String,
            description: String,
            instructions: ContentBinding,
            activation: ActivationPolicy,
            requiredCapabilities: List<CapabilityReference>,
            allowedToolReferences: List<ExactToolReference>,
            maxInstructionBytes: Long,
            contractVersion: String,
        ): SkillContractResult<SkillDefinition> {
            if (!validText(skillId, MAX_ID_BYTES) || !validText(skillRevision, MAX_ID_BYTES) ||
                !validText(name, MAX_NAME_BYTES) || !validText(description, MAX_DESCRIPTION_BYTES) ||
                !validText(contractVersion, MAX_REFERENCE_BYTES) || maxInstructionBytes <= 0 ||
                instructions.byteLength > maxInstructionBytes || !orderedUnique(requiredCapabilities) ||
                !orderedUnique(allowedToolReferences)
            ) {
                return failure(SkillErrorCode.INVALID_SKILL)
            }
            return SkillContractResult.Success(
                SkillDefinition(
                    skillId, skillRevision, name, description, instructions, activation,
                    requiredCapabilities.toList(), allowedToolReferences.toList(),
                    maxInstructionBytes, contractVersion,
                ),
            )
        }
    }
}

internal const val DEFINITION_CONTRACT: String = "garive.skill-definition"
internal const val REQUEST_CONTRACT: String = "garive.skill-activation"
internal const val CONTRACT_VERSION: Int = 1
internal const val MAX_ID_BYTES: Int = 128
internal const val MAX_NAME_BYTES: Int = 256
internal const val MAX_DESCRIPTION_BYTES: Int = 4_096
internal const val MAX_TAG_BYTES: Int = 128
internal const val MAX_REFERENCE_BYTES: Int = 256

internal fun failure(code: SkillErrorCode): SkillContractResult.Failure =
    SkillContractResult.Failure(SkillError(code))

internal fun validText(value: String, maxBytes: Int): Boolean =
    value.isNotEmpty() && value.encodeToByteArray().size <= maxBytes && value.trim() == value

internal fun validDigest(value: String): Boolean =
    value.length == 64 && value.all { it in '0'..'9' || it in 'a'..'f' }

internal fun <T : Comparable<T>> orderedUnique(values: List<T>): Boolean =
    values.zipWithNext().all { (left, right) -> left < right }
