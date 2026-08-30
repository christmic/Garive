import SwiftUI

struct PairingView: View {
    @State private var origin = ""
    @State private var accessCode = ""
    @State private var showingCode = false
    let connect: (String, String) -> Void

    private var valid: Bool {
        origin.lowercased().hasPrefix("https://") && !accessCode.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                Spacer(minLength: 36)
                ZStack {
                    RoundedRectangle(cornerRadius: 22).fill(GarivePalette.coral.gradient)
                    Image(systemName: "point.3.connected.trianglepath.dotted")
                        .font(.system(size: 34, weight: .semibold)).foregroundStyle(.white)
                }.frame(width: 72, height: 72)

                VStack(alignment: .leading, spacing: 10) {
                    Text("Your agents, wherever you are").font(.system(.largeTitle, design: .rounded).bold())
                    Text("Securely steer work running on your Garive service, review progress, and answer decisions without carrying your computer.")
                        .font(.title3).foregroundStyle(.secondary).lineSpacing(4)
                }

                VStack(spacing: 16) {
                    field("Service address", icon: "network", text: $origin)
                    HStack {
                        Image(systemName: "key.horizontal").foregroundStyle(.secondary)
                        Group {
                            if showingCode { TextField("One-time access code", text: $accessCode) }
                            else { SecureField("One-time access code", text: $accessCode) }
                        }
                        Button { showingCode.toggle() } label: {
                            Image(systemName: showingCode ? "eye.slash" : "eye")
                        }.accessibilityLabel(showingCode ? "Hide access code" : "Show access code")
                    }
                    .padding(16).background(GarivePalette.raised, in: RoundedRectangle(cornerRadius: 16))
                }

                Button { connect(origin, accessCode) } label: {
                    Label("Connect securely", systemImage: "lock.shield")
                        .font(.headline).frame(maxWidth: .infinity).padding(.vertical, 7)
                }
                .buttonStyle(.borderedProminent).controlSize(.large).disabled(!valid)

                Label("The access grant stays in this device’s Keychain. Garive never stores agent output in notifications.", systemImage: "checkmark.shield")
                    .font(.footnote).foregroundStyle(.secondary)
                Spacer(minLength: 24)
            }
            .frame(maxWidth: 560, alignment: .leading).padding(28).frame(maxWidth: .infinity)
        }.background(GarivePalette.ink)
    }

    private func field(_ title: String, icon: String, text: Binding<String>) -> some View {
        HStack {
            Image(systemName: icon).foregroundStyle(.secondary)
            TextField(title, text: text)
        }.padding(16).background(GarivePalette.raised, in: RoundedRectangle(cornerRadius: 16))
    }
}
