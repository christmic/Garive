package com.garive.eng.kt.ledger

import kotlinx.serialization.json.Json

/** Classification returned after durable Runtime fact validation. */
public enum class RuntimeFactDisposition { APPLIED_V1, OPAQUE }

/** Validates admitted C6 Turn/Execution payload-v1 semantics and envelope ownership. */
public fun validateRuntimeFact(fact: FactDraft): LedgerResult<RuntimeFactDisposition> {
    val kind = fact.kind.value
    val executionFamily = kind.startsWith("execution.")
    val modelFamily = kind.startsWith("model.")
    if (!kind.startsWith("turn.") && !executionFamily && !modelFamily) {
        return LedgerResult.Success(RuntimeFactDisposition.OPAQUE)
    }
    if (fact.schemaVersion != 1u) return LedgerResult.Success(RuntimeFactDisposition.OPAQUE)
    if (fact.turnId == null || (fact.executionId != null) != (executionFamily || modelFamily) ||
        (fact.modelRequestId != null) != modelFamily || fact.toolInvocationId != null
    ) {
        return LedgerResult.Failure(LedgerError.InvalidFact)
    }
    return try {
        val payload = Json.parseToJsonElement(fact.payload.json).asObject()
        if (modelFamily) validateModelFact(kind, payload) else validateTurnFact(kind, payload)
        LedgerResult.Success(RuntimeFactDisposition.APPLIED_V1)
    } catch (_: IllegalArgumentException) {
        LedgerResult.Failure(LedgerError.InvalidFact)
    }
}
