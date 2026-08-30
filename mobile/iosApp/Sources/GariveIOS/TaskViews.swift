#if canImport(GariveShared)
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
                Section("Start with a clear outcome") {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 10) {
                            ForEach(mobileGoalStarters, id: \.label) { starter in
                                Button { prompt = starter.prompt } label: {
                                    VStack(alignment: .leading, spacing: 6) {
                                        Text(starter.label).font(.subheadline.weight(.semibold))
                                            .foregroundStyle(GarivePalette.coral)
                                        Text(starter.prompt).font(.caption).foregroundStyle(.secondary)
                                            .multilineTextAlignment(.leading).lineLimit(2)
                                    }
                                    .frame(width: 210, alignment: .leading).padding(12)
                                    .background(GarivePalette.raised, in: RoundedRectangle(cornerRadius: 14))
                                }.buttonStyle(.plain)
                            }
                        }.padding(.vertical, 2)
                    }
                }
                Section("Goal") {
                    TextEditor(text: $prompt).frame(minHeight: 150)
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
                    Button("Start") { model.start(definitionID: selectedID, text: prompt) }
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
        }.presentationDetents(dynamicTypeSize.isAccessibilitySize ? [.large] : [.medium, .large])
    }

    private var selectedID: String { definitionID.isEmpty ? agents.first?.definitionId ?? "" : definitionID }
    private let maxInputBytes = 16_384
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

struct ConversationView: View {
    @ObservedObject var model: MobileViewModel
    let state: MobileWorkState
    @Environment(\.dismiss) private var dismiss
    @State private var confirmingCancel = false
    @State private var confirmingAbandonRetry = false

    var body: some View {
        VStack(spacing: 0) {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 18) {
                        if state.pendingCommand != nil || state.noticeCode != nil {
                            RetryNotice(
                                code: state.noticeCode ?? "command_unknown",
                                pending: state.pendingCommand != nil,
                                retry: model.retryExact,
                                abandon: { confirmingAbandonRetry = true }
                            )
                        }
                        ForEach(state.timeline, id: \.turnId) { turn in
                            TurnView(
                                turn: turn,
                                controlsEnabled: state.connection == .online && state.pendingCommand == nil,
                                respond: { model.continueDecision($0) }
                            )
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
            Composer(
                text: Binding(get: { state.draft }, set: { value in model.editDraft(value) }),
                sending: state.pendingCommand != nil,
                enabled: canCompose
            ) {
                model.send(state.draft)
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
            if canCancel {
                ToolbarItem {
                    Button(role: .destructive) { confirmingCancel = true } label: { Image(systemName: "stop.circle") }
                        .accessibilityLabel("Stop current work")
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
    let controlsEnabled: Bool
    let respond: (String) -> Void
    @State private var response = ""
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
                                Text(activity.label).font(.subheadline)
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
            if let decision = turn.decision {
                DecisionCard(decision: decision, response: $response, enabled: controlsEnabled, submit: respond)
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
            Text(text).textSelection(.enabled).padding(15)
                .background(GarivePalette.raised, in: UnevenRoundedRectangle(
                    topLeadingRadius: 18, bottomLeadingRadius: 18, bottomTrailingRadius: 5, topTrailingRadius: 18
                ))
                .foregroundStyle(.primary)
        }
    }
}

private struct AssistantMessage: View {
    let text: String
    let status: MobileWorkStatus
    let truncated: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "sparkles")
                .font(.subheadline.weight(.semibold)).foregroundStyle(GarivePalette.coral)
                .frame(width: 34, height: 34)
                .background(GarivePalette.coral.opacity(0.10), in: RoundedRectangle(cornerRadius: 10))
            VStack(alignment: .leading, spacing: 10) {
                Text(text).textSelection(.enabled).font(.body)
                if truncated {
                    Text("Display content was safely bounded").font(.caption).foregroundStyle(.secondary)
                }
                Divider()
            }
            Spacer(minLength: 0)
        }
    }
}

private struct DecisionCard: View {
    let decision: MobileDecision
    @Binding var response: String
    let enabled: Bool
    let submit: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 13) {
            Label("Approval needed", systemImage: "hand.raised.fill").font(.headline).foregroundStyle(GarivePalette.amber)
            Text(decision.prompt.isEmpty ? decision.title : decision.prompt).font(.body)
            Text("This Turn only · one response · committed history stays")
                .font(.caption.weight(.medium)).foregroundStyle(GarivePalette.amber)
            if !decision.kind.lowercased().contains("approval") {
                TextField("Your response", text: $response).textFieldStyle(.roundedBorder)
            }
            if decision.kind.lowercased().contains("approval") {
                HStack {
                    Button("Decline") { submit("false") }.buttonStyle(.bordered).frame(maxWidth: .infinity)
                    Button("Approve once") { submit("true") }.buttonStyle(.borderedProminent).frame(maxWidth: .infinity)
                }.disabled(!enabled)
            } else {
                Button(decision.actionLabel) { submit(response) }.buttonStyle(.borderedProminent)
                    .disabled(!enabled || response.utf8.count > 16_384 || response.isEmpty)
            }
        }.padding(17).background(GarivePalette.panel, in: RoundedRectangle(cornerRadius: 20))
            .overlay(RoundedRectangle(cornerRadius: 18).stroke(GarivePalette.amber.opacity(0.35)))
    }
}

private struct Composer: View {
    @Binding var text: String
    let sending: Bool
    let enabled: Bool
    let send: () -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .bottom, spacing: 10) {
                TextField(enabled ? "Give the Agent direction" : "Waiting for committed state", text: $text, axis: .vertical)
                    .lineLimit(1...5).padding(.leading, 4)
                Button(action: send) {
                    Image(systemName: "arrow.up").font(.headline).frame(width: 42, height: 42)
                        .background(GarivePalette.coral, in: Circle()).foregroundStyle(.white)
                }.buttonStyle(.plain).disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
                    text.utf8.count > 16_384 || sending || !enabled)
            }
            .padding(7).background(GarivePalette.panel, in: RoundedRectangle(cornerRadius: 20))
            .overlay(RoundedRectangle(cornerRadius: 20).stroke(.secondary.opacity(0.25)))
            .shadow(color: .black.opacity(0.08), radius: 10, y: 3)
            Text("Draft clears only after the server commits").font(.caption2).foregroundStyle(.secondary).padding(.horizontal, 8)
        }.padding(.horizontal, 12).padding(.vertical, 9).background(.ultraThinMaterial)
    }
}

private struct RetryNotice: View {
    let code: String
    let pending: Bool
    let retry: () -> Void
    let abandon: () -> Void
    var body: some View {
        HStack {
            Image(systemName: "exclamationmark.arrow.triangle.2.circlepath")
            Text(code.replacingOccurrences(of: "_", with: " ")).font(.subheadline)
            Spacer()
            if pending {
                VStack(alignment: .trailing) {
                    Button("Retry exact command", action: retry).font(.caption.bold())
                    Button("Forget retry", role: .destructive, action: abandon).font(.caption)
                }
            }
        }.padding(13).background(GarivePalette.amber.opacity(0.1), in: RoundedRectangle(cornerRadius: 14))
    }
}
#endif
