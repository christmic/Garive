package com.garive.mobile.host

import com.garive.host.v1.AgentDefinitionPageV1
import com.garive.host.v1.AgentDefinitionSummaryV1
import com.garive.host.v1.HostActivityV1
import com.garive.host.v1.HostEventV1
import com.garive.host.v1.SessionPageV1
import com.garive.host.v1.SessionSummaryV1
import com.garive.host.v1.SessionViewV1
import com.garive.host.v1.SuspensionViewV1
import com.garive.host.v1.TurnTimelineItemV1
import com.garive.host.v1.TurnTimelinePageV1
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import okio.ByteString.Companion.encodeUtf8

public class HostReadModelWireTest {
    @Test
    public fun generatedReadModelsPreservePresenceAndUnknownStrings(): Unit {
        val definition = AgentDefinitionSummaryV1("v1", "definition-1", "revision-1", listOf("future"))
        val definitions = AgentDefinitionPageV1("v1", listOf(definition))
        assertEquals(definitions, AgentDefinitionPageV1.ADAPTER.decode(definitions.encode()))

        val summary = SessionSummaryV1(
            "v1", "session-1", "agent-1", "definition-1", "revision-1",
            "2026-08-30T00:00:00Z", Long.MAX_VALUE, null, "future_state", 0,
        )
        val sessions = SessionPageV1("v1", listOf(summary), null)
        val decodedSessions = SessionPageV1.ADAPTER.decode(sessions.encode())
        assertEquals(sessions, decodedSessions)
        assertNull(decodedSessions.next_before)
        val view = SessionViewV1("v1", summary, Long.MAX_VALUE)
        assertEquals(view, SessionViewV1.ADAPTER.decode(view.encode()))

        val suspension = SuspensionViewV1(
            "suspension-1", Long.MAX_VALUE, "future_kind", "garive.prompt.v1",
            "{}".encodeUtf8(), "prompt-digest", null, null,
        )
        val activity = HostActivityV1(
            "v1", "activity-1", "future_kind", "agent.activity.future",
            "future_state", Long.MAX_VALUE, false, null,
        )
        val timeline = TurnTimelinePageV1(
            "v1", "session-1",
            listOf(
                TurnTimelineItemV1(
                    "turn-1", 1, Long.MAX_VALUE, "future_state", "hello", null,
                    suspension, false, listOf(activity),
                ),
            ),
            Long.MAX_VALUE, Long.MAX_VALUE, false,
        )
        val decodedTimeline = TurnTimelinePageV1.ADAPTER.decode(timeline.encode())
        assertEquals(timeline, decodedTimeline)
        assertNull(decodedTimeline.items.single().completion_text)
        assertNull(decodedTimeline.items.single().activities.single().safe_code)

        val event = HostEventV1(
            "v1", "session-1", Long.MAX_VALUE, "agent.activity.future", "turn-1",
            "execution-1", "", activity,
        )
        assertEquals(event, HostEventV1.ADAPTER.decode(event.encode()))
    }
}
