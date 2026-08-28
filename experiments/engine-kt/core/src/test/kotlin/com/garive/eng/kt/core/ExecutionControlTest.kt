package com.garive.eng.kt.core

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotEquals

class ExecutionControlTest {
    private fun control(completed: UInt = 0u, maximum: UInt = 2u) =
        ExecutionControl.create(
            TurnId.of("turn-1"),
            ExecutionId.of("execution-1"),
            completed,
            ExecutionLimits(maximum),
        )

    @Test
    fun `limit closes without overcounting`() {
        val control = control(completed = 1u)
        assertEquals(BeginIteration.Started(2u), control.beginIteration())
        assertEquals(BeginIteration.IterationLimitReached, control.beginIteration())
        assertEquals(2u, control.completedIterations)
        assertEquals(ExecutionStatus.Closed(ExecutionOutcomeKind.STOPPED), control.status)
    }

    @Test
    fun `continuation uses new execution identity and durable cursor`() {
        val first = control(maximum = 3u)
        first.beginIteration()
        first.close(ExecutionOutcomeKind.SUSPENDED)
        val continued = ExecutionControl.create(
            first.turnId,
            ExecutionId.of("execution-2"),
            first.completedIterations,
            first.limits,
        )
        assertEquals(first.turnId, continued.turnId)
        assertNotEquals(first.executionId, continued.executionId)
        assertEquals(1u, continued.completedIterations)
        assertEquals(ExecutionStatus.Active, continued.status)
    }

    @Test
    fun `closed execution is immutable`() {
        val control = control()
        control.close(ExecutionOutcomeKind.COMPLETED)
        assertFailsWith<ControlException.AlreadyClosed> { control.beginIteration() }
        assertFailsWith<ControlException.AlreadyClosed> {
            control.close(ExecutionOutcomeKind.FAILED)
        }
        assertEquals(ExecutionStatus.Closed(ExecutionOutcomeKind.COMPLETED), control.status)
    }

    @Test
    fun `identities and limits reject invalid construction`() {
        assertFailsWith<IllegalArgumentException> { TurnId.of("") }
        assertFailsWith<IllegalArgumentException> { ExecutionId.of("") }
        assertFailsWith<IllegalArgumentException> { ExecutionLimits(0u) }
        assertFailsWith<ControlException.CursorBeyondLimit> {
            ExecutionControl.create(
                TurnId.of("turn-1"),
                ExecutionId.of("execution-1"),
                completedIterations = 3u,
                limits = ExecutionLimits(2u),
            )
        }
    }
}
