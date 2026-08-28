package com.garive.eng.kt.llm

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class ModelOutcomeTest {
    private fun usage(input: TokenCount, output: TokenCount) = ModelUsage(
        input,
        output,
        cacheReadTokens = TokenCount.Known(4u),
        cacheWriteTokens = TokenCount.Known(1u),
        source = UsageSource.PROVIDER_REPORTED,
    )

    @Test
    fun `usage distinguishes known unknown and overflow`() {
        assertEquals(UsageTotal.Known(12u), usage(TokenCount.Known(10u), TokenCount.Known(2u)).totalTokens())
        assertEquals(UsageTotal.Unknown, usage(TokenCount.Unknown, TokenCount.Known(2u)).totalTokens())
        assertEquals(UsageTotal.Overflow, usage(TokenCount.Known(ULong.MAX_VALUE), TokenCount.Known(1u)).totalTokens())
    }

    @Test
    fun `completed is the only success and interrupted is the only partial`() {
        val completed = InvokeOutcome.Completed(emptyList(), usage(TokenCount.Known(1u), TokenCount.Known(1u)), ModelStopReason.EndTurn)
        val interrupted = InvokeOutcome.Interrupted(InterruptionKind.OUTPUT_LIMIT, listOf(ModelItem.Text("prefix")), usage(TokenCount.Known(1u), TokenCount.Unknown))
        val rejected = InvokeOutcome.Rejected(RejectionKind.CONTENT_POLICY, "policy")
        assertTrue(completed.isSuccess)
        assertFalse(completed.isPartial)
        assertTrue(interrupted.isPartial)
        assertFalse(interrupted.isSuccess)
        assertFalse(rejected.isSuccess)
        assertFalse(rejected.isPartial)
        assertFalse(ModelStopReason.PauseTurn == ModelStopReason.Refusal)
    }
}
