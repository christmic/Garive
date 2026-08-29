package com.garive.eng.kt.core

import com.garive.eng.kt.tools.GovernedToolResult
import com.garive.eng.kt.tools.PreparationError
import com.garive.eng.kt.tools.PreparedToolCall
import com.garive.eng.kt.tools.ToolDefinition
import com.garive.eng.kt.tools.ToolIntent

/** Immutable exact tool capabilities frozen for one Kernel Execution. */
public data class AgentToolCapabilities(public val definitions: List<ToolDefinition>)

/** Governed result returned only after its required facts are durable. */
public data class CommittedGovernedResult(
    public val result: GovernedToolResult,
    public val throughPosition: ULong,
    public val suspensionBinding: GovernedSuspensionBinding?,
)

/** Runtime-owned durable identities carried into the Execution terminal. */
public sealed interface GovernedSuspensionBinding {
    /** One committed approval or external-input interaction. */
    public data class Interaction(
        public val suspensionId: String,
        public val interactionId: String,
        public val invocationId: String,
        public val preparedDigest: String,
    ) : GovernedSuspensionBinding

    /** One uncertain invocation requiring conclusive operator evidence. */
    public data class OperatorReconciliation(
        public val suspensionId: String,
        public val invocationId: String,
        public val preparedDigest: String,
    ) : GovernedSuspensionBinding
}

/** Runtime-owned durable authority and execution boundary used by the Agent loop. */
public interface GovernedEffectPort {
    /** Commits a C4 preparation rejection before returning model feedback. */
    public suspend fun reject(
        sourceModelRequestId: String,
        intent: ToolIntent,
        error: PreparationError,
    ): Result<CommittedGovernedResult>

    /** Allocates, authorizes, and executes or suspends one prepared call. */
    public suspend fun invoke(
        sourceModelRequestId: String,
        prepared: PreparedToolCall,
    ): Result<CommittedGovernedResult>
}

/** Runs the C0-C5 tool-capable bounded Agent loop. */
public suspend fun executeAgent(
    request: AgentTurnRequest,
    capabilities: AgentToolCapabilities,
    ports: AgentExecutionPorts,
    effects: GovernedEffectPort,
): ExecutionReport = executeKernel(request, ports, capabilities.definitions, effects)
