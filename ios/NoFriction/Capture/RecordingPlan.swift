import Foundation
import UserNotifications

// Timed recording (docs/TIMED_RECORDING_AND_CLASSES.md): the iPhone's
// per-meeting slot and warning notification. The choices and the deadline
// logic are shared with Apple Watch (ios/Shared/TimeLimit.swift); the
// vocabulary (type, notebook, markers) is in ios/Shared/RecordingVocabulary.swift.
// RecordingSession owns the live timer and stops through its normal `stop()`.

/// The plan of the recording in progress, keyed to its meeting: a timer
/// armed for one meeting can never act on another.
struct TimeLimitSlot: Equatable {
    private(set) var meetingID: UUID?
    private(set) var plan: TimeLimitPlan?

    mutating func arm(meetingID: UUID, plan: TimeLimitPlan) {
        self.meetingID = meetingID
        self.plan = plan
    }

    mutating func clear() {
        meetingID = nil
        plan = nil
    }

    /// `.none` unless `meetingID` is the armed meeting.
    mutating func tick(meetingID: UUID, now: Date) -> TimeLimitPlan.Action {
        guard self.meetingID == meetingID, var p = plan else { return .none }
        let action = p.tick(now: now)
        plan = p
        return action
    }

    @discardableResult
    mutating func extend(meetingID: UUID, now: Date) -> Bool {
        guard self.meetingID == meetingID, var p = plan else { return false }
        let ok = p.extend(now: now)
        plan = p
        return ok
    }

    @discardableResult
    mutating func removeLimit(meetingID: UUID) -> Bool {
        guard self.meetingID == meetingID, var p = plan else { return false }
        p.removeLimit()
        plan = p
        return true
    }
}

/// The "5 minutes left" notification with +15 min / No limit. Scheduled
/// ahead (so it fires even if the app is suspended), only when notification
/// permission is already granted. RecordingSession asks in context, at the
/// first timed recording, never at launch.
@MainActor
enum TimeLimitNotifier {
    nonisolated static let category = "TIME_LIMIT"
    nonisolated static let extendAction = "TIME_LIMIT_EXTEND"
    nonisolated static let noLimitAction = "TIME_LIMIT_REMOVE"
    nonisolated static let requestID = "time-limit-warning"

    nonisolated static var notificationCategory: UNNotificationCategory {
        let extend = UNNotificationAction(identifier: extendAction, title: "+15 min", options: [])
        let noLimit = UNNotificationAction(identifier: noLimitAction, title: "No limit", options: [])
        return UNNotificationCategory(identifier: category, actions: [extend, noLimit], intentIdentifiers: [], options: [])
    }

    nonisolated static func title(secondsLeft: Int) -> String {
        let minutes = max(1, Int((Double(secondsLeft) / 60).rounded(.up)))
        return minutes == 1 ? "1 minute left in this recording" : "\(minutes) minutes left in this recording"
    }

    /// Ask only if never asked (the first timed recording). No-op otherwise.
    static func requestAuthorizationIfNeeded() async {
        let center = UNUserNotificationCenter.current()
        let settings = await center.notificationSettings()
        guard settings.authorizationStatus == .notDetermined else { return }
        _ = try? await center.requestAuthorization(options: [.alert, .sound])
    }

    /// (Re)schedule the warning for `plan`, if permitted. Cancels any earlier one.
    static func schedule(for plan: TimeLimitPlan, title meetingTitle: String) async {
        cancel()
        guard let warnAt = plan.warnAt, let deadline = plan.deadline else { return }
        let center = UNUserNotificationCenter.current()
        let status = await center.notificationSettings().authorizationStatus
        guard status == .authorized || status == .provisional || status == .ephemeral else { return }
        let content = UNMutableNotificationContent()
        content.title = title(secondsLeft: Int(deadline.timeIntervalSince(warnAt)))
        content.body = "\(meetingTitle) stops at \(deadline.formatted(date: .omitted, time: .shortened)). Add 15 minutes or remove the limit."
        content.categoryIdentifier = category
        content.sound = .default
        content.interruptionLevel = .active
        let delay = max(1, warnAt.timeIntervalSinceNow)
        let trigger = UNTimeIntervalNotificationTrigger(timeInterval: delay, repeats: false)
        try? await center.add(UNNotificationRequest(identifier: requestID, content: content, trigger: trigger))
    }

    static func cancel() {
        let center = UNUserNotificationCenter.current()
        center.removePendingNotificationRequests(withIdentifiers: [requestID])
        center.removeDeliveredNotifications(withIdentifiers: [requestID])
    }
}
