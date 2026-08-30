package com.garive.mobile.host

import com.garive.host.v1.*
import kotlin.test.*
import okio.ByteString.Companion.encodeUtf8

public class ProductHostMappingTest {
    private val digest: String = "a".repeat(64)

    @Test
    public fun retainsCompleteH2H3PublicValues(): Unit {
        val definitions = AgentDefinitionPageV1("v1", listOf(
            AgentDefinitionSummaryV1("v1", "definition-a", "revision-a", listOf("chat", "tools")),
        )).toDefinitionsLoaded()
        val sessions = SessionPageV1("v1", listOf(SessionSummaryV1(
            "v1", "session-a", "agent-a", "definition-a", "revision-a", "2026-08-30T00:00:00Z",
            9, "turn-a", "suspended", 1,
        )), null).toSessionPageLoaded()
        val timeline = completeTimeline().toTimelineLoaded("session-a")
        assertEquals("revision-a", definitions.definitions.single().definitionRevision)
        assertEquals("revision-a", sessions.sessions.single().definitionRevision)
        assertEquals("hello", timeline.items.single().userText)
        assertEquals("suspension.approval.title", timeline.items.single().suspension?.titleKey)
        assertEquals(digest, timeline.items.single().suspension?.responseSchemaDigest)
        assertEquals("agent.activity.effect", timeline.activities.single().labelKey)
    }

    @Test
    public fun unknownEventSurvivesAndUnsafePromptFailsContentFree(): Unit {
        val event = HostEventV1("v1", "session-a", 10, "future.event", "turn-a", "execution-a", "", null)
            .toProductEvent("session-a")
        assertEquals("future.event", event.event)
        val unsafe = completeTimeline().let { page ->
            page.copy(items = page.items.map { item -> item.copy(suspension = item.suspension?.copy(
                prompt_json = """{"schema_version":1,"title_key":"safe","action_label_key":"safe","secret":"leak"}""".encodeUtf8(),
            )) })
        }
        val error = assertFailsWith<ProductHostMappingException> { unsafe.toTimelineLoaded("session-a") }
        assertFalse(error.toString().contains("leak"))
        assertEquals("invalid_host_value", error.error.code)
    }

    private fun completeTimeline(): TurnTimelinePageV1 {
        val prompt = """{"schema_version":1,"title_key":"suspension.approval.title","action_label_key":"suspension.approval.continue"}"""
        val suspension = SuspensionViewV1("suspension-a", 3, "approval",
            "garive.public-suspension-prompt.v1", prompt.encodeUtf8(), digest, "{}".encodeUtf8(), digest)
        val activity = HostActivityV1("v1", "activity-a", "effect", "agent.activity.effect", "running", 8, false, null)
        val item = TurnTimelineItemV1("turn-a", 1, 9, "suspended", "hello", null, suspension, false, listOf(activity))
        return TurnTimelinePageV1("v1", "session-a", listOf(item), 9, 9, false)
    }
}
