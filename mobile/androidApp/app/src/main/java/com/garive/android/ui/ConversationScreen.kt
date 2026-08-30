package com.garive.android.ui

import androidx.compose.foundation.BorderStroke
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.automirrored.rounded.Send
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.VerifiedUser
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
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
    onContinue: () -> Unit,
    onRetry: () -> Unit,
    onAbandonRetry: () -> Unit,
) {
    val latest = state.timeline.lastOrNull()
    Column(Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Rounded.ArrowBack, contentDescription = "Back to Work")
            }
            Column(Modifier.weight(1f).padding(horizontal = 4.dp)) {
                Text("Agent work", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
                Text(
                    latest?.status?.label() ?: "Ready",
                    color = latest?.status?.let { statusColor(it) } ?: MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.labelMedium,
                )
            }
            if (latest?.status in setOf(MobileWorkStatus.WORKING, MobileWorkStatus.NEEDS_INPUT)) {
                IconButton(onClick = onCancel) {
                    Icon(Icons.Rounded.Close, contentDescription = "Request cancellation")
                }
            }
        }
        LazyColumn(
            modifier = Modifier.weight(1f).padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            item {
                Surface(
                    shape = RoundedCornerShape(16.dp),
                    color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f),
                ) {
                    Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                        Icon(
                            Icons.Rounded.VerifiedUser,
                            contentDescription = null,
                            tint = GariveMint,
                        )
                        Text(
                            "Durable server history · safe public activity",
                            modifier = Modifier.padding(start = 8.dp),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
            items(state.timeline, key = { it.turnId }) { TurnCard(it) }
            if (state.timeline.isEmpty()) {
                item {
                    Column(Modifier.padding(vertical = 36.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("What should the Agent do?", style = MaterialTheme.typography.headlineSmall)
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
            )
        } else {
            MessageComposer(
                draft = state.draft,
                onDraft = onDraft,
                onSend = onSend,
                busy = state.pendingCommand != null,
                enabled = latest?.status !in setOf(MobileWorkStatus.WORKING, MobileWorkStatus.NEEDS_INPUT),
            )
        }
        if (state.pendingCommand != null) {
            Surface(color = MaterialTheme.colorScheme.errorContainer) {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("Result unknown. Retry the exact command.", modifier = Modifier.weight(1f))
                    Column(horizontalAlignment = Alignment.End) {
                        OutlinedButton(onClick = onRetry) {
                            Icon(Icons.Rounded.Refresh, contentDescription = null)
                            Text("Retry exact", modifier = Modifier.padding(start = 6.dp))
                        }
                        androidx.compose.material3.TextButton(onClick = onAbandonRetry) {
                            Text("Forget retry")
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun TurnCard(turn: MobileTurnItem) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
            Surface(
                shape = RoundedCornerShape(20.dp, 20.dp, 5.dp, 20.dp),
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.sizeIn(maxWidth = 340.dp),
            ) {
                Text(
                    turn.userText,
                    color = MaterialTheme.colorScheme.onPrimary,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    style = MaterialTheme.typography.bodyLarge,
                )
            }
        }
        if (!turn.responseText.isNullOrBlank()) {
            Surface(
                shape = RoundedCornerShape(5.dp, 20.dp, 20.dp, 20.dp),
                color = MaterialTheme.colorScheme.surface,
                border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline.copy(alpha = 0.22f)),
                modifier = Modifier.sizeIn(maxWidth = 560.dp),
            ) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(turn.responseText.orEmpty(), style = MaterialTheme.typography.bodyLarge)
                    if (turn.contentTruncated) {
                        Text(
                            "Display content was safely bounded",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
        if (turn.activities.isNotEmpty()) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                turn.activities.take(3).forEach { activity ->
                    AssistChip(
                        onClick = {},
                        label = { Text("${activity.label} · ${activity.state.replace('_', ' ')}") },
                    )
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
    Surface(shadowElevation = 10.dp, color = MaterialTheme.colorScheme.background) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .imePadding()
                .padding(12.dp),
            verticalAlignment = Alignment.Bottom,
        ) {
            OutlinedTextField(
                value = draft,
                onValueChange = onDraft,
                modifier = Modifier.weight(1f),
                placeholder = { Text(if (enabled) "Give the Agent direction" else "Agent is still working") },
                enabled = enabled && !busy,
                minLines = 1,
                maxLines = 5,
                shape = RoundedCornerShape(18.dp),
            )
            FilledIconButton(
                onClick = onSend,
                enabled = enabled && !busy && draft.isNotBlank(),
                modifier = Modifier.padding(start = 8.dp),
            ) {
                Icon(Icons.AutoMirrored.Rounded.Send, contentDescription = "Send to Agent")
            }
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
    onContinue: () -> Unit,
    busy: Boolean,
) {
    Surface(
        color = MaterialTheme.colorScheme.tertiaryContainer,
        shadowElevation = 12.dp,
    ) {
        Column(
            Modifier.fillMaxWidth().navigationBarsPadding().imePadding().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(title, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
            Text(
                prompt.ifBlank { "Review the public request before continuing this exact suspended Turn." },
                color = MaterialTheme.colorScheme.onTertiaryContainer,
            )
            if (!approval) {
                OutlinedTextField(
                    value = draft,
                    onValueChange = onDraft,
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = { Text("Your response") },
                    enabled = !busy,
                    maxLines = 4,
                    shape = RoundedCornerShape(16.dp),
                )
            }
            Button(
                onClick = onContinue,
                enabled = !busy && (approval || draft.isNotBlank()),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text(action)
            }
        }
    }
}
