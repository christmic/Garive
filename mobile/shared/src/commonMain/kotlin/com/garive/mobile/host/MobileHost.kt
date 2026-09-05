package com.garive.mobile.host

import com.garive.host.v1.AgentDefinitionPageV1
import com.garive.host.v1.CreateSessionResponseV1
import com.garive.host.v1.SessionPageV1
import com.garive.host.v1.SessionViewV1
import com.garive.host.v1.TurnCommandResponseV1
import com.garive.host.v1.TurnTimelinePageV1
import kotlinx.coroutines.CancellationException

/** Host-only application port consumed by the shared mobile controller. */
public interface MobileHost {
    /** Loads installed definitions. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun agentDefinitions(): AgentDefinitionPageV1

    /** Loads bounded durable Sessions. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun sessions(limit: Int): SessionPageV1

    /** Loads one exact Session. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun session(sessionId: String): SessionViewV1

    /** Loads one bounded durable timeline. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun timeline(sessionId: String, afterPosition: Long, limit: Int): TurnTimelinePageV1

    /** Creates one durable Session. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun createSession(commandId: String, definitionId: String): CreateSessionResponseV1

    /** Starts one durable Turn. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun startTurn(commandId: String, sessionId: String, text: String): TurnCommandResponseV1

    /** Requests durable cancellation. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun cancelTurn(
        commandId: String,
        sessionId: String,
        turnId: String,
        requestedThroughPosition: Long,
    ): TurnCommandResponseV1

    /** Submits plain-text input for one Open Turn (no suspension). */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun steerTurn(
        commandId: String,
        sessionId: String,
        turnId: String,
        text: String,
    ): TurnCommandResponseV1

    /** Submits an operator decision against one ApprovalRequired suspension. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun approvalEvent(
        commandId: String,
        sessionId: String,
        turnId: String,
        suspensionId: String,
        expectedSessionVersion: Long,
        approve: Boolean,
    ): TurnCommandResponseV1

    /** Submits an RFC 8785 typed JSON reply against a schema-bound ExternalInputRequired. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun askReplyEvent(
        commandId: String,
        sessionId: String,
        turnId: String,
        suspensionId: String,
        expectedSessionVersion: Long,
        inputJson: String,
    ): TurnCommandResponseV1

    /** Submits plain-text input against a schema-less ExternalInputRequired or PartialOutput. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun externalInputEvent(
        commandId: String,
        sessionId: String,
        turnId: String,
        suspensionId: String,
        expectedSessionVersion: Long,
        text: String,
    ): TurnCommandResponseV1

    /** Follows committed events until a durable terminal. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun followUntilTerminal(sessionId: String, afterPosition: Long): HostView
}
