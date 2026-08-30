package com.garive.mobile.model

/** Stable top-level mobile destinations. */
public enum class MobileDestination { WORK, SESSIONS, AGENTS, SETTINGS, CONVERSATION }

/** Truthful authenticated connection presentation. */
public enum class MobileConnectionState { CONNECTING, ONLINE, RECONNECTING, OFFLINE, SIGNED_OUT, SECURITY_ERROR }

/** Public mobile work states derived from Host lifecycle values. */
public enum class MobileWorkStatus { READY, WORKING, NEEDS_INPUT, COMPLETED, STOPPED, FAILED, UPDATED }

/** Installed Agent card rendered by native apps. */
public data class MobileAgentCard(
    public val definitionId: String,
    public val displayName: String,
    public val revision: String,
    public val capabilities: List<String>,
)

/** Durable Session card rendered in Work and Sessions. */
public data class MobileSessionCard(
    public val sessionId: String,
    public val agentName: String,
    public val status: MobileWorkStatus,
    public val openedAt: String,
    public val latestPosition: Long,
    public val turnCount: Long,
)

/** Redacted committed Agent activity rendered beneath a Turn. */
public data class MobileActivityItem(
    public val activityId: String,
    public val label: String,
    public val state: String,
    public val terminal: Boolean,
    public val safeCode: String?,
)

/** Restart-safe supported suspension coordinates. */
public data class MobileDecision(
    public val suspensionId: String,
    public val sessionVersion: Long,
    public val kind: String,
    public val title: String,
    public val prompt: String,
    public val actionLabel: String,
)

/** One complete durable Turn rendered in the conversation timeline. */
public data class MobileTurnItem(
    public val turnId: String,
    public val userText: String,
    public val responseText: String?,
    public val status: MobileWorkStatus,
    public val latestPosition: Long,
    public val contentTruncated: Boolean,
    public val decision: MobileDecision?,
    public val activities: List<MobileActivityItem>,
)

/** Pending mutation whose identity must survive exact retry. */
public data class MobilePendingCommand(
    public val kind: String,
    public val commandId: String,
    public val sessionId: String?,
    public val turnId: String?,
)

/** Complete immutable state consumed by Compose and SwiftUI. */
public data class MobileWorkState(
    public val destination: MobileDestination = MobileDestination.WORK,
    public val connection: MobileConnectionState = MobileConnectionState.CONNECTING,
    public val agents: List<MobileAgentCard> = emptyList(),
    public val sessions: List<MobileSessionCard> = emptyList(),
    public val selectedSessionId: String? = null,
    public val timeline: List<MobileTurnItem> = emptyList(),
    public val timelineCursor: Long = 0,
    public val draft: String = "",
    public val pendingCommand: MobilePendingCommand? = null,
    public val noticeCode: String? = null,
    public val refreshing: Boolean = false,
) {
    /** Sessions currently requiring an explicit user response. */
    public val attention: List<MobileSessionCard>
        get() = sessions.filter { it.status == MobileWorkStatus.NEEDS_INPUT }

    /** Sessions with active server work. */
    public val running: List<MobileSessionCard>
        get() = sessions.filter { it.status == MobileWorkStatus.WORKING }

    /** Recently terminal or ready Sessions. */
    public val recent: List<MobileSessionCard>
        get() = sessions.filter { it.status !in setOf(MobileWorkStatus.NEEDS_INPUT, MobileWorkStatus.WORKING) }
}
