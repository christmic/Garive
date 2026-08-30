package com.garive.mobile.application

import com.garive.host.v1.AgentDefinitionSummaryV1
import com.garive.host.v1.SessionSummaryV1
import com.garive.host.v1.TurnTimelineItemV1
import com.garive.mobile.model.MobileActivityItem
import com.garive.mobile.model.MobileAgentCard
import com.garive.mobile.model.MobileDecision
import com.garive.mobile.model.MobileSessionCard
import com.garive.mobile.model.MobileTurnItem
import com.garive.mobile.model.MobileWorkStatus

internal fun agentCard(value: AgentDefinitionSummaryV1): MobileAgentCard = MobileAgentCard(
    value.definition_id,
    value.definition_id.humanize(),
    value.definition_revision,
    value.capabilities,
)

internal fun sessionCard(
    value: SessionSummaryV1,
    definitions: Map<String, MobileAgentCard>,
): MobileSessionCard = MobileSessionCard(
    value.session_id,
    definitions[value.definition_id]?.displayName ?: value.definition_id.humanize(),
    status(value.latest_turn_state),
    value.opened_at,
    value.latest_position,
    value.turn_count,
)

internal fun turnItem(value: TurnTimelineItemV1): MobileTurnItem = MobileTurnItem(
    value.turn_id,
    value.user_text,
    value.completion_text,
    status(value.state),
    value.latest_position,
    value.content_truncated,
    value.suspension?.let { suspension ->
        MobileDecision(
            suspension.suspension_id,
            suspension.session_version,
            suspension.kind,
            if (suspension.kind == "approval_required") "Approval needed" else "Input needed",
            if (suspension.kind == "approval_required") "Approve" else "Respond",
        )
    },
    value.activities.map { activity ->
        MobileActivityItem(
            activity.activity_id,
            activity.label_key.substringAfterLast('.').humanize(),
            activity.state,
            activity.terminal,
            activity.safe_code,
        )
    },
)

internal fun status(value: String?): MobileWorkStatus = when (value) {
    null, "" -> MobileWorkStatus.READY
    "running" -> MobileWorkStatus.WORKING
    "suspended" -> MobileWorkStatus.NEEDS_INPUT
    "completed" -> MobileWorkStatus.COMPLETED
    "stopped" -> MobileWorkStatus.STOPPED
    "failed" -> MobileWorkStatus.FAILED
    else -> MobileWorkStatus.UPDATED
}

private fun String.humanize(): String = split('-', '_', '.').filter { it.isNotEmpty() }
    .joinToString(" ") { word -> word.replaceFirstChar { it.uppercase() } }
