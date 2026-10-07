import Foundation
import UIKit
import UserNotifications

/// The background half of meeting-end detection: when the countdown starts
/// while the app isn't on screen (recording continues via the audio
/// background mode), a local notification offers the same two choices as
/// the in-app banner.
@MainActor
final class MeetingEndNotifier: NSObject, UNUserNotificationCenterDelegate {
    static let shared = MeetingEndNotifier()

    nonisolated static let category = "MEETING_END"
    nonisolated static let keepAction = "MEETING_END_KEEP"
    nonisolated static let stopAction = "MEETING_END_STOP"
    nonisolated static let requestID = "meeting-end"

    weak var session: RecordingSession?
    private var center: UNUserNotificationCenter { .current() }

    /// Call once at launch, before any notification response can arrive.
    func install() {
        center.delegate = self
        let keep = UNNotificationAction(identifier: Self.keepAction, title: "Keep recording", options: [])
        let stop = UNNotificationAction(identifier: Self.stopAction, title: "Stop now", options: [.destructive])
        let category = UNNotificationCategory(identifier: Self.category, actions: [keep, stop], intentIdentifiers: [], options: [])
        // Timed recording's "5 minutes left" (+15 min / No limit) shares this delegate
        center.setNotificationCategories([category, TimeLimitNotifier.notificationCategory])
    }

    /// Ask once, at the first recording with auto-stop on — the moment the
    /// permission is actually useful. Never re-asks after a decision.
    func requestAuthorizationIfNeeded() async {
        let settings = await center.notificationSettings()
        guard settings.authorizationStatus == .notDetermined else { return }
        _ = try? await center.requestAuthorization(options: [.alert, .sound])
    }

    var isInBackground: Bool { UIApplication.shared.applicationState != .active }

    func post(deadline: Date) {
        let seconds = max(1, Int(deadline.timeIntervalSinceNow.rounded()))
        let content = UNMutableNotificationContent()
        content.title = "This seems to have ended"
        content.body = "Stopping the recording in \(seconds) s. Everything said so far is saved."
        content.categoryIdentifier = Self.category
        content.sound = .default
        content.interruptionLevel = .active
        center.add(UNNotificationRequest(identifier: Self.requestID, content: content, trigger: nil))
    }

    func clear() {
        center.removePendingNotificationRequests(withIdentifiers: [Self.requestID])
        center.removeDeliveredNotifications(withIdentifiers: [Self.requestID])
    }

    // MARK: UNUserNotificationCenterDelegate

    nonisolated func userNotificationCenter(_: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let action = response.actionIdentifier
        if response.notification.request.content.categoryIdentifier == TimeLimitNotifier.category {
            await MainActor.run {
                guard let session = MeetingEndNotifier.shared.session else { return }
                switch action {
                case TimeLimitNotifier.extendAction: session.extendTimeLimit()
                case TimeLimitNotifier.noLimitAction: session.removeTimeLimit()
                default: break   // tapped: the app opens on the recording
                }
            }
            return
        }
        guard response.notification.request.content.categoryIdentifier == Self.category else { return }
        await MainActor.run {
            guard let session = MeetingEndNotifier.shared.session else { return }
            switch action {
            case Self.keepAction: session.keepRecording()
            case Self.stopAction: session.stopNow()
            default: break   // tapped: the app opens on the in-app banner
            }
        }
    }

    /// In the foreground the in-app banner already shows; don't double up.
    nonisolated func userNotificationCenter(_: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        notification.request.content.categoryIdentifier == Self.category ? [] : [.banner, .sound]
    }
}
