package com.garive.eng.kt.core

import com.garive.eng.kt.llm.ModelInputContent
import com.garive.eng.kt.llm.ModelInputItem
import com.garive.eng.kt.llm.ModelRole
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ContextPropertiesTest {
    private fun candidate(position: ULong, text: String, retention: Retention) = ContextCandidate(
        FactRef("session", position),
        CandidateKind.USER_INPUT,
        retention,
        Visibility.Visible,
        listOf(ModelInputItem.Message(ModelRole.USER, listOf(ModelInputContent.Text(text)))),
    )

    private fun request(maxItems: Int, maxBytes: Int) = ContextRequest(
        "session",
        "turn",
        ContextPurpose.INFERENCE,
        null,
        4u,
        maxItems,
        maxBytes,
    )

    @Test
    fun `selection invariants hold across small budget space`() {
        val candidates = listOf(
            candidate(1u, "r", Retention.REQUIRED),
            candidate(2u, "aa", Retention.OPTIONAL),
            candidate(3u, "bbb", Retention.OPTIONAL),
            candidate(4u, "cccc", Retention.OPTIONAL),
        )
        for (maxItems in 1..4) for (maxBytes in 1..10) {
            val result = deriveContext(request(maxItems, maxBytes), candidates)
            val surface = (result as ContextDerivationResult.Success).surface
            assertEquals(result, deriveContext(request(maxItems, maxBytes), candidates))
            assertTrue(surface.itemCount <= maxItems)
            assertTrue(surface.utf8Bytes <= maxBytes)
            assertEquals(1uL, surface.retainedRefs.first().position)
            assertTrue(surface.retainedRefs.zipWithNext().all { (left, right) -> left.position < right.position })
            assertTrue(surface.retainedRefs.toSet().intersect(surface.droppedRefs.toSet()).isEmpty())
        }
    }

    @Test
    fun `text chunking preserves admission and byte cost`() {
        val combined = candidate(1u, "蟹蟹", Retention.REQUIRED)
        val split = combined.copy(
            items = listOf(
                ModelInputItem.Message(
                    ModelRole.USER,
                    listOf(ModelInputContent.Text("蟹"), ModelInputContent.Text("蟹")),
                ),
            ),
        )
        val first = (deriveContext(request(1, 6), listOf(combined)) as ContextDerivationResult.Success).surface
        val second = (deriveContext(request(1, 6), listOf(split)) as ContextDerivationResult.Success).surface
        assertEquals(first.retainedRefs, second.retainedRefs)
        assertEquals(first.itemCount, second.itemCount)
        assertEquals(first.utf8Bytes, second.utf8Bytes)
    }
}
