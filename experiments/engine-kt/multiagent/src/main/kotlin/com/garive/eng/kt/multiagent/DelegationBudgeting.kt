package com.garive.eng.kt.multiagent

/** Known or conservatively unknown child token usage. */
public sealed interface TokenUsageEvidence {
    public data class Known(public val value: ULong) : TokenUsageEvidence
    public data object Unknown : TokenUsageEvidence
}

/** Child token evidence used for conservative settlement. */
public data class DelegationUsage(public val inputTokens: TokenUsageEvidence, public val outputTokens: TokenUsageEvidence)

/** Exact finite child lifecycle consumption. */
public data class DelegationConsumption(
    public val childTurns: ULong, public val childExecutions: ULong,
    public val completedIterations: ULong, public val elapsedMs: ULong,
) {
    /** V1 terminal results account for one Turn and at least one Execution. */
    public fun validate(): DelegationContractResult<Unit> =
        if (childTurns == 1uL && childExecutions != 0uL) success(Unit)
        else failure(DelegationErrorCode.INVALID_DELEGATION)
}

/** Consumable aggregate dimensions charged or released. */
public data class BudgetAmounts(
    public val childTurns: ULong, public val childExecutions: ULong, public val iterations: ULong,
    public val inputTokens: ULong, public val outputTokens: ULong, public val elapsedMs: ULong,
)

/** Conservative terminal charge and safely releasable reservation. */
public data class DelegationBudgetSettlement(public val charged: BudgetAmounts, public val released: BudgetAmounts)

/** Computes terminal charge, conservatively charging a full token reservation when usage is unknown. */
public fun settleDelegationBudget(
    reservation: DelegationBudget,
    consumption: DelegationConsumption,
    usage: DelegationUsage,
): DelegationContractResult<DelegationBudgetSettlement> {
    if (reservation.validate() is DelegationContractResult.Failure || consumption.validate() is DelegationContractResult.Failure) {
        return failure(DelegationErrorCode.INVALID_DELEGATION)
    }
    val input = chargedTokens(usage.inputTokens, reservation.maxInputTokens) ?: return failure(DelegationErrorCode.BUDGET_EXHAUSTED)
    val output = chargedTokens(usage.outputTokens, reservation.maxOutputTokens) ?: return failure(DelegationErrorCode.BUDGET_EXHAUSTED)
    val charged = BudgetAmounts(consumption.childTurns, consumption.childExecutions, consumption.completedIterations, input, output, consumption.elapsedMs)
    val reserved = BudgetAmounts(
        reservation.maxChildTurns, reservation.maxChildExecutions, reservation.maxIterations,
        reservation.maxInputTokens, reservation.maxOutputTokens, reservation.deadlineBudgetMs,
    )
    if (charged.values().zip(reserved.values()).any { (used, limit) -> used > limit }) return failure(DelegationErrorCode.BUDGET_EXHAUSTED)
    return success(
        DelegationBudgetSettlement(
            charged,
            BudgetAmounts(
                reserved.childTurns - charged.childTurns, reserved.childExecutions - charged.childExecutions,
                reserved.iterations - charged.iterations, reserved.inputTokens - charged.inputTokens,
                reserved.outputTokens - charged.outputTokens, reserved.elapsedMs - charged.elapsedMs,
            ),
        ),
    )
}

/** Parent aggregate remainder and policy caps available for one reservation. */
public data class DelegationAllowance(
    public val remainingChildTurns: ULong, public val remainingChildExecutions: ULong,
    public val remainingIterations: ULong, public val remainingInputTokens: ULong,
    public val remainingOutputTokens: ULong, public val remainingElapsedMs: ULong,
    public val maxDepth: ULong, public val maxObjectiveBytes: ULong,
    public val maxInputEvidence: ULong, public val maxResultSchemaBytes: ULong,
    public val maxResultBytes: ULong, public val maxResultEvidence: ULong,
)

/** Exact authority grant committed before child allocation/start. */
public data class DelegationGrant(
    public val grantId: String, public val intentDigest: String,
    public val reservedBudget: DelegationBudget, public val authorityRevision: String,
)

/** Grant plus aggregate allowance after maximum-budget reservation. */
public data class DelegationAuthorization(public val grant: DelegationGrant, public val remaining: DelegationAllowance)

/** Checks authority bounds and reserves the full requested maximum. */
public fun authorizeDelegation(
    intent: DelegationIntent, grantId: String, authorityRevision: String,
    currentDepth: ULong, activeParentDelegations: ULong, allowance: DelegationAllowance,
): DelegationContractResult<DelegationAuthorization> {
    if (!validId(grantId) || !validId(authorityRevision)) return failure(DelegationErrorCode.INVALID_DELEGATION)
    if (currentDepth >= intent.budget.maxDepth || currentDepth >= allowance.maxDepth) return failure(DelegationErrorCode.DEPTH_EXCEEDED)
    if (activeParentDelegations != 0uL) return failure(DelegationErrorCode.CONCURRENCY_EXCEEDED)
    val budget = intent.budget
    if (listOf(
            budget.maxObjectiveBytes to allowance.maxObjectiveBytes,
            budget.maxInputEvidence to allowance.maxInputEvidence,
            budget.maxResultSchemaBytes to allowance.maxResultSchemaBytes,
            budget.maxResultBytes to allowance.maxResultBytes,
            budget.maxResultEvidence to allowance.maxResultEvidence,
        ).any { (requested, cap) -> requested > cap }
    ) return failure(DelegationErrorCode.BUDGET_EXHAUSTED)
    val requested = listOf(
        budget.maxChildTurns, budget.maxChildExecutions, budget.maxIterations,
        budget.maxInputTokens, budget.maxOutputTokens, budget.deadlineBudgetMs,
    )
    val available = allowance.remainingValues()
    if (requested.zip(available).any { (need, have) -> need > have }) return failure(DelegationErrorCode.BUDGET_EXHAUSTED)
    val left = available.zip(requested).map { (have, need) -> have - need }
    val digest = when (val value = intent.intentDigest()) {
        is DelegationContractResult.Success -> value.value
        is DelegationContractResult.Failure -> return value
    }
    return success(
        DelegationAuthorization(
            DelegationGrant(grantId, digest, budget, authorityRevision), allowance.withRemaining(left),
        ),
    )
}

/** Releases only unused terminal reservation without exceeding the pre-reservation ceiling. */
public fun releaseDelegationBudget(
    remaining: DelegationAllowance,
    settlement: DelegationBudgetSettlement,
    ceiling: DelegationAllowance,
): DelegationContractResult<DelegationAllowance> {
    val release = settlement.released.values()
    val current = remaining.remainingValues()
    if (current.zip(release).any { (left, right) -> ULong.MAX_VALUE - left < right }) return failure(DelegationErrorCode.BUDGET_OVERFLOW)
    val output = current.zip(release).map { (left, right) -> left + right }
    if (output.zip(ceiling.remainingValues()).any { (value, limit) -> value > limit }) return failure(DelegationErrorCode.CORRUPT_DELEGATION_STATE)
    return success(remaining.withRemaining(output))
}

private fun chargedTokens(value: TokenUsageEvidence, reservation: ULong): ULong? = when (value) {
    is TokenUsageEvidence.Known -> value.value.takeIf { it <= reservation }
    TokenUsageEvidence.Unknown -> reservation
}
private fun BudgetAmounts.values(): List<ULong> = listOf(childTurns, childExecutions, iterations, inputTokens, outputTokens, elapsedMs)
private fun DelegationAllowance.remainingValues(): List<ULong> = listOf(remainingChildTurns, remainingChildExecutions, remainingIterations, remainingInputTokens, remainingOutputTokens, remainingElapsedMs)
private fun DelegationAllowance.withRemaining(value: List<ULong>): DelegationAllowance = copy(
    remainingChildTurns = value[0], remainingChildExecutions = value[1], remainingIterations = value[2],
    remainingInputTokens = value[3], remainingOutputTokens = value[4], remainingElapsedMs = value[5],
)
