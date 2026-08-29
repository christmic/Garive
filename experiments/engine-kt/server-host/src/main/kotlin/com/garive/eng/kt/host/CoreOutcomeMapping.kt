package com.garive.eng.kt.host

/** C6 terminal class accepted from one disposable Core execution. */
public enum class RuntimeCoreOutcomeKind { COMPLETED, SUSPENDED, STOPPED, FAILED }

/** Portable terminal fact names and resulting durable Turn classification. */
public data class RuntimeCoreOutcomeMapping(
    public val facts: List<String>,
    public val turnState: String,
)

/** Maps one Core outcome class to the atomic C6 terminal fact pair. */
public fun mapCoreOutcome(kind: RuntimeCoreOutcomeKind): RuntimeCoreOutcomeMapping = when (kind) {
    RuntimeCoreOutcomeKind.COMPLETED -> mapping("completed", "terminal")
    RuntimeCoreOutcomeKind.SUSPENDED -> mapping("suspended", "resumable")
    RuntimeCoreOutcomeKind.STOPPED -> mapping("stopped", "terminal")
    RuntimeCoreOutcomeKind.FAILED -> mapping("failed", "terminal")
}

private fun mapping(kind: String, state: String): RuntimeCoreOutcomeMapping =
    RuntimeCoreOutcomeMapping(listOf("execution.$kind", "turn.$kind"), state)
