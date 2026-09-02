package com.garive.eng.kt.multiagent

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs

class CollaborationTest {
    @Test
    fun `ten named peers share one equal roster`() {
        val members = (0 until MAX_NAMED_SESSION_AGENTS).map {
            assertSuccess(NamedAgent.create("agent-$it", "Peer $it"))
        }
        val roster = assertSuccess(SessionRoster.create(members))

        assertEquals(10, roster.members.size)
        assertIs<AssigneeSelector.Named>(assertSuccess(AssigneeSelector.named("agent-7", roster)))
        assertEquals(DeliveryPolicy.NOTIFY, DeliveryPolicy.NOTIFY)
    }

    @Test
    fun `roster and selector limits fail closed`() {
        val duplicate = assertSuccess(NamedAgent.create("agent-1", "Peer"))
        assertFailure(DelegationErrorCode.INVALID_DELEGATION, SessionRoster.create(listOf(duplicate, duplicate)))
        val eleven = (0..MAX_NAMED_SESSION_AGENTS).map {
            assertSuccess(NamedAgent.create("agent-$it", "Peer $it"))
        }
        assertFailure(DelegationErrorCode.INVALID_DELEGATION, SessionRoster.create(eleven))
        val roster = assertSuccess(SessionRoster.create(listOf(duplicate)))
        assertFailure(DelegationErrorCode.INVALID_DELEGATION, AssigneeSelector.named("missing", roster))
        assertFailure(DelegationErrorCode.INVALID_DELEGATION, AssigneeSelector.forkSelf("agent-1", 0uL, null))
    }

    private fun <T> assertSuccess(result: DelegationContractResult<T>): T =
        assertIs<DelegationContractResult.Success<T>>(result).value

    private fun assertFailure(code: DelegationErrorCode, result: DelegationContractResult<*>): Unit =
        assertEquals(code, assertIs<DelegationContractResult.Failure>(result).code)
}
