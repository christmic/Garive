package com.garive.eng.kt.host

/** Durable Execution position considered by experimental Runtime recovery. */
public enum class ExecutionRecoveryPosition { ACTIVE, SUSPENDED, TERMINAL }

/** Most advanced model lifecycle position in a lost Execution. */
public enum class ModelRecoveryPosition { NONE, PREPARED, STARTED, UNCERTAIN, TERMINAL }

/** Most advanced effect or interaction position in a lost Execution. */
public enum class EffectRecoveryPosition {
    NONE,
    PREPARED,
    STARTED,
    RECEIPT,
    UNCERTAIN,
    INTERACTION_REQUESTED,
    TERMINAL,
}

/** Minimal durable positions required for one recovery decision. */
public data class RuntimeRecoverySnapshot(
    public val execution: ExecutionRecoveryPosition,
    public val model: ModelRecoveryPosition,
    public val effect: EffectRecoveryPosition,
    public val recoveryOrdinal: ULong,
    public val maxRecoveries: ULong,
)

/** Unique fail-closed action selected by experimental recovery semantics. */
public enum class RuntimeRecoveryAction {
    ABANDON_AND_RESTART,
    CLASSIFY_MODEL_UNCERTAIN,
    CLASSIFY_EFFECT_UNCERTAIN,
    RECOVER_RECEIPT_TERMINAL,
    AWAIT_CONTINUATION,
    RETURN_COMMITTED_TERMINAL,
    FAIL_RECOVERY_BOUND,
    FAIL_CORRUPT_LEDGER,
}

/** Selects a C6 restart action without performing storage or external I/O. */
public fun selectRuntimeRecovery(snapshot: RuntimeRecoverySnapshot): RuntimeRecoveryAction {
    if (snapshot.maxRecoveries == 0uL) return RuntimeRecoveryAction.FAIL_CORRUPT_LEDGER
    return when {
        snapshot.execution == ExecutionRecoveryPosition.TERMINAL ->
            RuntimeRecoveryAction.RETURN_COMMITTED_TERMINAL
        snapshot.execution == ExecutionRecoveryPosition.SUSPENDED &&
            snapshot.model == ModelRecoveryPosition.UNCERTAIN ->
            RuntimeRecoveryAction.AWAIT_CONTINUATION
        snapshot.execution == ExecutionRecoveryPosition.SUSPENDED &&
            snapshot.effect in setOf(
                EffectRecoveryPosition.INTERACTION_REQUESTED,
                EffectRecoveryPosition.UNCERTAIN,
            ) ->
            RuntimeRecoveryAction.AWAIT_CONTINUATION
        snapshot.execution == ExecutionRecoveryPosition.SUSPENDED ->
            RuntimeRecoveryAction.FAIL_CORRUPT_LEDGER
        snapshot.recoveryOrdinal >= snapshot.maxRecoveries ->
            RuntimeRecoveryAction.FAIL_RECOVERY_BOUND
        snapshot.model == ModelRecoveryPosition.STARTED ->
            RuntimeRecoveryAction.CLASSIFY_MODEL_UNCERTAIN
        snapshot.effect == EffectRecoveryPosition.STARTED ->
            RuntimeRecoveryAction.CLASSIFY_EFFECT_UNCERTAIN
        snapshot.effect == EffectRecoveryPosition.RECEIPT ->
            RuntimeRecoveryAction.RECOVER_RECEIPT_TERMINAL
        snapshot.model in setOf(
            ModelRecoveryPosition.NONE,
            ModelRecoveryPosition.PREPARED,
            ModelRecoveryPosition.TERMINAL,
        ) && snapshot.effect in setOf(
            EffectRecoveryPosition.NONE,
            EffectRecoveryPosition.PREPARED,
            EffectRecoveryPosition.TERMINAL,
        ) -> RuntimeRecoveryAction.ABANDON_AND_RESTART
        else -> RuntimeRecoveryAction.FAIL_CORRUPT_LEDGER
    }
}
