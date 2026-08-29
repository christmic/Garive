package com.garive.eng.kt.ledger

import kotlinx.serialization.json.Json

/** Classification returned after durable Runtime fact validation. */
public enum class RuntimeFactDisposition { APPLIED_V1, OPAQUE }

/** Validates admitted C6 Turn/Execution payload-v1 semantics and envelope ownership. */
public fun validateRuntimeFact(fact: FactDraft): LedgerResult<RuntimeFactDisposition> {
    val kind = fact.kind.value
    val executionFamily = kind.startsWith("execution.")
    if (!kind.startsWith("turn.") && !executionFamily) {
        return LedgerResult.Success(RuntimeFactDisposition.OPAQUE)
    }
    if (fact.schemaVersion != 1u) return LedgerResult.Success(RuntimeFactDisposition.OPAQUE)
    if (fact.turnId == null || (fact.executionId != null) != executionFamily ||
        fact.modelRequestId != null || fact.toolInvocationId != null
    ) {
        return LedgerResult.Failure(LedgerError.InvalidFact)
    }
    return try {
        validateTurnFact(kind, Json.parseToJsonElement(fact.payload.json).asObject())
        LedgerResult.Success(RuntimeFactDisposition.APPLIED_V1)
    } catch (_: IllegalArgumentException) {
        LedgerResult.Failure(LedgerError.InvalidFact)
    }
}
