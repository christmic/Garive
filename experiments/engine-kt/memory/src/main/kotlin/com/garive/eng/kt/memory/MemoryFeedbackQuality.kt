package com.garive.eng.kt.memory

/** Attributable reality outcome for one applied recalled revision. */
public enum class RecallFeedbackOutcome { VERIFIED, FALSIFIED, NEUTRAL }

/** Content-free projection of one recall/application/outcome chain. */
public data class RecallFeedbackRow(
    public val exposureId: String,
    public val selectionId: String,
    public val recordId: String,
    public val revisionId: String,
    public val applied: Boolean,
    public val outcome: RecallFeedbackOutcome?,
)

/** Version bindings and rows for one exact feedback reduction. */
public data class RecallFeedbackQualityRequest(
    public val policyRevision: String,
    public val candidatePortRevision: String,
    public val attributionPolicyRevision: String,
    public val verifierRevision: String,
    public val corpusDigest: String,
    public val rows: List<RecallFeedbackRow>,
)

/** Exact integer evidence from attributable production or pinned chains. */
public data class RecallFeedbackQualitySummary(
    public val exposures: ULong,
    public val applications: ULong,
    public val censored: ULong,
    public val pending: ULong,
    public val verified: ULong,
    public val falsified: ULong,
    public val neutral: ULong,
    public val applicationRatio: RecallQualityRatio?,
    public val verifiedOutcomeRatio: RecallQualityRatio?,
)

/** Reduces one bounded, version-bound chain set without I/O or floating point. */
public fun evaluateRecallFeedbackQuality(
    request: RecallFeedbackQualityRequest,
): MemoryContractResult<RecallFeedbackQualitySummary> {
    val revisions = listOf(
        request.policyRevision, request.candidatePortRevision,
        request.attributionPolicyRevision, request.verifierRevision,
    )
    if (request.rows.size > MAX_FEEDBACK_ROWS || !validDigest(request.corpusDigest) ||
        revisions.any { it.isEmpty() || it.trim() != it } ||
        request.rows.zipWithNext().any { (left, right) -> left.exposureId >= right.exposureId } ||
        request.rows.any { row ->
            listOf(row.exposureId, row.selectionId, row.recordId, row.revisionId)
                .any { it.isEmpty() || it.trim() != it } || !row.applied && row.outcome != null
        }
    ) return failure(MemoryErrorCode.INVALID_MEMORY)
    val exposures = request.rows.size.toULong()
    val applications = request.rows.count { it.applied }.toULong()
    val verified = request.rows.count { it.outcome == RecallFeedbackOutcome.VERIFIED }.toULong()
    val falsified = request.rows.count { it.outcome == RecallFeedbackOutcome.FALSIFIED }.toULong()
    return MemoryContractResult.Success(
        RecallFeedbackQualitySummary(
            exposures, applications, exposures - applications,
            request.rows.count { it.applied && it.outcome == null }.toULong(),
            verified, falsified,
            request.rows.count { it.outcome == RecallFeedbackOutcome.NEUTRAL }.toULong(),
            ratio(applications, exposures), ratio(verified, verified + falsified),
        ),
    )
}

private fun ratio(numerator: ULong, denominator: ULong): RecallQualityRatio? =
    if (denominator == 0uL) null else RecallQualityRatio(numerator, denominator)

private const val MAX_FEEDBACK_ROWS: Int = 4096
