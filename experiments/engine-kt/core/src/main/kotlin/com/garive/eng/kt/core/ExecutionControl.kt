package com.garive.eng.kt.core

@JvmInline
/** Validated non-empty Turn identity. */
public value class TurnId private constructor(public val value: String) {
    public companion object {
        /** Validates and constructs a Turn identity. */
        public fun of(value: String): TurnId {
            require(value.isNotEmpty()) { "turn identity cannot be empty" }
            return TurnId(value)
        }
    }
}

@JvmInline
/** Validated non-empty disposable Execution identity. */
public value class ExecutionId private constructor(public val value: String) {
    public companion object {
        /** Validates and constructs an Execution identity. */
        public fun of(value: String): ExecutionId {
            require(value.isNotEmpty()) { "execution identity cannot be empty" }
            return ExecutionId(value)
        }
    }
}

/** Validated non-empty Session identity. */
@JvmInline public value class SessionId private constructor(public val value: String) {
    public companion object {
        public fun of(value: String): SessionId = SessionId(value.also { require(it.isNotEmpty()) })
    }
}
/** Validated Runtime-owned Agent instance identity. */
@JvmInline public value class AgentInstanceId private constructor(public val value: String) {
    public companion object {
        public fun of(value: String): AgentInstanceId = AgentInstanceId(value.also { require(it.isNotEmpty()) })
    }
}
/** Validated Agent definition identity. */
@JvmInline public value class AgentDefinitionId private constructor(public val value: String) {
    public companion object {
        public fun of(value: String): AgentDefinitionId = AgentDefinitionId(value.also { require(it.isNotEmpty()) })
    }
}
/** Validated exact Agent definition revision. */
@JvmInline public value class AgentDefinitionRevision private constructor(public val value: String) {
    public companion object {
        public fun of(value: String): AgentDefinitionRevision =
            AgentDefinitionRevision(value.also { require(it.isNotEmpty()) })
    }
}

/** Non-zero hard limits for one kernel Execution. */
public data class ExecutionLimits(public val maxIterations: UInt) {
    init {
        require(maxIterations > 0u) { "max iterations must be non-zero" }
    }
}

/** Terminal class recorded when an Execution closes. */
public enum class ExecutionOutcomeKind { COMPLETED, SUSPENDED, STOPPED, FAILED }

/** Lifecycle status of the disposable control projection. */
public sealed interface ExecutionStatus {
    public data object Active : ExecutionStatus
    public data class Closed(public val kind: ExecutionOutcomeKind) : ExecutionStatus
}

/** Result of attempting to enter the next bounded iteration. */
public sealed interface BeginIteration {
    public data class Started(public val iteration: UInt) : BeginIteration
    public data object IterationLimitReached : BeginIteration
}

/** Rejected transition in [ExecutionControl]. */
public sealed class ControlException protected constructor(message: String) : IllegalStateException(message) {
    public data class CursorBeyondLimit(public val completed: UInt, public val maximum: UInt) :
        ControlException("completed iteration cursor $completed exceeds limit $maximum")

    public data object AlreadyClosed : ControlException("execution is already closed")
}

/** Disposable bounded control projection reconstructed from durable progress. */
public class ExecutionControl private constructor(
    public val turnId: TurnId,
    public val executionId: ExecutionId,
    public val limits: ExecutionLimits,
    completedIterations: UInt,
) {
    /** Iterations entered so far. */
    public var completedIterations: UInt = completedIterations
        private set

    /** Active or exactly one terminal status. */
    public var status: ExecutionStatus = ExecutionStatus.Active
        private set

    /** Starts the next iteration or closes as stopped at the iteration cap. */
    public fun beginIteration(): BeginIteration {
        requireActive()
        if (completedIterations == limits.maxIterations) {
            status = ExecutionStatus.Closed(ExecutionOutcomeKind.STOPPED)
            return BeginIteration.IterationLimitReached
        }
        completedIterations += 1u
        return BeginIteration.Started(completedIterations)
    }

    /** Closes an active Execution with one terminal class. */
    public fun close(kind: ExecutionOutcomeKind): Unit {
        requireActive()
        status = ExecutionStatus.Closed(kind)
    }

    private fun requireActive() {
        if (status is ExecutionStatus.Closed) throw ControlException.AlreadyClosed
    }

    public companion object {
        /** Restores an active controller from a durable completed-iteration cursor. */
        public fun create(
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
