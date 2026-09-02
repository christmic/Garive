package com.garive.eng.kt.multiagent

/** Maximum stable named peers admitted to one durable Session. */
public const val MAX_NAMED_SESSION_AGENTS: Int = 10
private const val MAX_DISPLAY_NAME_BYTES: Int = 64

/** Stable Runtime identity and Session-unique display metadata. */
@ConsistentCopyVisibility
public data class NamedAgent private constructor(
    public val agentInstanceId: String,
    public val displayName: String,
) {
    public companion object {
        /** Validates an identity/name binding. */
        public fun create(agentInstanceId: String, displayName: String): DelegationContractResult<NamedAgent> =
            if (validId(agentInstanceId) && validText(displayName, MAX_DISPLAY_NAME_BYTES)) {
                success(NamedAgent(agentInstanceId, displayName))
            } else failure(DelegationErrorCode.INVALID_DELEGATION)
    }
}

/** Immutable named-peer roster; order conveys no authority. */
@ConsistentCopyVisibility
public data class SessionRoster private constructor(public val members: List<NamedAgent>) {
    /** Resolves exact roster membership by Runtime identity. */
    public fun contains(agentInstanceId: String): Boolean = members.any { it.agentInstanceId == agentInstanceId }

    public companion object {
        /** Admits at most ten unique identities and exact names. */
        public fun create(members: List<NamedAgent>): DelegationContractResult<SessionRoster> =
            if (members.size <= MAX_NAMED_SESSION_AGENTS &&
                members.map(NamedAgent::agentInstanceId).distinct().size == members.size &&
                members.map(NamedAgent::displayName).distinct().size == members.size
            ) success(SessionRoster(members.toList())) else failure(DelegationErrorCode.INVALID_DELEGATION)
    }
}

/** Runtime-resolved target of one temporary delegation edge. */
public sealed interface AssigneeSelector {
    public data class Anonymous(public val definitionId: String, public val definitionRevision: String) : AssigneeSelector
    public data class ForkSelf(
        public val sourceAgentInstanceId: String,
        public val throughPosition: ULong,
        public val branchName: String?,
    ) : AssigneeSelector
    public data class Named(public val agentInstanceId: String) : AssigneeSelector

    public companion object {
        /** Validates a task-scoped anonymous definition target. */
        public fun anonymous(definitionId: String, definitionRevision: String): DelegationContractResult<AssigneeSelector> =
            if (validId(definitionId) && validId(definitionRevision)) success(Anonymous(definitionId, definitionRevision))
            else failure(DelegationErrorCode.INVALID_DELEGATION)

        /** Validates a real self-fork at one durable Session prefix. */
        public fun forkSelf(
            sourceAgentInstanceId: String,
            throughPosition: ULong,
            branchName: String?,
        ): DelegationContractResult<AssigneeSelector> =
            if (validId(sourceAgentInstanceId) && throughPosition != 0uL &&
                (branchName == null || validText(branchName, MAX_DISPLAY_NAME_BYTES))
            ) success(ForkSelf(sourceAgentInstanceId, throughPosition, branchName))
            else failure(DelegationErrorCode.INVALID_DELEGATION)

        /** Validates an existing equal named Session peer. */
        public fun named(agentInstanceId: String, roster: SessionRoster): DelegationContractResult<AssigneeSelector> =
            if (roster.contains(agentInstanceId)) success(Named(agentInstanceId))
            else failure(DelegationErrorCode.INVALID_DELEGATION)
    }
}

/** Result delivery selected independently from assignee identity. */
public enum class DeliveryPolicy {
    NOTIFY,
    AWAIT_BEFORE_FINAL,
    SUSPEND_EXECUTION,
}
