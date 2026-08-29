package com.garive.eng.kt.memory

/** Exact logical Memory revision identity used by pinned evaluation. */
public data class RecallQualityIdentity(public val recordId: String, public val revisionId: String)

/** One pinned recall result with relevance and safety labels. */
public data class RecallQualityCase(
    public val caseId: String,
    public val expected: List<RecallQualityIdentity>,
    public val forbidden: List<RecallQualityIdentity>,
    public val selected: List<RecallQualityIdentity>,
    public val replay: List<RecallQualityIdentity>,
)

/** Exact unreduced ratio. */
public data class RecallQualityRatio(public val numerator: ULong, public val denominator: ULong)

/** Deterministic aggregate for one pinned semantic recall suite. */
public data class RecallQualitySummary(
    public val cases: ULong,
    public val recall: RecallQualityRatio?,
    public val precision: RecallQualityRatio?,
    public val forbiddenAdmissions: ULong,
    public val replayMismatches: ULong,
)

/** Reduces pinned recall evidence without model, storage, or network I/O. */
public fun evaluateRecallQuality(cases: List<RecallQualityCase>): MemoryContractResult<RecallQualitySummary> {
    if (cases.map { it.caseId }.toSet().size != cases.size || cases.any { !valid(it) }) {
        return failure(MemoryErrorCode.INVALID_MEMORY)
    }
    var relevant = 0uL
    var expected = 0uL
    var selected = 0uL
    var forbidden = 0uL
    var mismatches = 0uL
    for (case in cases) {
        expected = add(expected, case.expected.size) ?: return failure(MemoryErrorCode.INVALID_MEMORY)
        selected = add(selected, case.selected.size) ?: return failure(MemoryErrorCode.INVALID_MEMORY)
        relevant = add(relevant, case.selected.count { it in case.expected })
            ?: return failure(MemoryErrorCode.INVALID_MEMORY)
        forbidden = add(forbidden, case.selected.count { it in case.forbidden })
            ?: return failure(MemoryErrorCode.INVALID_MEMORY)
        if (case.selected != case.replay) mismatches = add(mismatches, 1)
            ?: return failure(MemoryErrorCode.INVALID_MEMORY)
    }
    return MemoryContractResult.Success(
        RecallQualitySummary(
            cases.size.toULong(), ratio(relevant, expected), ratio(relevant, selected), forbidden, mismatches,
        ),
    )
}

private fun valid(case: RecallQualityCase): Boolean = case.caseId.isNotEmpty() &&
    listOf(case.expected, case.forbidden, case.selected, case.replay).all { values ->
        values.toSet().size == values.size && values.all { it.recordId.isNotEmpty() && it.revisionId.isNotEmpty() }
    } && case.expected.none { it in case.forbidden }

private fun add(value: ULong, amount: Int): ULong? {
    val increment = amount.toULong()
    return if (ULong.MAX_VALUE - value < increment) null else value + increment
}

private fun ratio(numerator: ULong, denominator: ULong): RecallQualityRatio? =
    if (denominator == 0uL) null else RecallQualityRatio(numerator, denominator)
