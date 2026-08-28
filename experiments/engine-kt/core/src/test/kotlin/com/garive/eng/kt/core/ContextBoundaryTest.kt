package com.garive.eng.kt.core

import com.garive.eng.kt.llm.MediaKind
import com.garive.eng.kt.llm.ModelInputContent
import com.garive.eng.kt.llm.ModelInputItem
import com.garive.eng.kt.llm.ModelRole
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs

class ContextBoundaryTest {
    private val base = ContextRequest(
        sessionId = "session-1",
        turnId = "turn-1",
        purpose = ContextPurpose.INFERENCE,
        afterPosition = 1uL,
        throughPosition = 4uL,
        maxItems = 4,
        maxUtf8Bytes = 64,
    )

    @Test
    fun `request boundaries fail closed`() {
        val requests = listOf(
            base.copy(sessionId = ""),
            base.copy(turnId = ""),
            base.copy(throughPosition = 0uL),
            base.copy(maxItems = 0),
            base.copy(maxUtf8Bytes = 0),
            base.copy(afterPosition = base.throughPosition),
            base.copy(afterPosition = ULong.MAX_VALUE),
        )
        requests.forEach { request ->
            val failure = assertIs<ContextDerivationResult.Failure>(deriveContext(request, emptyList()))
            assertEquals(ContextDerivationError.InvalidRequest, failure.error)
        }
    }

    @Test
    fun `candidate boundaries fail closed`() {
        val cases = listOf(
            candidate(sessionId = "other") to ContextDerivationError.SessionMismatch,
            candidate(position = 0uL) to ContextDerivationError.PositionBeyondSurface,
            candidate(position = 5uL) to ContextDerivationError.PositionBeyondSurface,
            candidate(items = emptyList()) to ContextDerivationError.EmptyRequiredContent,
            candidate(visibility = Visibility.Purposes(emptySet())) to ContextDerivationError.InvalidVisibility,
        )
        cases.forEach { (candidate, expected) ->
            val failure = assertIs<ContextDerivationResult.Failure>(deriveContext(base, listOf(candidate)))
            assertEquals(expected, failure.error)
        }
    }

    @Test
    fun `every model input payload field counts toward the budget`() {
        val items = listOf(
            ModelInputItem.Message(
                ModelRole.SYSTEM,
                listOf(
                    ModelInputContent.Text("a"),
                    ModelInputContent.MediaReference(MediaKind.Other("custom"), "ref", "image/png"),
                ),
            ),
            ModelInputItem.ToolObservation("call", "{}"),
            ModelInputItem.ReasoningReference("reason"),
        )
        val request = base.copy(afterPosition = null, throughPosition = 1uL, maxItems = 3, maxUtf8Bytes = 31)
        val success = assertIs<ContextDerivationResult.Success>(
            deriveContext(request, listOf(candidate(position = 1uL, items = items))),
        )
        assertEquals(3, success.surface.itemCount)
        assertEquals(31, success.surface.utf8Bytes)
    }

    private fun candidate(
        sessionId: String = "session-1",
        position: ULong = 2uL,
        visibility: Visibility = Visibility.Visible,
        items: List<ModelInputItem> = listOf(
            ModelInputItem.Message(ModelRole.SYSTEM, listOf(ModelInputContent.Text("instruction"))),
        ),
    ) = ContextCandidate(
        FactRef(sessionId, position),
        CandidateKind.INSTRUCTION,
        Retention.REQUIRED,
        visibility,
        items,
    )
}
