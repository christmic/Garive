package com.garive.runtime.server.agent

@JvmInline
value class TurnId private constructor(val value: String) {
    companion object {
        fun of(value: String): TurnId {
            require(value.isNotEmpty()) { "turn identity cannot be empty" }
            return TurnId(value)
        }
    }
}

@JvmInline
value class ExecutionId private constructor(val value: String) {
    companion object {
        fun of(value: String): ExecutionId {
            require(value.isNotEmpty()) { "execution identity cannot be empty" }
            return ExecutionId(value)
        }
    }
}

@JvmInline value class SessionId private constructor(val value: String) {
    companion object { fun of(value: String) = SessionId(value.also { require(it.isNotEmpty()) }) }
}
@JvmInline value class AgentInstanceId private constructor(val value: String) {
    companion object { fun of(value: String) = AgentInstanceId(value.also { require(it.isNotEmpty()) }) }
}
@JvmInline value class AgentDefinitionId private constructor(val value: String) {
    companion object { fun of(value: String) = AgentDefinitionId(value.also { require(it.isNotEmpty()) }) }
}
@JvmInline value class AgentDefinitionRevision private constructor(val value: String) {
    companion object { fun of(value: String) = AgentDefinitionRevision(value.also { require(it.isNotEmpty()) }) }
}

data class ExecutionLimits(val maxIterations: UInt) {
    init {
        require(maxIterations > 0u) { "max iterations must be non-zero" }
    }
}

enum class ExecutionOutcomeKind { COMPLETED, SUSPENDED, STOPPED, FAILED }

sealed interface ExecutionStatus {
    data object Active : ExecutionStatus
    data class Closed(val kind: ExecutionOutcomeKind) : ExecutionStatus
}

sealed interface BeginIteration {
    data class Started(val iteration: UInt) : BeginIteration
    data object IterationLimitReached : BeginIteration
}

sealed class ControlException(message: String) : IllegalStateException(message) {
    data class CursorBeyondLimit(val completed: UInt, val maximum: UInt) :
        ControlException("completed iteration cursor $completed exceeds limit $maximum")

    data object AlreadyClosed : ControlException("execution is already closed")
}

class ExecutionControl private constructor(
    val turnId: TurnId,
    val executionId: ExecutionId,
    val limits: ExecutionLimits,
    completedIterations: UInt,
) {
    var completedIterations: UInt = completedIterations
        private set

    var status: ExecutionStatus = ExecutionStatus.Active
        private set

    fun beginIteration(): BeginIteration {
        requireActive()
        if (completedIterations == limits.maxIterations) {
            status = ExecutionStatus.Closed(ExecutionOutcomeKind.STOPPED)
            return BeginIteration.IterationLimitReached
        }
        completedIterations += 1u
        return BeginIteration.Started(completedIterations)
    }

    fun close(kind: ExecutionOutcomeKind) {
        requireActive()
        status = ExecutionStatus.Closed(kind)
    }

    private fun requireActive() {
        if (status is ExecutionStatus.Closed) throw ControlException.AlreadyClosed
    }

    companion object {
        fun create(
            turnId: TurnId,
            executionId: ExecutionId,
            completedIterations: UInt,
            limits: ExecutionLimits,
        ): ExecutionControl {
            if (completedIterations > limits.maxIterations) {
                throw ControlException.CursorBeyondLimit(completedIterations, limits.maxIterations)
            }
            return ExecutionControl(turnId, executionId, limits, completedIterations)
        }
    }
}
