package com.garive.eng.kt.ledger

import kotlinx.serialization.json.Json

/** Classification returned after durable Runtime fact validation. */
public enum class RuntimeFactDisposition { APPLIED_V1, OPAQUE }

/** Validates admitted C6 Turn/Execution payload-v1 semantics and envelope ownership. */
public fun validateRuntimeFact(fact: FactDraft): LedgerResult<RuntimeFactDisposition> {
    val kind = fact.kind.value
    val executionFamily = kind.startsWith("execution.")
    val modelFamily = kind.startsWith("model.")
    val effectFamily = kind.startsWith("effect.") || kind.startsWith("interaction.")
    val skillFamily = kind.startsWith("skill.")
    val memoryFamily = kind.startsWith("memory.")
    val knowledgeFamily = kind.startsWith("knowledge.")
    val schedulerFamily = kind.startsWith("schedule.")
    val delegationFamily = kind.startsWith("delegation.")
    val memorySessionScoped = kind in setOf(
        "memory.tombstoned", "memory.observation_recorded", "memory.lifecycle_transitioned",
        "memory.candidate_recorded", "memory.maintenance_decided", "memory.distillation_checkpointed",
        "memory.audit_recorded", "memory.promotion_requested", "memory.promotion_recorded",
        "memory.erasure_requested", "memory.erasure_recorded",
    )
    val rejection = kind == "tool.preparation_rejected"
    if (!kind.startsWith("turn.") && !executionFamily && !modelFamily && !effectFamily && !skillFamily && !memoryFamily && !knowledgeFamily && !schedulerFamily && !delegationFamily && !rejection) {
        return LedgerResult.Success(RuntimeFactDisposition.OPAQUE)
    }
    if (fact.schemaVersion != 1u) return LedgerResult.Success(RuntimeFactDisposition.OPAQUE)
    if ((fact.turnId != null) != !(memorySessionScoped || schedulerFamily) ||
        (fact.executionId != null) != (executionFamily || modelFamily || effectFamily || skillFamily || knowledgeFamily || delegationFamily || rejection || memoryFamily && !memorySessionScoped) ||
        (fact.modelRequestId != null) != (modelFamily || rejection) ||
        (fact.toolInvocationId != null) != effectFamily
    ) {
        return LedgerResult.Failure(LedgerError.InvalidFact)
    }
    return try {
        val payload = Json.parseToJsonElement(fact.payload.json).asObject()
        when {
            delegationFamily -> validateDelegationFact(kind, payload)
            schedulerFamily -> validateSchedulerFact(kind, payload)
            knowledgeFamily -> validateKnowledgeFact(kind, payload)
            memoryFamily -> validateMemoryFact(kind, payload)
            skillFamily -> validateSkillFact(kind, payload)
            effectFamily || rejection -> validateEffectFact(kind, payload)
            modelFamily -> validateModelFact(kind, payload)
            else -> validateTurnFact(kind, payload)
        }
        LedgerResult.Success(RuntimeFactDisposition.APPLIED_V1)
    } catch (_: IllegalArgumentException) {
        LedgerResult.Failure(LedgerError.InvalidFact)
    }
}
