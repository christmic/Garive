#if os(iOS)
import UIKit
import UserNotifications

@MainActor
final class MobilePushInbox {
    static let shared = MobilePushInbox()
    private var registrationID: String?
    private var wakeToken: String?
    private var registrationHandler: ((String) -> Void)?
    private var wakeHandler: ((String) -> Void)?

    func attach(registration: @escaping (String) -> Void, wake: @escaping (String) -> Void) {
        registrationHandler = registration
        wakeHandler = wake
        if let registrationID { registration(registrationID) }
        if let wakeToken { wake(wakeToken); self.wakeToken = nil }
    }

    func publishRegistration(_ value: String) {
        registrationID = value
        registrationHandler?(value)
    }

    func publishWake(_ value: String) {
        if let wakeHandler { wakeHandler(value) } else { wakeToken = value }
    }
}

final class GariveAppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        let center = UNUserNotificationCenter.current()
        center.delegate = self
        center.requestAuthorization(options: [.alert, .badge]) { granted, _ in
            guard granted else { return }
            Task { @MainActor in application.registerForRemoteNotifications() }
        }
        return true
    }

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken token: Data) {
        MobilePushInbox.shared.publishRegistration(token.map { String(format: "%02x", $0) }.joined())
    }

    func application(
        _ application: UIApplication,
        didReceiveRemoteNotification userInfo: [AnyHashable: Any],
        fetchCompletionHandler completionHandler: @escaping (UIBackgroundFetchResult) -> Void
    ) {
        guard let token = WakeEnvelope.routeToken(from: userInfo) else {
            completionHandler(.noData)
            return
        }
        MobilePushInbox.shared.publishWake(token)
        completionHandler(.newData)
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse
    ) async {
        if let token = WakeEnvelope.routeToken(from: response.notification.request.content.userInfo) {
            await MainActor.run { MobilePushInbox.shared.publishWake(token) }
        }
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions { [.banner, .list] }

}
#endif
