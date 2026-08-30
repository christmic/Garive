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
                enabled: state.connection == .online
            ) {
                model.send(state.draft)
            }
        }
        .background(GarivePalette.ink)
        .navigationTitle(title)
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

    var body: some View {
        VStack(spacing: 14) {
            MessageBubble(text: turn.userText, user: true)
            if let text = turn.responseText, !text.isEmpty { MessageBubble(text: text, user: false) }
            if !turn.activities.isEmpty {
                VStack(alignment: .leading, spacing: 9) {
                    ForEach(turn.activities, id: \.activityId) { activity in
                        HStack(spacing: 9) {
                            Image(systemName: activity.terminal ? "checkmark.circle.fill" : "gearshape.2.fill")
                                .foregroundStyle(activity.terminal ? GarivePalette.mint : .secondary)
                            Text(activity.label).font(.subheadline)
                            Spacer()
                            Text(activity.state.lowercased()).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }.padding(14).background(GarivePalette.raised, in: RoundedRectangle(cornerRadius: 15))
            }
            if let decision = turn.decision {
                DecisionCard(decision: decision, response: $response, enabled: controlsEnabled) {
                    let input = decision.kind.lowercased().contains("approval") ? "true" : response
                    respond(input)
                }
            }
            HStack {
                StatusBadge(status: turn.status.name.lowercased())
                if turn.contentTruncated { Label("Truncated", systemImage: "ellipsis.circle").font(.caption).foregroundStyle(.secondary) }
                Spacer()
            }
        }
    }
}

private struct MessageBubble: View {
    let text: String
    let user: Bool
    var body: some View {
        HStack {
            if user { Spacer(minLength: 44) }
            Text(text).textSelection(.enabled).padding(15)
                .background(user ? GarivePalette.coral : GarivePalette.panel, in: RoundedRectangle(cornerRadius: 18))
                .foregroundStyle(user ? .white : .primary)
            if !user { Spacer(minLength: 28) }
        }
    }
}

private struct DecisionCard: View {
    let decision: MobileDecision
    @Binding var response: String
    let enabled: Bool
    let submit: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 13) {
            Label("Your decision is needed", systemImage: "hand.raised.fill").font(.headline).foregroundStyle(GarivePalette.amber)
            Text(decision.prompt.isEmpty ? decision.title : decision.prompt).font(.body)
            if !decision.kind.lowercased().contains("approval") {
                TextField("Your response", text: $response).textFieldStyle(.roundedBorder)
            }
            Button(decision.actionLabel, action: submit).buttonStyle(.borderedProminent)
                .disabled(!enabled || response.utf8.count > 16_384 ||
                    (!decision.kind.lowercased().contains("approval") && response.isEmpty))
        }.padding(17).background(GarivePalette.amber.opacity(0.09), in: RoundedRectangle(cornerRadius: 18))
            .overlay(RoundedRectangle(cornerRadius: 18).stroke(GarivePalette.amber.opacity(0.35)))
    }
}

private struct Composer: View {
    @Binding var text: String
    let sending: Bool
    let enabled: Bool
    let send: () -> Void
    var body: some View {
        HStack(alignment: .bottom, spacing: 10) {
            TextField("Steer the agent…", text: $text, axis: .vertical).lineLimit(1...5)
                .padding(12).background(GarivePalette.raised, in: RoundedRectangle(cornerRadius: 16))
            Button(action: send) {
                Image(systemName: "arrow.up").font(.headline).frame(width: 42, height: 42)
                    .background(GarivePalette.coral, in: Circle()).foregroundStyle(.white)
            }.buttonStyle(.plain).disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
                text.utf8.count > 16_384 || sending || !enabled)
        }.padding(12).background(.ultraThinMaterial)
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
