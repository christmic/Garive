import SwiftUI

@main
struct GariveIOSApp: App {
#if os(iOS)
    @UIApplicationDelegateAdaptor(GariveAppDelegate.self) private var appDelegate
#endif

    var body: some Scene {
        WindowGroup {
#if GARIVE_SHARED_AVAILABLE
            GariveRootView()
#else
            ContentUnavailableView(
                "Shared framework missing",
                systemImage: "shippingbox",
                description: Text("Build GariveShared.xcframework before launching the app.")
            )
#endif
        }
    }
}
