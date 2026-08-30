#if canImport(GariveShared)
import SwiftUI
#if os(iOS)
import UIKit
#endif
@preconcurrency import GariveShared

struct WorkView: View {
    @ObservedObject var model: MobileViewModel
    let state: MobileWorkState

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 24) {
                ConnectionBanner(state: state)
                if !state.attention.isEmpty {
                    SessionSection(title: "Needs you", subtitle: "A decision is waiting", sessions: state.attention, model: model)
                }
                if !state.running.isEmpty {
                    SessionSection(title: "Working now", subtitle: "Agents continue on the server", sessions: state.running, model: model)
                }
                SessionSection(title: "Recent", subtitle: "Durable work you can reopen", sessions: state.recent, model: model)
                if state.sessions.isEmpty { EmptyWorkView() }
            }.padding(20)
        }
        .background(GarivePalette.ink)
        .navigationTitle("Remote work")
        .toolbar {
            ToolbarItem { Button { model.refresh() } label: { Image(systemName: "arrow.clockwise") } }
            ToolbarItem { Button { model.showNewTask() } label: { Label("New task", systemImage: "plus") } }
        }
        .refreshable { model.refresh() }
    }
}

struct SessionsView: View {
    @ObservedObject var model: MobileViewModel
    let state: MobileWorkState
    @State private var search = ""
    @State private var filter = "All"

    private var matches: [MobileSessionCard] {
        return state.sessions.filter {
            let matchesSearch = search.isEmpty || $0.agentName.localizedCaseInsensitiveContains(search)
                || $0.sessionId.localizedCaseInsensitiveContains(search)
            let matchesFilter = switch filter {
            case "Working": $0.status == .working
            case "Needs you": $0.status == .needsInput
            case "Done": $0.status == .completed || $0.status == .stopped
            default: true
            }
            return matchesSearch && matchesFilter
        }
    }

    var body: some View {
        List {
            Picker("Status", selection: $filter) {
                ForEach(["All", "Working", "Needs you", "Done"], id: \.self) { Text($0) }
            }
            .pickerStyle(.segmented)
            .listRowBackground(GarivePalette.ink)
            ForEach(matches, id: \.sessionId) { session in
                SessionRow(session: session) { model.open(session.sessionId) }
                    .listRowBackground(GarivePalette.panel)
            }
        }
        .scrollContentBackground(.hidden).background(GarivePalette.ink)
        .navigationTitle("Sessions").searchable(text: $search, prompt: "Agent or session")
        .toolbar { Button { model.showNewTask() } label: { Label("New task", systemImage: "plus") } }
        .refreshable { model.refresh() }
    }
}

struct AgentsView: View {
    @ObservedObject var model: MobileViewModel
    let state: MobileWorkState

    var body: some View {
        ScrollView {
            LazyVStack(spacing: 14) {
                ForEach(state.agents, id: \.definitionId) { agent in
                    VStack(alignment: .leading, spacing: 13) {
                        HStack(spacing: 14) {
                            Image(systemName: "cpu.fill").font(.title2).foregroundStyle(GarivePalette.mint)
                                .frame(width: 46, height: 46).background(GarivePalette.mint.opacity(0.12), in: RoundedRectangle(cornerRadius: 14))
                            VStack(alignment: .leading, spacing: 3) {
                                Text(agent.displayName).font(.headline)
                                Text("Revision \(agent.revision)").font(.caption).foregroundStyle(.secondary)
                            }
                            Spacer()
                        }
                        if !agent.capabilities.isEmpty {
                            Text(agent.capabilities.joined(separator: "  ·  ")).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                        }
                        Button("Start with this agent") {
                            model.showNewTask(definitionID: agent.definitionId)
                        }.buttonStyle(.bordered)
                    }
                    .padding(18).background(GarivePalette.panel, in: RoundedRectangle(cornerRadius: 20))
                }
            }.padding(20)
        }.background(GarivePalette.ink).navigationTitle("Agents")
    }
}

struct SettingsView: View {
    @ObservedObject var model: MobileViewModel
    let state: MobileWorkState
    @Binding var theme: String
    @State private var confirmUnpair = false
    @State private var diagnosticsCopied = false

    private var origin: String { model.credentials?.origin ?? "—" }
    private var host: String { URL(string: origin)?.host ?? "—" }
    private var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "—"
    }
    private var walkthroughBottom: Bool {
#if DEBUG
        ProcessInfo.processInfo.arguments.contains("--garive-walkthrough-settings-bottom")
#else
        false
#endif
    }
    private var walkthroughUnpairConfirmation: Bool {
#if DEBUG
        ProcessInfo.processInfo.arguments.contains("--garive-walkthrough-unpair-confirmation")
#else
        false
#endif
    }

    var body: some View {
        ScrollViewReader { proxy in
            List {
                Section("Connection") {
                    LabeledContent("Status") { StatusBadge(status: state.connection.name.lowercased()) }
                    LabeledContent("Service", value: origin)
                    LabeledContent("Verified host", value: host)
                    Label("Access grant protected by Keychain", systemImage: "lock.shield")
                }
                Section("Notifications") {
                    Label("Notification previews hide agent output", systemImage: "bell.slash")
                    Button("Open notification settings") {
#if os(iOS)
                        guard let url = URL(string: UIApplication.openSettingsURLString) else { return }
                        UIApplication.shared.open(url)
#endif
                    }
                }
                Section("Appearance") {
                    Picker("Theme", selection: $theme) {
                        Text("System").tag("system")
                        Text("Light").tag("light")
                        Text("Dark").tag("dark")
                    }
                    .pickerStyle(.segmented)
                }
                Section("Privacy") {
                    Label("Durable work remains on your service", systemImage: "externaldrive")
                }
                Section("Diagnostics") {
#if os(iOS)
                    LabeledContent("Device", value: UIDevice.current.name)
                    LabeledContent("iOS", value: UIDevice.current.systemVersion)
#endif
                    LabeledContent("Garive", value: version)
                    LabeledContent("Connection", value: state.connection.name.lowercased())
                    Button(diagnosticsCopied ? "Diagnostics copied" : "Copy safe diagnostics") {
#if os(iOS)
                        UIPasteboard.general.string = diagnostics
#endif
                        diagnosticsCopied = true
                    }
                }
                Section {
                    Button("Unpair this device", role: .destructive) { confirmUnpair = true }
                }
                .id("unpair")
            }
            .scrollContentBackground(.hidden).background(GarivePalette.ink).navigationTitle("Settings")
            .onAppear {
                guard walkthroughBottom else { return }
                DispatchQueue.main.async { proxy.scrollTo("unpair", anchor: .bottom) }
            }
            .onAppear {
                if walkthroughUnpairConfirmation { confirmUnpair = true }
            }
            .confirmationDialog(
                "Unpair this device?",
                isPresented: $confirmUnpair,
                titleVisibility: .visible
            ) {
                Button("Unpair device", role: .destructive) { model.signOut() }
                Button("Keep paired", role: .cancel) {}
            } message: {
                Text("This removes access from this phone. Agent work and history remain on your service.")
            }
        }
    }

    private var diagnostics: String {
#if os(iOS)
        return "Garive \(version)\niOS \(UIDevice.current.systemVersion)\nConnection \(state.connection.name.lowercased())"
#else
        return "Garive \(version)\nConnection \(state.connection.name.lowercased())"
#endif
    }
}

private struct SessionSection: View {
    let title: String
    let subtitle: String
    let sessions: [MobileSessionCard]
    @ObservedObject var model: MobileViewModel

    var body: some View {
        if !sessions.isEmpty {
            VStack(alignment: .leading, spacing: 12) {
                Text(title).font(.title2.bold())
                Text(subtitle).font(.subheadline).foregroundStyle(.secondary)
                ForEach(sessions, id: \.sessionId) { session in
                    SessionRow(session: session) { model.open(session.sessionId) }
                }
            }
        }
    }
}

struct SessionRow: View {
    let session: MobileSessionCard
    let open: () -> Void
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        Button(action: open) {
            Group {
                if dynamicTypeSize.isAccessibilitySize {
                    VStack(alignment: .leading, spacing: 14) {
                        HStack {
                            statusMark
                            Spacer()
                            disclosure
                        }
                        sessionIdentity
                        StatusBadge(status: session.status.name.lowercased())
                    }
                } else {
                    HStack(alignment: .top, spacing: 14) {
                        statusMark
                        VStack(alignment: .leading, spacing: 9) {
                            sessionIdentity
                            StatusBadge(status: session.status.name.lowercased())
                        }
                        Spacer(minLength: 8)
                        disclosure.padding(.top, 15)
                    }
                }
            }
            .padding(16).background(GarivePalette.panel, in: RoundedRectangle(cornerRadius: 18))
        }.buttonStyle(.plain)
    }

    private var statusMark: some View {
        Circle().fill(statusColor.opacity(0.18)).frame(width: 44, height: 44)
            .overlay(Image(systemName: statusIcon).foregroundStyle(statusColor))
            .accessibilityHidden(true)
    }

    private var sessionIdentity: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(session.agentName).font(.headline).foregroundStyle(.primary)
                .fixedSize(horizontal: false, vertical: true)
            Text("\(session.turnCount) turns · \(shortID)").font(.caption).foregroundStyle(.secondary)
                .lineLimit(2).fixedSize(horizontal: false, vertical: true)
        }
    }

    private var disclosure: some View {
        Image(systemName: "chevron.right").font(.caption).foregroundStyle(.tertiary)
            .accessibilityHidden(true)
    }

    private var shortID: String { String(session.sessionId.prefix(8)) }
    private var statusIcon: String { session.status.name == "NEEDS_INPUT" ? "hand.raised.fill" : "bolt.fill" }
    private var statusColor: Color { session.status.name == "NEEDS_INPUT" ? GarivePalette.amber : GarivePalette.mint }
}

struct StatusBadge: View {
    let status: String
    var body: some View {
        Text(status.replacingOccurrences(of: "_", with: " "))
            .font(.caption2.weight(.semibold)).textCase(.uppercase).lineLimit(1)
            .fixedSize(horizontal: true, vertical: false).padding(.horizontal, 9).padding(.vertical, 5)
            .background(color.opacity(0.14), in: Capsule()).foregroundStyle(color)
    }
    private var color: Color {
        status.contains("input") ? GarivePalette.amber : status.contains("fail") ? GarivePalette.coral : GarivePalette.mint
    }
}

private struct ConnectionBanner: View {
    let state: MobileWorkState
    var body: some View {
        HStack {
            Circle().fill(state.connection.name == "ONLINE" ? GarivePalette.mint : GarivePalette.amber).frame(width: 9, height: 9)
            Text(state.connection.name == "ONLINE" ? "Connected · server work continues" : "Reconnecting · showing durable state")
                .font(.subheadline.weight(.medium))
            Spacer()
        }.padding(13).background(GarivePalette.raised, in: RoundedRectangle(cornerRadius: 14))
    }
}

private struct EmptyWorkView: View {
    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "sparkles.rectangle.stack").font(.system(size: 42)).foregroundStyle(GarivePalette.coral)
            Text("Start work from anywhere").font(.title3.bold())
            Text("Choose an agent, give it a goal, then leave the work running on your service.")
                .multilineTextAlignment(.center).foregroundStyle(.secondary)
        }.frame(maxWidth: .infinity).padding(.vertical, 46)
    }
}
#endif
