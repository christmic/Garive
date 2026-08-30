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

    /** Continues one exact durable suspension. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun continueTurn(
        commandId: String,
        sessionId: String,
        turnId: String,
        suspensionId: String,
        expectedSessionVersion: Long,
        input: String,
        inputJson: Boolean,
    ): TurnCommandResponseV1

    /** Follows committed events until a durable terminal. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun followUntilTerminal(sessionId: String, afterPosition: Long): HostView
}
