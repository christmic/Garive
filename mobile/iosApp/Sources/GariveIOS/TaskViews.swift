#if GARIVE_SHARED_AVAILABLE
import SwiftUI
@preconcurrency import GariveShared

struct NewTaskView: View {
    @ObservedObject var model: MobileViewModel
    let agents: [MobileAgentCard]
    @Environment(\.dismiss) private var dismiss
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @State private var definitionID = ""
    @State private var prompt = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("Agent") {
                    if dynamicTypeSize.isAccessibilitySize {
                        Picker("Choose an agent", selection: $definitionID) {
                            ForEach(agents, id: \.definitionId) { agent in
                                Text(agent.displayName).tag(agent.definitionId)
                            }
                        }.pickerStyle(.inline)
                    } else {
                        Picker("Choose an agent", selection: $definitionID) {
                            ForEach(agents, id: \.definitionId) { agent in
                                Text(agent.displayName).tag(agent.definitionId)
                            }
                        }
                    }
                }
                if showsMobileGoalStarters(prompt) {
                    Section("Start with a clear outcome") {
                        ForEach(mobileGoalStarters, id: \.label) { starter in
                            Button { prompt = starter.prompt } label: {
                                HStack(spacing: 12) {
                                    Text(starter.label).font(.subheadline.weight(.semibold))
                                        .foregroundStyle(GarivePalette.coral)
                                    Text(starter.prompt).font(.caption).foregroundStyle(.secondary)
                                        .multilineTextAlignment(.leading).lineLimit(2)
                                    Spacer(minLength: 0)
                                    Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
                                }
                                .frame(maxWidth: .infinity, minHeight: GariveMobileMetrics.touchTarget, alignment: .leading)
                            }.buttonStyle(.plain)
                        }
                    }
                }
                Section("Goal") {
                    TextEditor(text: $prompt)
                        .frame(minHeight: 150)
                        .accessibilityLabel("Outcome for the Agent")
                    Text("Be specific about the outcome. The agent keeps working on your service if this app closes.")
                        .font(.footnote).foregroundStyle(.secondary)
                    if prompt.utf8.count > maxInputBytes {
                        Text("Goal is larger than the 16 KiB service limit")
                            .font(.footnote).foregroundStyle(.red)
                    }
                }
            }
            .navigationTitle(dynamicTypeSize.isAccessibilitySize ? "New task" : "New remote task")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { model.dismissNewTask(); dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Start on server") { model.start(definitionID: selectedID, text: prompt) }
                        .disabled(selectedID.isEmpty || prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
                            prompt.utf8.count > maxInputBytes || model.state?.connection != .online)
                }
            }
            .onAppear {
                if definitionID.isEmpty {
                    definitionID = agents.contains { $0.definitionId == model.preferredDefinitionID }
                        ? model.preferredDefinitionID ?? ""
                        : agents.first?.definitionId ?? ""
                }
                prompt = model.state?.draft ?? ""
            }
            .onChange(of: prompt) { _, value in model.editDraft(value) }
        }.presentationDetents([.large])
    }

    private var selectedID: String { definitionID.isEmpty ? agents.first?.definitionId ?? "" : definitionID }
    private let maxInputBytes = mobileMaxInputBytes
}

struct MobileGoalStarter: Equatable {
    let label: String
    let prompt: String
}

let mobileGoalStarters = [
    MobileGoalStarter(label: "Synthesize", prompt: "Turn notes into a clear decision memo"),
    MobileGoalStarter(label: "Analyze", prompt: "Find the key patterns and recommend next steps"),
    MobileGoalStarter(label: "Create", prompt: "Draft a polished project brief from my outline"),
]

func showsMobileGoalStarters(_ draft: String) -> Bool { draft.isEmpty }

struct ConversationView: View {
    @ObservedObject var model: MobileViewModel
    let state: MobileWorkState
    @Environment(\.dismiss) private var dismiss
    @State private var confirmingCancel = false
    @State private var confirmingAbandonRetry = false
    @State private var decisionResponse = ""

    var body: some View {
        VStack(spacing: 0) {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 18) {
                        if state.pendingCommand != nil || state.noticeCode != nil {
                            MobileNoticeBanner(
                                code: state.noticeCode ?? "command_unknown",
                                pending: state.pendingCommand != nil,
                                dismiss: model.dismissNotice,
                                retry: model.retryExact,
                                abandon: { confirmingAbandonRetry = true }
                            )
                        }
                        ForEach(state.timeline, id: \.turnId) { turn in
                            TurnView(turn: turn)
                                .id(turn.turnId)
                        }
                        if state.timeline.isEmpty {
                            ContentUnavailableView("No turns yet", systemImage: "bubble.left.and.bubble.right")
                                .padding(.top, 80)
                        }
                    }.padding(18)
                }
                .onChange(of: state.timeline.count) { _, _ in
                    if let last = state.timeline.last { withAnimation { proxy.scrollTo(last.turnId, anchor: .bottom) } }
                }
            }
            if let decision = state.timeline.last?.decision {
                DecisionRail(
                    decision: decision,
                    response: $decisionResponse,
                    enabled: state.connection == .online && state.pendingCommand == nil
                ) { value in
                    model.continueDecision(value)
                    decisionResponse = ""
                }
            } else {
                Composer(
                    text: Binding(get: { state.draft }, set: { value in model.editDraft(value) }),
                    sending: state.pendingCommand != nil,
                    running: canCancel,
                    enabled: canCompose,
                    send: { model.send(state.draft) },
                    stop: { confirmingCancel = true }
                )
            }
        }
        .background(GarivePalette.ink)
        .navigationTitle(title)
        .compactNavigationTitle()
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Done") { model.select(.work); dismiss() } }
            if !state.timeline.isEmpty {
                ToolbarItem {
                    ShareLink(item: transcript) { Image(systemName: "square.and.arrow.up") }
                        .accessibilityLabel("Share conversation")
                }
            }
        }
        .confirmationDialog("Stop this agent’s current turn?", isPresented: $confirmingCancel, titleVisibility: .visible) {
            Button("Stop turn", role: .destructive) { model.cancel() }
        } message: { Text("Committed work remains in the timeline.") }
        .confirmationDialog("Forget exact retry?", isPresented: $confirmingAbandonRetry, titleVisibility: .visible) {
            Button("Forget retry", role: .destructive) { model.abandonPending() }
            Button("Keep retry", role: .cancel) {}
        } message: {
            Text("The server may already have accepted this command. Refresh history before starting replacement work.")
        }
    }

    private var title: String {
        state.sessions.first(where: { $0.sessionId == state.selectedSessionId })?.agentName ?? "Session"
    }

    private var canCancel: Bool {
        guard let status = state.timeline.last?.status else { return false }
        return status == .working || status == .needsInput
    }

    private var canCompose: Bool {
        guard state.connection == .online, state.pendingCommand == nil else { return false }
        guard let latest = state.timeline.last else { return true }
        return latest.status != .working && latest.status != .needsInput
    }

    private var transcript: String {
        state.timeline.map { turn in
            var value = "You\n\(turn.userText)"
            if let response = turn.responseText, !response.isEmpty {
                value += "\n\nAgent\n\(response)"
            }
            return value
        }.joined(separator: "\n\n")
    }
}

private struct TurnView: View {
    let turn: MobileTurnItem
    @State private var activityExpanded = false

    var body: some View {
        VStack(spacing: 14) {
            UserMessage(text: turn.userText)
            if let text = turn.responseText, !text.isEmpty {
                AssistantMessage(text: text, status: turn.status, truncated: turn.contentTruncated)
            }
            if !turn.activities.isEmpty {
                DisclosureGroup(isExpanded: $activityExpanded) {
                    VStack(alignment: .leading, spacing: 12) {
                        ForEach(turn.activities, id: \.activityId) { activity in
                            HStack(spacing: 9) {
                                Image(systemName: activity.terminal ? "checkmark.circle.fill" : "gearshape.2.fill")
                                    .foregroundStyle(activity.terminal ? GarivePalette.mint : .secondary)
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(activity.label).font(.subheadline)
                                    if let code = activity.safeCode {
                                        Text("Code · \(code)")
                                            .font(.caption.monospaced())
                                            .foregroundStyle(.secondary)
                                            .textSelection(.enabled)
                                    }
                                }
                                Spacer()
                                Text(activity.state.lowercased()).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }.padding(.top, 10)
                } label: {
                    Text("Activity · \(turn.activities.count)").font(.subheadline.weight(.medium))
                }
                .tint(GarivePalette.coral)
                .padding(.leading, 44)
            }
            HStack(spacing: 8) {
                Text(turn.status.name.lowercased().replacingOccurrences(of: "_", with: " "))
                    .font(.caption.weight(.semibold)).foregroundStyle(statusColor)
                if turn.contentTruncated { Label("Truncated", systemImage: "ellipsis.circle").font(.caption).foregroundStyle(.secondary) }
                Spacer()
                Text("Committed").font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    private var statusColor: Color {
        switch turn.status {
        case .completed: GarivePalette.mint
        case .needsInput: GarivePalette.amber
        case .failed: .red
        default: .secondary
        }
    }
}

private struct UserMessage: View {
    let text: String
    var body: some View {
        HStack {
            Spacer(minLength: 44)
            Text(text).textSelection(.enabled)
                .padding(.horizontal, 16).padding(.vertical, 10)
                .containerRelativeFrame(.horizontal) { width, _ in width * 0.70 }
                .background(
                    GarivePalette.raised,
                    in: RoundedRectangle(cornerRadius: GariveMobileMetrics.userPromptRadius, style: .continuous)
                )
                .foregroundStyle(.primary)
        }
    }
}

private struct AssistantMessage: View {
    let text: String
    let status: MobileWorkStatus
    let truncated: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            MobileResponseText(text: text)
            if truncated {
                Text("Display content was safely bounded").font(.caption).foregroundStyle(.secondary)
            }
            Divider()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct DecisionRail: View {
    let decision: MobileDecision
    @Binding var response: String
    let enabled: Bool
    let submit: (String) -> Void

    private var approval: Bool { decision.kind.lowercased().contains("approval") }

    var body: some View {
        VStack(alignment: .leading, spacing: 13) {
            Label(decision.title, systemImage: approval ? "hand.raised.fill" : "text.bubble.fill")
                .font(.headline).foregroundStyle(GarivePalette.amber)
            Text(decision.prompt.isEmpty ? decision.title : decision.prompt).font(.body)
            Text("This Turn only · one response · committed history stays")
                .font(.caption.weight(.medium)).foregroundStyle(GarivePalette.amber)
            if !approval {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 10) { responseField; responseButton }
                    VStack(spacing: 8) { responseField; responseButton }
                }
            }
            if approval {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 10) { declineButton; approveButton }
                    VStack(spacing: 8) { declineButton; approveButton }
                }.disabled(!enabled)
            }
        }
        .padding(16)
        .background(
            GarivePalette.panel,
            in: RoundedRectangle(cornerRadius: GariveMobileMetrics.decisionRadius, style: .continuous)
        )
        .overlay(alignment: .leading) {
            Rectangle().fill(GarivePalette.amber).frame(width: GariveMobileMetrics.attentionEdge)
                .clipShape(RoundedRectangle(cornerRadius: GariveMobileMetrics.decisionRadius, style: .continuous))
        }
        .shadow(color: .black.opacity(0.06), radius: 8, y: 2)
        .padding(.horizontal, 12).padding(.vertical, 9)
        .background(.ultraThinMaterial)
        .accessibilityIdentifier("mobile-decision-rail")
        .accessibilityValue("Needs input for this Turn")
    }

    private var responseField: some View {
        TextField("Your response", text: $response)
            .textFieldStyle(.roundedBorder)
            .submitLabel(.send)
            .onSubmit { if canSubmitResponse { submit(response) } }
    }

    private var responseButton: some View {
        Button(decision.actionLabel) { submit(response) }
            .buttonStyle(.borderedProminent)
            .frame(maxWidth: .infinity, minHeight: GariveMobileMetrics.touchTarget)
            .disabled(!canSubmitResponse)
    }

    private var declineButton: some View {
        Button("Decline") { submit("false") }.buttonStyle(.bordered)
            .frame(maxWidth: .infinity, minHeight: GariveMobileMetrics.touchTarget)
    }

    private var approveButton: some View {
        Button("Approve once") { submit("true") }.buttonStyle(.borderedProminent)
            .frame(maxWidth: .infinity, minHeight: GariveMobileMetrics.touchTarget)
    }

    private var canSubmitResponse: Bool {
        enabled && !response.isEmpty && response.utf8.count <= mobileMaxInputBytes
    }
}

private struct Composer: View {
    @Binding var text: String
    let sending: Bool
    let running: Bool
    let enabled: Bool
    let send: () -> Void
    let stop: () -> Void
    var body: some View {
        HStack(alignment: .bottom, spacing: 10) {
            TextField(enabled ? "Give the Agent direction" : "Waiting for committed state", text: $text, axis: .vertical)
                .lineLimit(1...5).padding(.leading, 4)
            if running {
                Button(action: stop) {
                    Image(systemName: "stop.fill").font(.subheadline.weight(.bold))
                        .frame(width: GariveMobileMetrics.touchTarget, height: GariveMobileMetrics.touchTarget)
                        .background(Color.red, in: Circle()).foregroundStyle(.white)
                }.buttonStyle(.plain).accessibilityLabel("Stop current work").disabled(sending)
            } else {
                Button(action: send) {
                    Image(systemName: "arrow.up").font(.headline)
                        .frame(width: GariveMobileMetrics.touchTarget, height: GariveMobileMetrics.touchTarget)
                        .background(GarivePalette.coral, in: Circle()).foregroundStyle(.white)
                }.buttonStyle(.plain).accessibilityLabel("Send to Agent")
                    .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
                        text.utf8.count > mobileMaxInputBytes || sending || !enabled)
            }
        }
        .padding(7)
        .background(
            GarivePalette.panel,
            in: RoundedRectangle(cornerRadius: GariveMobileMetrics.composerRadius, style: .continuous)
        )
        .shadow(color: .black.opacity(0.06), radius: 8, y: 2)
        .padding(.horizontal, 12).padding(.vertical, 9).background(.ultraThinMaterial)
        .accessibilityIdentifier("mobile-composer")
        .accessibilityValue(running ? "Working on server. Stop action available." : "Ready for a new Turn.")
        .accessibilityHint("Draft clears only after the server commits")
    }
}

struct MobileNoticeBanner: View {
    let code: String
    let pending: Bool
    let dismiss: () -> Void
    let retry: () -> Void
    let abandon: () -> Void
    var body: some View {
        HStack {
            Image(systemName: "exclamationmark.arrow.triangle.2.circlepath")
            Text(mobileNoticeMessage(code)).font(.subheadline)
            Spacer()
            if pending {
                VStack(alignment: .trailing) {
                    Button("Retry exact command", action: retry).font(.caption.bold())
                    Button("Forget retry", role: .destructive, action: abandon).font(.caption)
                }
            } else {
                Button(action: dismiss) {
                    Image(systemName: "xmark").frame(width: 32, height: 32)
                }
                .accessibilityLabel("Dismiss notice")
            }
        }.padding(13).background(GarivePalette.amber.opacity(0.1), in: RoundedRectangle(cornerRadius: 14))
    }
}

func mobileNoticeMessage(_ code: String) -> String {
    switch code {
    case "validation_input_empty": "Add an outcome before sending."
    case "validation_input_too_large": "Outcome is over 16 KiB. Shorten it before sending."
    case "command_unknown": "Result unknown. Verify history or retry the exact command."
    case "pending_retry_abandoned": "Exact retry was forgotten. Verify server history before replacing the work."
    case "runtime_unavailable": "Runtime unavailable. Verified history is still shown."
    case "transport_failure", "follow_deadline": "Connection interrupted. Verified history is still shown."
    case "rate_limited": "The service is busy. Wait before trying again."
    case "actor_forbidden": "This device cannot access that work."
    case "device_reauth_required": "This device must pair again before remote work can continue."
    default: code.replacingOccurrences(of: "_", with: " ").capitalized
    }
}
#endif
