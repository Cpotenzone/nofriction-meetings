import Foundation
import UserNotifications

// Timed recording and classes (docs/TIMED_RECORDING_AND_CLASSES.md).
// Pure logic here is unit-tested; RecordingSession owns the live timer and
// stops through its normal `stop()`.

/// "How long?": 15 / 30 / 60 / 90 minutes, or no limit.
enum RecordingLimit: Hashable, Sendable {
    case minutes(Int)
    case noLimit

    static let choices: [RecordingLimit] = [.minutes(15), .minutes(30), .minutes(60), .minutes(90), .noLimit]
    /// "+15 min"
    static let extendMinutes = 15
    /// Longest plan (and longest extension total), 12 hours
    static let maxMinutes = 12 * 60

    var minutes: Int? {
        if case .minutes(let m) = self { return m }
        return nil
    }

    /// UserDefaults value: "15" … "90", "none"
    var storageValue: String { minutes.map(String.init) ?? "none" }

    init?(storageValue: String) {
        let v = storageValue.trimmingCharacters(in: .whitespaces).lowercased()
        if v == "none" { self = .noLimit; return }
        guard let m = Int(v), (1...Self.maxMinutes).contains(m) else { return nil }
        self = .minutes(m)
    }

    /// Short label: "15" / "∞"
    var shortLabel: String { minutes.map(String.init) ?? "∞" }
    /// "15 min" / "No limit"
    var label: String { minutes.map { "\($0) min" } ?? "No limit" }
    var spoken: String { minutes.map { "\($0) minutes" } ?? "No limit" }
}

/// The remembered choice (preselected in the sheet; used by starts without it).
enum RecordingLimitStore {
    static let key = "recordingDefaultLength"

    /// Never chosen (or unreadable): no limit, so nothing is cut short unexpectedly.
    static func remembered(_ defaults: UserDefaults = .standard) -> RecordingLimit {
        guard let raw = defaults.string(forKey: key), let limit = RecordingLimit(storageValue: raw),
              RecordingLimit.choices.contains(limit) else { return .noLimit }
        return limit
    }

    static func remember(_ limit: RecordingLimit, _ defaults: UserDefaults = .standard) {
        guard RecordingLimit.choices.contains(limit) else { return }
        defaults.set(limit.storageValue, forKey: key)
    }
}

/// The deadline of one recording. Wall-clock: pausing doesn't move it.
struct TimeLimitPlan: Equatable, Sendable {
    let startedAt: Date
    private(set) var deadline: Date?
    /// The warning for the current deadline was given
    private(set) var warned = false
    /// The deadline passed and a stop was requested
    private(set) var stopping = false

    enum Action: Equatable { case none, warn(secondsLeft: Int), stop }

    init(startedAt: Date, limit: RecordingLimit) {
        self.startedAt = startedAt
        self.deadline = limit.minutes.map { startedAt.addingTimeInterval(TimeInterval($0 * 60)) }
    }

    /// Whole minutes, rounded up; nil without a limit.
    var plannedMinutes: Int? {
        deadline.map { Int((max(0, $0.timeIntervalSince(startedAt)) / 60).rounded(.up)) }
    }

    /// 2 minutes ahead for plans of 15 minutes or less, otherwise 5.
    static func warnLead(plannedMinutes: Int) -> TimeInterval {
        plannedMinutes <= 15 ? 120 : 300
    }

    var warnAt: Date? {
        guard let deadline, let planned = plannedMinutes else { return nil }
        return deadline.addingTimeInterval(-Self.warnLead(plannedMinutes: planned))
    }

    func remaining(at now: Date) -> TimeInterval? {
        deadline.map { max(0, $0.timeIntervalSince(now)) }
    }

    mutating func tick(now: Date) -> Action {
        guard let deadline, !stopping else { return .none }
        if now >= deadline {
            stopping = true
            return .stop
        }
        if !warned, let warnAt, now >= warnAt {
            warned = true
            return .warn(secondsLeft: Int(deadline.timeIntervalSince(now).rounded(.up)))
        }
        return .none
    }

    /// "+15 min": from the deadline, or from now if it already passed. Re-arms the warning.
    @discardableResult
    mutating func extend(by minutes: Int = RecordingLimit.extendMinutes, now: Date) -> Bool {
        guard let deadline else { return false }
        let cap = startedAt.addingTimeInterval(TimeInterval(RecordingLimit.maxMinutes * 60))
        let next = min(max(deadline, now).addingTimeInterval(TimeInterval(max(1, minutes) * 60)), cap)
        guard next > deadline else { return false }
        self.deadline = next
        stopping = false
        warned = false
        if let warnAt, now >= warnAt { warned = true }
        return true
    }

    /// "No limit"
    mutating func removeLimit() {
        deadline = nil
        warned = false
        stopping = false
    }
}

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

/// Class names: cleaned, matched ignoring case, recent ones from saved meetings.
enum ClassNames {
    static let maxLength = 80
    /// One-time notice, shown the first time a class is recorded
    static let noticeShownKey = "classRecordingNoticeShown"
    static let notice = "Many schools require the instructor's permission to record a class, and some require classmates' consent. Check your school's policy."

    /// Trimmed, whitespace collapsed, control characters dropped, ≤ 80 characters. Empty → nil.
    static func normalize(_ input: String?) -> String? {
        guard let input else { return nil }
        let words = input.unicodeScalars
            .filter { !CharacterSet.controlCharacters.subtracting(.whitespacesAndNewlines).contains($0) }
            .map(String.init).joined()
            .split(whereSeparator: { $0.isWhitespace })
        let joined = String(words.joined(separator: " ").prefix(maxLength))
        let trimmed = joined.trimmingCharacters(in: .whitespaces)
        return trimmed.isEmpty ? nil : trimmed
    }

    /// An existing class with the same name ignoring case wins ("bio 101" → "BIO 101").
    static func canonical(_ input: String?, existing: [String]) -> String? {
        guard let name = normalize(input) else { return nil }
        return existing.first { $0.caseInsensitiveCompare(name) == .orderedSame } ?? name
    }

    /// Most recently recorded first, one per name ignoring case. Derived from
    /// the meetings themselves, so deleting a meeting removes its class here.
    static func recent(_ meetings: [(className: String?, startedAt: Date)], limit: Int = 12) -> [String] {
        var seen = Set<String>()
        var out: [String] = []
        for m in meetings.sorted(by: { $0.startedAt > $1.startedAt }) {
            guard let c = normalize(m.className), seen.insert(c.lowercased()).inserted else { continue }
            out.append(c)
            if out.count == limit { break }
        }
        return out
    }

    /// Chips while typing: recents when empty, prefix matches first, then contains.
    static func suggestions(_ input: String, recents: [String], max: Int = 6) -> [String] {
        guard let q = normalize(input)?.lowercased() else { return Array(recents.prefix(max)) }
        let starts = recents.filter { $0.lowercased().hasPrefix(q) }
        let contains = recents.filter { !$0.lowercased().hasPrefix(q) && $0.lowercased().contains(q) }
        return Array((starts + contains).prefix(max))
    }

    static func matches(_ className: String?, filter: String?) -> Bool {
        guard let filter else { return true }
        return className?.caseInsensitiveCompare(filter) == .orderedSame
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
