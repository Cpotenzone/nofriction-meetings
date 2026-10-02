import Foundation

/// Decides when a recording has outlived its meeting. Pure logic: the
/// caller feeds it real (post-filter) speech and input levels, calls
/// `tick()` about once a second, and acts on what it returns. Time comes
/// from an injectable clock so tests can drive it.
///
/// Signals (any one starts a countdown):
/// - calendar: the matched event ended more than 2 min ago and nobody has
///   said anything real for 60 s;
/// - silence: no real speech for N minutes (default 3) and the input level
///   stayed below the speech threshold for those N minutes;
/// - long silence: no real speech for 2 × N minutes whatever the level
///   (a TV or HVAC can keep the meter up while nobody talks).
/// Quiet audio alone never ends a meeting — a soft far-field voice can sit
/// under the threshold and still be transcribed.
///
/// A countdown (30 s) is cancelled by real speech, the user's "Keep
/// recording" (which also snoozes 10 min), pausing, or turning the setting
/// off; if nothing cancels it, `tick()` returns `.stop`.
struct MeetingEndDetector {
    struct Config: Equatable {
        var enabled = true
        var silenceMinutes: Double = 3
        /// Input level (0…1 from `AudioCapture`, −50…0 dBFS) counted as someone talking
        var speechLevel: Float = 0.25
        var calendarGrace: TimeInterval = 120
        var calendarQuiet: TimeInterval = 60
        var countdown: TimeInterval = 30
        var snooze: TimeInterval = 600

        static let enabledKey = "autoStopEnabled"
        static let minutesKey = "autoStopSilenceMinutes"
        static let minutesRange = 1...30

        static func load(_ defaults: UserDefaults = .standard) -> Config {
            var c = Config()
            if defaults.object(forKey: enabledKey) != nil { c.enabled = defaults.bool(forKey: enabledKey) }
            let minutes = defaults.integer(forKey: minutesKey)
            if minutesRange.contains(minutes) { c.silenceMinutes = Double(minutes) }
            return c
        }
    }

    enum Reason: Equatable {
        case calendarEnded
        case silence
    }

    enum Phase: Equatable {
        case listening
        case countingDown(Reason, deadline: Date)
        case snoozed(until: Date)
        case ended(Reason)
    }

    enum Action: Equatable {
        case none
        case beginCountdown(Reason, deadline: Date)
        case cancelCountdown
        case stop(Reason)
    }

    var config: Config
    /// End of the matched calendar event, if any (may arrive mid-recording)
    var scheduledEnd: Date?
    private(set) var phase: Phase = .listening
    private(set) var lastSpeechAt: Date
    private(set) var lastLoudAt: Date
    private let clock: () -> Date

    init(config: Config = Config(), scheduledEnd: Date? = nil, clock: @escaping () -> Date = Date.init) {
        self.config = config
        self.scheduledEnd = scheduledEnd
        self.clock = clock
        let now = clock()
        lastSpeechAt = now
        lastLoudAt = now
    }

    var now: Date { clock() }

    var isCountingDown: Bool { if case .countingDown = phase { true } else { false } }
    /// The meeting has been judged over (countdown running or done)
    var endDetected: Bool {
        switch phase {
        case .countingDown, .ended: true
        default: false
        }
    }

    // MARK: Inputs

    /// A kept, substantive transcript segment.
    mutating func noteSpeech() -> Action {
        lastSpeechAt = now
        lastLoudAt = max(lastLoudAt, lastSpeechAt)
        return cancelCountdown()
    }

    /// One meter sample.
    mutating func noteLevel(_ level: Float) {
        if level >= config.speechLevel { lastLoudAt = now }
    }

    /// Resuming after a pause: the silence clock starts over.
    mutating func resetIdle() -> Action {
        lastSpeechAt = now
        lastLoudAt = now
        return cancelCountdown()
    }

    // MARK: User

    /// "Keep recording": cancel any countdown and stay quiet for the snooze period.
    mutating func keepRecording() -> Action {
        let wasCounting = isCountingDown
        phase = .snoozed(until: now.addingTimeInterval(config.snooze))
        return wasCounting ? .cancelCountdown : .none
    }

    /// "Stop now"
    mutating func stopNow() -> Action {
        let reason: Reason = if case .countingDown(let r, _) = phase { r } else { .silence }
        phase = .ended(reason)
        return .stop(reason)
    }

    mutating func cancelCountdown() -> Action {
        guard isCountingDown else { return .none }
        phase = .listening
        return .cancelCountdown
    }

    // MARK: Clock

    mutating func tick() -> Action {
        let now = self.now
        guard config.enabled else { return cancelCountdown() }
        switch phase {
        case .ended:
            return .none
        case .countingDown(let reason, let deadline):
            guard now >= deadline else { return .none }
            phase = .ended(reason)
            return .stop(reason)
        case .snoozed(let until):
            guard now >= until else { return .none }
            phase = .listening
        case .listening:
            break
        }
        guard let reason = evaluate(now) else { return .none }
        let deadline = now.addingTimeInterval(config.countdown)
        phase = .countingDown(reason, deadline: deadline)
        return .beginCountdown(reason, deadline: deadline)
    }

    /// Which end signal holds right now, if any.
    func evaluate(_ now: Date) -> Reason? {
        let quietSpeech = now.timeIntervalSince(lastSpeechAt)
        let quietAudio = now.timeIntervalSince(lastLoudAt)
        let window = config.silenceMinutes * 60
        if let end = scheduledEnd, now >= end.addingTimeInterval(config.calendarGrace), quietSpeech >= config.calendarQuiet {
            return .calendarEnded
        }
        if quietSpeech >= window, quietAudio >= window { return .silence }
        if quietSpeech >= 2 * window { return .silence }
        return nil
    }
}

/// Recent input levels, to tell whether a segment was "heard" on near silence.
struct LevelHistory {
    private var samples: [(at: Date, level: Float)] = []
    var keep: TimeInterval = 180

    mutating func append(_ level: Float, at date: Date) {
        samples.append((date, level))
        if let first = samples.first, date.timeIntervalSince(first.at) > keep + 30 {
            samples.removeAll { date.timeIntervalSince($0.at) > keep }
        }
    }

    /// Loudest sample in [from, to], nil if none were recorded then.
    func peak(from: Date, to: Date) -> Float? {
        samples.lazy.filter { $0.at >= from && $0.at <= to }.map(\.level).max()
    }
}
