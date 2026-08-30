package com.garive.eng.kt.ledger

import kotlinx.serialization.json.Json

/** Classification returned after durable Runtime fact validation. */
public enum class RuntimeFactDisposition { APPLIED_V1, APPLIED_V2, APPLIED_V3, OPAQUE }

/** Validates admitted C6 Turn/Execution payload-v1 semantics and envelope ownership. */
public fun validateRuntimeFact(fact: FactDraft): LedgerResult<RuntimeFactDisposition> {
    val kind = fact.kind.value
    val executionFamily = kind.startsWith("execution.")
    val modelFamily = kind.startsWith("model.")
    val effectFamily = kind.startsWith("effect.") || kind.startsWith("interaction.")
    val f0Family = kind.startsWith("safety.") || kind.startsWith("sandbox.")
    val skillFamily = kind.startsWith("skill.")
    val memoryFamily = kind.startsWith("memory.")
    val knowledgeFamily = kind.startsWith("knowledge.")
    val schedulerFamily = kind.startsWith("schedule.")
    val delegationFamily = kind.startsWith("delegation.")
    val goalFamily = kind.startsWith("goal.")
    val planFamily = kind.startsWith("plan.")
    val memorySessionScoped = kind in setOf(
        "memory.tombstoned", "memory.revision_classified", "memory.observation_recorded", "memory.lifecycle_transitioned",
        "memory.candidate_recorded", "memory.maintenance_decided", "memory.distillation_checkpointed",
        "memory.audit_recorded", "memory.promotion_requested", "memory.promotion_recorded",
        "memory.erasure_requested", "memory.erasure_recorded",
    )
    val rejection = kind == "tool.preparation_rejected"
    if (!kind.startsWith("turn.") && !executionFamily && !modelFamily && !effectFamily && !f0Family && !skillFamily && !memoryFamily && !knowledgeFamily && !schedulerFamily && !delegationFamily && !goalFamily && !planFamily && !rejection) {
        return LedgerResult.Success(RuntimeFactDisposition.OPAQUE)
    }
    val effectPreparedV2 = kind == "effect.prepared" && fact.schemaVersion == 2u
    val effectPreparedV3 = kind == "effect.prepared" && fact.schemaVersion == 3u
    if (fact.schemaVersion != 1u && !effectPreparedV2 && !effectPreparedV3) {
        return LedgerResult.Success(RuntimeFactDisposition.OPAQUE)
    }
    if ((fact.turnId != null) != !(memorySessionScoped || schedulerFamily || goalFamily || planFamily) ||
        (fact.executionId != null) != (executionFamily || modelFamily || effectFamily || f0Family || skillFamily || knowledgeFamily || delegationFamily || rejection || memoryFamily && !memorySessionScoped) ||
        (fact.modelRequestId != null) != (modelFamily || rejection) ||
        (fact.toolInvocationId != null) != (effectFamily || f0Family)
    ) {
        return LedgerResult.Failure(LedgerError.InvalidFact)
    }
    return try {
        val payload = Json.parseToJsonElement(fact.payload.json).asObject()
        when {
            effectPreparedV2 -> validateEffectPreparedV2(payload)
            effectPreparedV3 -> validateEffectPreparedV3(payload)
            f0Family -> validateF0Fact(kind, payload)
            goalFamily -> validateGoalFact(kind, payload)
            planFamily -> validatePlanFact(kind, payload)
            delegationFamily -> validateDelegationFact(kind, payload)
            schedulerFamily -> validateSchedulerFact(kind, payload)
            knowledgeFamily -> validateKnowledgeFact(kind, payload)
            memoryFamily -> validateMemoryFact(kind, payload)
            skillFamily -> validateSkillFact(kind, payload)
            effectFamily || rejection -> validateEffectFact(kind, payload)
            modelFamily -> validateModelFact(kind, payload)
            else -> validateTurnFact(kind, payload)
        }
        LedgerResult.Success(
            when {
                effectPreparedV2 -> RuntimeFactDisposition.APPLIED_V2
                effectPreparedV3 -> RuntimeFactDisposition.APPLIED_V3
                else -> RuntimeFactDisposition.APPLIED_V1
            },
        )
    } catch (_: IllegalArgumentException) {
        LedgerResult.Failure(LedgerError.InvalidFact)
    }
}
