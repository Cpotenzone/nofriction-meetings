import Foundation

// "How long?" (docs/TIMED_RECORDING_AND_CLASSES.md): the choices, the
// remembered one, and the wall-clock deadline of one recording. Shared by
// the iPhone app (RecordingSession) and the Apple Watch app
// (RecorderStateMachine), so both follow the same rules. Pure logic,
// unit-tested on both.

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
