#if GARIVE_SHARED_AVAILABLE
import SwiftUI
#if os(iOS)
import UIKit
#else
import AppKit
#endif
@preconcurrency import GariveShared

struct GariveRootView: View {
    @StateObject private var model = MobileViewModel()
    @AppStorage("garive.theme") private var theme = "system"
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        ZStack {
            if privacyCovered {
                MobilePrivacyShield().transition(.opacity).zIndex(10)
            } else {
                Group {
                    if model.credentials == nil {
                        PairingView(
                            errorCode: model.errorCode,
                            pairing: model.pairing,
                            suggestion: model.pairingSuggestion
                        ) {
                            model.pair(origin: $0, accessGrant: $1)
                        }
                    } else if let state = model.state {
                        RemoteWorkspaceView(model: model, state: state, theme: $theme)
                    } else {
                        LoadingView(errorCode: model.errorCode, retry: model.refresh)
                    }
                }
            }
        }
        .tint(GarivePalette.coral)
        .preferredColorScheme(theme == "dark" ? .dark : theme == "light" ? .light : nil)
        .onAppear { model.setTheme(theme) }
        .onChange(of: theme) { _, value in model.setTheme(value) }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active, model.state != nil { model.refresh() }
        }
        .onOpenURL { model.acceptPairingURL($0) }
    }

    private var privacyCovered: Bool {
        guard model.credentials != nil else { return false }
#if DEBUG
        if ProcessInfo.processInfo.arguments.contains("--garive-walkthrough-privacy-shield") { return true }
#endif
        return scenePhase != .active
    }
}

private struct MobilePrivacyShield: View {
    var body: some View {
        VStack(spacing: 18) {
            Image(systemName: "lock.shield.fill")
                .font(.system(size: 34, weight: .semibold))
                .foregroundStyle(GarivePalette.coral)
                .frame(width: 72, height: 72)
                .background(GarivePalette.coral.opacity(0.12), in: RoundedRectangle(cornerRadius: 22))
            Text("Remote work is private").font(.title2.bold())
            Text("Return to Garive to view your Agent activity.")
                .font(.body).foregroundStyle(.secondary).multilineTextAlignment(.center)
        }
        .padding(32).frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(GarivePalette.ink.ignoresSafeArea())
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Remote work hidden while Garive is inactive")
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
    @Binding var theme: String
    @State private var sidebarPresented = false
    @State private var confirmingAbandonRetry = false

    var body: some View {
        GeometryReader { geometry in
            let wide = geometry.size.width >= 700
            ZStack(alignment: .leading) {
                HStack(spacing: 0) {
                    if wide {
                        RemoteSidebar(model: model, state: state, host: remoteHost, close: {})
                            .frame(width: 300)
                    }
                    NavigationStack {
                        destinationView
                            .safeAreaInset(edge: .top, spacing: 0) {
                                if state.destination != .conversation,
                                   let code = state.noticeCode {
                                    MobileNoticeBanner(
                                        code: code,
                                        pending: state.pendingCommand != nil,
                                        dismiss: model.dismissNotice,
                                        retry: model.retryExact,
                                        abandon: { confirmingAbandonRetry = true }
                                    )
                                }
                            }
                            .toolbar {
                                if !wide {
                                    ToolbarItem(placement: .navigation) {
                                        Button { withAnimation(.snappy) { sidebarPresented = true } } label: {
                                            Image(systemName: "line.3.horizontal")
                                        }.accessibilityLabel("Open navigation")
                                    }
                                }
                            }
                    }
                }
                if sidebarPresented && !wide {
                    Color.black.opacity(0.42).ignoresSafeArea()
                        .onTapGesture { withAnimation(.snappy) { sidebarPresented = false } }
                        .transition(.opacity)
                    RemoteSidebar(
                        model: model,
                        state: state,
                        host: remoteHost,
                        close: { withAnimation(.snappy) { sidebarPresented = false } }
                    )
                    .frame(width: min(geometry.size.width * 0.86, 360))
                    .transition(.move(edge: .leading))
                }
            }
        }
        .onAppear {
#if DEBUG
            if ProcessInfo.processInfo.arguments.contains("--garive-walkthrough-sidebar") {
                sidebarPresented = true
            }
#endif
        }
        .sheet(isPresented: $model.presentingNewTask) {
            NewTaskView(model: model, agents: state.agents)
        }
        .confirmationDialog(
            "Forget exact retry?",
            isPresented: $confirmingAbandonRetry,
            titleVisibility: .visible
        ) {
            Button("Forget retry", role: .destructive, action: model.abandonPending)
            Button("Keep retry", role: .cancel) {}
        } message: {
            Text("The server may already have accepted this command. Verify history before replacing the work.")
        }
        .sheet(isPresented: Binding(
            get: { state.destination.name == "CONVERSATION" },
            set: { if !$0 { model.select(.work) } }
        )) {
            NavigationStack { ConversationView(model: model, state: state) }
        }
    }

    @ViewBuilder private var destinationView: some View {
        switch state.destination.name {
        case "SESSIONS": SessionsView(model: model, state: state)
        case "AGENTS": AgentsView(model: model, state: state)
        case "SETTINGS": SettingsView(model: model, state: state, theme: $theme)
        default: WorkView(model: model, state: state)
        }
    }

    private var remoteHost: String {
        guard let origin = model.credentials?.origin, let host = URL(string: origin)?.host else { return "service" }
        return URL(string: origin)?.port.map { "\(host):\($0)" } ?? host
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

private struct RemoteSidebar: View {
    @ObservedObject var model: MobileViewModel
    let state: MobileWorkState
    let host: String
    let close: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Garive").font(.title2.bold()).padding(.top, 24)
            HStack(spacing: 7) {
                Circle().fill(GarivePalette.mint).frame(width: 7, height: 7)
                Text("Remote · \(host)").font(.subheadline).foregroundStyle(.secondary)
            }.padding(.bottom, 14)
            destinationButton("Work", icon: "sparkles", destination: .work)
            destinationButton("Sessions", icon: "rectangle.stack", destination: .sessions)
            destinationButton("Agents", icon: "cpu", destination: .agents)
            destinationButton("Settings", icon: "gearshape", destination: .settings)
            Text("Recent").font(.headline).padding(.top, 24).padding(.bottom, 5)
            ForEach(Array(state.sessions.prefix(5)), id: \.sessionId) { session in
                Button {
                    model.open(session.sessionId)
                    close()
                } label: {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(session.agentName).lineLimit(1).foregroundStyle(.primary)
                        Text(session.status.name.lowercased().replacingOccurrences(of: "_", with: " "))
                            .font(.caption).foregroundStyle(session.status == .needsInput ? GarivePalette.amber : .secondary)
                    }.frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 8)
                }.buttonStyle(.plain)
            }
            Spacer()
            Button { model.showNewTask(); close() } label: {
                Label("New task", systemImage: "square.and.pencil")
                    .font(.headline).frame(maxWidth: .infinity).padding(.vertical, 12)
            }.buttonStyle(.borderedProminent).buttonBorderShape(.capsule)
        }
        .padding(.horizontal, 20).padding(.bottom, 18)
        .frame(maxHeight: .infinity)
        .background(GarivePalette.ink.ignoresSafeArea())
        .shadow(color: .black.opacity(0.28), radius: 24, x: 10)
    }

    private func destinationButton(_ title: String, icon: String, destination: MobileDestination) -> some View {
        Button {
            model.select(destination)
            close()
        } label: {
            Label(title, systemImage: icon).font(.body.weight(.medium))
                .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 14).frame(height: 48)
                .background(
                    state.destination == destination ? GarivePalette.raised : Color.clear,
                    in: RoundedRectangle(cornerRadius: 13)
                )
        }.buttonStyle(.plain)
    }
}

enum GarivePalette {
#if os(iOS)
    static let ink = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark ? .black : UIColor(red: 0.984, green: 0.980, blue: 0.965, alpha: 1)
    })
    static let panel = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark ? UIColor(white: 0.07, alpha: 1) : UIColor(red: 1, green: 0.996, blue: 0.984, alpha: 1)
    })
    static let raised = Color(uiColor: UIColor { traits in
        traits.userInterfaceStyle == .dark ? UIColor(white: 0.12, alpha: 1) : UIColor(red: 0.941, green: 0.933, blue: 0.906, alpha: 1)
    })
#else
    static let ink = Color(nsColor: .windowBackgroundColor)
    static let panel = Color(nsColor: .controlBackgroundColor)
    static let raised = Color(nsColor: .underPageBackgroundColor)
#endif
    static let coral = Color(red: 0.19, green: 0.37, blue: 0.81)
    static let mint = Color(red: 0.15, green: 0.51, blue: 0.35)
    static let amber = Color(red: 0.64, green: 0.41, blue: 0.09)
}

enum GariveMobileMetrics {
    static let composerRadius: CGFloat = 24
    static let userPromptRadius: CGFloat = 22
    static let decisionRadius: CGFloat = 20
    static let touchTarget: CGFloat = 44
    static let attentionEdge: CGFloat = 2
}
#endif
