package com.garive.android.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.automirrored.rounded.Send
import androidx.compose.material.icons.rounded.AutoAwesome
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.ExpandLess
import androidx.compose.material.icons.rounded.ExpandMore
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Share
import androidx.compose.material.icons.rounded.VerifiedUser
import androidx.compose.material3.Button
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.garive.android.MOBILE_MAX_INPUT_BYTES
import com.garive.mobile.model.MobileConnectionState
import com.garive.mobile.model.MobileTurnItem
import com.garive.mobile.model.MobileWorkState
import com.garive.mobile.model.MobileWorkStatus

/** Full-screen durable conversation and decision surface. */
@Composable
internal fun ConversationScreen(
    state: MobileWorkState,
    onBack: () -> Unit,
    onDraft: (String) -> Unit,
    onSend: () -> Unit,
    onCancel: () -> Unit,
    onShare: () -> Unit,
    onDismissNotice: () -> Unit,
    onContinue: (String) -> Unit,
    onRetry: () -> Unit,
    onAbandonRetry: () -> Unit,
) {
    val latest = state.timeline.lastOrNull()
    val agentName = state.sessions.firstOrNull { it.sessionId == state.selectedSessionId }?.agentName ?: "Agent work"
    Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
        Surface(color = MaterialTheme.colorScheme.background, shadowElevation = 1.dp) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = onBack) {
                    Icon(Icons.AutoMirrored.Rounded.ArrowBack, contentDescription = "Back to Work")
                }
                Column(Modifier.weight(1f).padding(horizontal = 4.dp)) {
                    Text(agentName, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
                    Text(
                        "${latest?.status?.label() ?: "Ready"} · server work continues",
                        color = latest?.status?.let { statusColor(it) } ?: MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.labelMedium,
                    )
                }
                if (state.timeline.isNotEmpty()) {
                    IconButton(onClick = onShare) {
                        Icon(Icons.Rounded.Share, contentDescription = "Share conversation")
                    }
                }
                if (latest?.status in setOf(MobileWorkStatus.WORKING, MobileWorkStatus.NEEDS_INPUT)) {
                    IconButton(onClick = onCancel) {
                        Icon(Icons.Rounded.Close, contentDescription = "Request cancellation")
                    }
                }
            }
        }
        LazyColumn(
            modifier = Modifier.weight(1f).padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            item {
                Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Rounded.VerifiedUser, contentDescription = null, tint = GariveMint)
                    Text(
                        "Committed history · safe public activity",
                        modifier = Modifier.padding(start = 8.dp),
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            items(state.timeline, key = { it.turnId }) { TurnCard(it) }
            if (state.timeline.isEmpty()) {
                item {
                    Column(Modifier.padding(vertical = 36.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(
                            "What should the Agent do?",
                            style = MaterialTheme.typography.headlineSmall,
                            color = MaterialTheme.colorScheme.onBackground,
                        )
                        Text(
                            "Send a clear outcome. You can leave after the server acknowledges it.",
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
            item { Spacer(Modifier.padding(2.dp)) }
        }
        val decision = latest?.decision
        if (decision != null) {
            DecisionComposer(
                title = decision.title,
                prompt = decision.prompt,
                approval = decision.kind == "approval_required",
                action = decision.actionLabel,
                draft = state.draft,
                onDraft = onDraft,
                onContinue = onContinue,
                busy = state.pendingCommand != null,
                enabled = state.connection == MobileConnectionState.ONLINE,
            )
        } else {
            MessageComposer(
                draft = state.draft,
                onDraft = onDraft,
                onSend = onSend,
                busy = state.pendingCommand != null,
                enabled = state.connection == MobileConnectionState.ONLINE &&
                    latest?.status !in setOf(MobileWorkStatus.WORKING, MobileWorkStatus.NEEDS_INPUT),
            )
        }
        if (state.noticeCode != null || state.pendingCommand != null) {
            MobileNoticeBanner(
                code = state.noticeCode ?: "command_unknown",
                pending = state.pendingCommand != null,
                onDismiss = onDismissNotice,
                onRetry = onRetry,
                onAbandonRetry = onAbandonRetry,
            )
        }
    }
}

@Composable
internal fun MobileNoticeBanner(
    code: String,
    pending: Boolean,
    onDismiss: () -> Unit,
    onRetry: () -> Unit,
    onAbandonRetry: () -> Unit,
) {
    Surface(color = MaterialTheme.colorScheme.errorContainer) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(mobileNoticeMessage(code), modifier = Modifier.weight(1f))
            if (pending) {
                Column(horizontalAlignment = Alignment.End) {
                    OutlinedButton(onClick = onRetry) {
                        Icon(Icons.Rounded.Refresh, contentDescription = null)
                        Text("Retry exact", modifier = Modifier.padding(start = 6.dp))
                    }
                    androidx.compose.material3.TextButton(onClick = onAbandonRetry) {
                        Text("Forget retry")
                    }
                }
            } else {
                IconButton(onClick = onDismiss) {
                    Icon(Icons.Rounded.Close, contentDescription = "Dismiss notice")
                }
            }
        }
    }
}

internal fun mobileNoticeMessage(code: String): String = when (code) {
    "validation_input_empty" -> "Add an outcome before sending."
    "validation_input_too_large" -> "Outcome is over 16 KiB. Shorten it before sending."
    "command_unknown" -> "Result unknown. Verify history or retry the exact command."
    "pending_retry_abandoned" -> "Exact retry was forgotten. Verify server history before replacing the work."
    "runtime_unavailable" -> "Runtime unavailable. Verified history is still shown."
    "transport_failure", "follow_deadline" -> "Connection interrupted. Verified history is still shown."
    "rate_limited" -> "The service is busy. Wait before trying again."
    "actor_forbidden" -> "This device cannot access that work."
    "device_reauth_required" -> "This device must pair again before remote work can continue."
    else -> code.replace('_', ' ').replaceFirstChar(Char::uppercase)
}

internal fun conversationTranscript(state: MobileWorkState): String = state.timeline.joinToString("\n\n") { turn ->
    buildString {
        append("You\n")
        append(turn.userText)
        turn.responseText?.takeIf(String::isNotBlank)?.let { response ->
            append("\n\nAgent\n")
            append(response)
        }
    }
}

@Composable
private fun TurnCard(turn: MobileTurnItem) {
    var activityExpanded by remember(turn.turnId) { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
            Surface(
                shape = RoundedCornerShape(20.dp, 20.dp, 5.dp, 20.dp),
                color = MaterialTheme.colorScheme.surfaceVariant,
                modifier = Modifier.sizeIn(maxWidth = 340.dp),
            ) {
                Text(
                    turn.userText,
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    style = MaterialTheme.typography.bodyLarge,
                )
            }
        }
        if (!turn.responseText.isNullOrBlank()) {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.Top) {
                Surface(
                    shape = RoundedCornerShape(10.dp),
                    color = MaterialTheme.colorScheme.primary.copy(alpha = 0.10f),
                ) {
                    Icon(
                        Icons.Rounded.AutoAwesome,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.padding(8.dp),
                    )
                }
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(
                        turn.responseText.orEmpty(),
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onBackground,
                    )
                    if (turn.contentTruncated) {
                        Text(
                            "Display content was safely bounded",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    HorizontalDivider(color = MaterialTheme.colorScheme.outline.copy(alpha = 0.18f))
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            turn.status.label(),
                            color = statusColor(turn.status),
                            style = MaterialTheme.typography.labelMedium,
                        )
                        Spacer(Modifier.weight(1f))
                        Text(
                            "Committed",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
        if (turn.activities.isNotEmpty()) {
            Column(Modifier.padding(start = 44.dp)) {
                TextButton(onClick = { activityExpanded = !activityExpanded }) {
                    Icon(
                        if (activityExpanded) Icons.Rounded.ExpandLess else Icons.Rounded.ExpandMore,
                        contentDescription = null,
                    )
                    Text("Activity · ${turn.activities.size}", modifier = Modifier.padding(start = 4.dp))
                }
                if (activityExpanded) {
                    turn.activities.forEach { activity ->
                        Row(
                            Modifier.fillMaxWidth().padding(vertical = 7.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Column(Modifier.weight(1f)) {
                                Text(
                                    activity.label,
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = MaterialTheme.colorScheme.onBackground,
                                )
                                activity.safeCode?.let { code ->
                                    SelectionContainer {
                                        Text(
                                            "Code · $code",
                                            style = MaterialTheme.typography.labelSmall,
                                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                                        )
                                    }
                                }
                            }
                            Text(
                                activity.state.replace('_', ' '),
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                style = MaterialTheme.typography.labelSmall,
                            )
                        }
                    }
                }
            }
        }
        if (turn.status == MobileWorkStatus.WORKING) {
            Text(
                "Agent is working on the server…",
                color = statusColor(turn.status),
                style = MaterialTheme.typography.labelLarge,
            )
        }
    }
}

@Composable
private fun MessageComposer(
    draft: String,
    onDraft: (String) -> Unit,
    onSend: () -> Unit,
    busy: Boolean,
    enabled: Boolean,
) {
    Surface(color = MaterialTheme.colorScheme.background) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .imePadding()
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Surface(
                shape = RoundedCornerShape(22.dp),
                color = MaterialTheme.colorScheme.surface,
                border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline.copy(alpha = 0.45f)),
                shadowElevation = 7.dp,
            ) {
                Row(
                    Modifier.fillMaxWidth().padding(start = 4.dp, end = 8.dp, top = 3.dp, bottom = 3.dp),
                    verticalAlignment = Alignment.Bottom,
                ) {
                    OutlinedTextField(
                        value = draft,
                        onValueChange = onDraft,
                        modifier = Modifier.weight(1f),
                        placeholder = {
                            Text(if (enabled) "Give the Agent direction" else "Waiting for committed state")
                        },
                        enabled = enabled && !busy,
                        minLines = 1,
                        maxLines = 5,
                        colors = OutlinedTextFieldDefaults.colors(
                            focusedBorderColor = Color.Transparent,
                            unfocusedBorderColor = Color.Transparent,
                            disabledBorderColor = Color.Transparent,
                        ),
                    )
                    FilledIconButton(
                        onClick = onSend,
                        enabled = enabled && !busy && draft.isNotBlank() &&
                            draft.encodeToByteArray().size <= MOBILE_MAX_INPUT_BYTES,
                        modifier = Modifier.padding(bottom = 4.dp),
                    ) {
                        Icon(Icons.AutoMirrored.Rounded.Send, contentDescription = "Send to Agent")
                    }
                }
            }
            Text(
                "Draft clears only after the server commits",
                modifier = Modifier.padding(horizontal = 8.dp),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.labelSmall,
            )
        }
    }
}

@Composable
private fun DecisionComposer(
    title: String,
    prompt: String,
    approval: Boolean,
    action: String,
    draft: String,
    onDraft: (String) -> Unit,
    onContinue: (String) -> Unit,
    busy: Boolean,
    enabled: Boolean,
) {
    Surface(
        color = MaterialTheme.colorScheme.surface,
        shape = RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.tertiary.copy(alpha = 0.45f)),
        shadowElevation = 12.dp,
    ) {
        Column(
            Modifier.fillMaxWidth().navigationBarsPadding().imePadding().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(title, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
            Text(
                prompt.ifBlank { "Review the public request before continuing this exact suspended Turn." },
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                "This Turn only · one response · committed history stays",
                color = MaterialTheme.colorScheme.tertiary,
                style = MaterialTheme.typography.labelMedium,
            )
            if (!approval) {
                OutlinedTextField(
                    value = draft,
                    onValueChange = onDraft,
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = { Text("Your response") },
                    enabled = enabled && !busy,
                    maxLines = 4,
                    shape = RoundedCornerShape(16.dp),
                )
            }
            if (approval) {
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    OutlinedButton(
                        onClick = { onContinue("false") },
                        enabled = enabled && !busy,
                        modifier = Modifier.weight(1f),
                    ) { Text("Decline") }
                    Button(
                        onClick = { onContinue("true") },
                        enabled = enabled && !busy,
                        modifier = Modifier.weight(1f),
                    ) { Text("Approve once") }
                }
            } else {
                Button(
                    onClick = { onContinue(draft) },
                    enabled = enabled && !busy && draft.isNotBlank() &&
                        draft.encodeToByteArray().size <= MOBILE_MAX_INPUT_BYTES,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text(action)
                }
            }
        }
    }
}
