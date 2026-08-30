#if canImport(GariveShared)
import SwiftUI
@preconcurrency import GariveShared

struct GariveRootView: View {
    @StateObject private var model = MobileViewModel()

    var body: some View {
        Group {
            if model.credentials == nil {
                PairingView(errorCode: model.errorCode, pairing: model.pairing) {
                    model.pair(origin: $0, accessGrant: $1)
                }
            } else if let state = model.state {
                RemoteWorkspaceView(model: model, state: state)
            } else {
                LoadingView(errorCode: model.errorCode, retry: model.refresh)
            }
        }
        .tint(GarivePalette.coral)
        .preferredColorScheme(.dark)
    }
}

private struct LoadingView: View {
    let errorCode: String?
    let retry: () -> Void

    var body: some View {
        VStack(spacing: 18) {
            if let errorCode {
                Image(systemName: "wifi.exclamationmark").font(.system(size: 38))
                Text("Couldn’t reach your workspace").font(.title2.bold())
                Text(errorCode.replacingOccurrences(of: "_", with: " "))
                    .foregroundStyle(.secondary)
                Button("Try again", action: retry).buttonStyle(.borderedProminent)
            } else {
                ProgressView().controlSize(.large)
                Text("Connecting securely…").foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(GarivePalette.ink)
    }
}

private struct RemoteWorkspaceView: View {
    @ObservedObject var model: MobileViewModel
    let state: MobileWorkState

    var body: some View {
        TabView(selection: Binding(
            get: { state.destination.name },
            set: { model.select(destination(for: $0)) }
        )) {
            NavigationStack { WorkView(model: model, state: state) }
                .tabItem { Label("Work", systemImage: "sparkles") }.tag("WORK")
            NavigationStack { SessionsView(model: model, state: state) }
                .tabItem { Label("Sessions", systemImage: "rectangle.stack") }.tag("SESSIONS")
            NavigationStack { AgentsView(model: model, state: state) }
                .tabItem { Label("Agents", systemImage: "cpu") }.tag("AGENTS")
            NavigationStack { SettingsView(model: model, state: state) }
                .tabItem { Label("Settings", systemImage: "gearshape") }.tag("SETTINGS")
        }
        .sheet(isPresented: $model.presentingNewTask) {
            NewTaskView(model: model, agents: state.agents)
        }
        .sheet(isPresented: Binding(
            get: { state.destination.name == "CONVERSATION" },
            set: { if !$0 { model.select(.work) } }
        )) {
            NavigationStack { ConversationView(model: model, state: state) }
        }
    }

    private func destination(for name: String) -> MobileDestination {
        switch name {
        case "SESSIONS": .sessions
        case "AGENTS": .agents
        case "SETTINGS": .settings
        default: .work
        }
    }
}

enum GarivePalette {
    static let ink = Color(red: 0.035, green: 0.043, blue: 0.055)
    static let panel = Color(red: 0.075, green: 0.086, blue: 0.105)
    static let raised = Color(red: 0.11, green: 0.125, blue: 0.15)
    static let coral = Color(red: 1.0, green: 0.39, blue: 0.30)
    static let mint = Color(red: 0.31, green: 0.84, blue: 0.66)
    static let amber = Color(red: 1.0, green: 0.72, blue: 0.30)
}
#endif
