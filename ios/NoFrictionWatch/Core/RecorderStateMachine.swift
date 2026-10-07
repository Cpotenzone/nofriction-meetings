import Foundation

/// The watch recorder's states and time bookkeeping, with no audio in it, so
/// it can be tested on its own (NoFrictionWatchTests).
///
///     idle ──start──▶ recording ◀──resume── paused(user | interruption)
///                        │  └──────pause / interrupted──────▲
///                        └──────────────stop───────────────▶ finished
///
/// `finished` carries the metadata the phone needs: wall-clock start/end,
/// seconds of audio, and where the pauses were (so file time maps back to
/// wall-clock time on the phone), plus what the recording is, its notebook,
/// its planned length and the moments marked.
///
/// "How long?" (docs/TIMED_RECORDING_AND_NOTEBOOKS.md) uses the same
/// `TimeLimitPlan` as the iPhone: a wall-clock deadline from the start that
/// pausing doesn't move, a warning 5 minutes before (2 for a plan of 15
/// minutes or less), +15 min and No limit. `tickLimit` returns `.stop` at
/// the deadline; the recorder then stops through its normal `stop()`.
struct RecorderStateMachine: Equatable {
    enum PauseReason: Equatable {
        /// The user tapped Pause
        case user
        /// A call, Siri or another app took the microphone. watchOS only
        /// lets a recording resume from the foreground, by the user.
        case interruption
    }

    enum Phase: Equatable {
        case idle
        case recording
        case paused(PauseReason)
        case finished
    }

    enum TransitionError: Error, Equatable {
        case notAllowed(from: Phase, event: String)
    }

    private(set) var phase: Phase = .idle
    private(set) var recordingID: UUID?
    private(set) var startedAt: Date?
    private(set) var endedAt: Date?
    private(set) var pauses: [WatchRecordingMetadata.Pause] = []
    /// "What is it?" (Meeting / Class / Personal)
    private(set) var kind: RecordingKind = .default
    /// The notebook picked when starting; nil = none
    private(set) var notebook: String?
    /// "How long?"; nil while idle
    private(set) var limit: TimeLimitPlan?
    /// Moments marked, in order
    private(set) var markers: [WatchMarker] = []
    /// Wall-clock time spent paused, closed pauses only
    private var pausedTotal: TimeInterval = 0
    private var pausedAt: Date?

    /// A second tap of the same kind this soon after a mark is the same mark
    /// (a double tap on the screen, a bounce)
    static let markDebounce: TimeInterval = 0.8

    var isActive: Bool {
        switch phase {
        case .recording, .paused: return true
        case .idle, .finished: return false
        }
    }

    var isPaused: Bool {
        if case .paused = phase { return true }
        return false
    }

    /// Seconds of audio recorded by `now` (pauses excluded).
    func elapsed(at now: Date) -> TimeInterval {
        guard let startedAt else { return 0 }
        let end = endedAt ?? now
        let openPause = pausedAt.map { max(0, end.timeIntervalSince($0)) } ?? 0
        return max(0, end.timeIntervalSince(startedAt) - pausedTotal - openPause)
    }

    mutating func start(id: UUID, at now: Date, kind: RecordingKind = .default, notebook: String? = nil,
                        limit: RecordingLimit = .noLimit) throws {
        guard phase == .idle || phase == .finished else { throw TransitionError.notAllowed(from: phase, event: "start") }
        self = RecorderStateMachine()
        phase = .recording
        recordingID = id
        startedAt = now
        self.kind = kind
        self.notebook = Notebook.normalize(notebook)
        self.limit = TimeLimitPlan(startedAt: now, limit: limit)
    }

    mutating func pause(at now: Date, reason: PauseReason = .user) throws {
        switch phase {
        case .recording:
            pausedAt = now
            pauses.append(.init(at: elapsed(at: now), length: 0))
            phase = .paused(reason)
        case .paused where reason == .interruption:
            // Interrupted while already paused: nothing more to record, but
            // the UI must say why it can't simply carry on
            phase = .paused(.interruption)
        default:
            throw TransitionError.notAllowed(from: phase, event: "pause")
        }
    }

    mutating func resume(at now: Date) throws {
        guard isPaused, let pausedAt else { throw TransitionError.notAllowed(from: phase, event: "resume") }
        let length = max(0, now.timeIntervalSince(pausedAt))
        pausedTotal += length
        if !pauses.isEmpty { pauses[pauses.count - 1].length = length }
        self.pausedAt = nil
        phase = .recording
    }

    // MARK: "How long?"

    /// When it stops by itself; nil with no limit.
    var deadline: Date? { limit?.deadline }

    /// Whole minutes planned (after any +15 min); nil with no limit.
    var plannedMinutes: Int? { limit?.plannedMinutes }

    /// The warning for the current deadline has been given.
    var warned: Bool { limit?.warned ?? false }

    /// Seconds left; nil with no limit or when not recording.
    func timeLeft(at now: Date) -> TimeInterval? {
        guard isActive else { return nil }
        return limit?.remaining(at: now)
    }

    /// One tick of the timer, recording or paused (wall clock). `.warn` once
    /// per deadline, then `.stop` once at the deadline.
    mutating func tickLimit(at now: Date) -> TimeLimitPlan.Action {
        guard isActive, var plan = limit else { return .none }
        let action = plan.tick(now: now)
        limit = plan
        return action
    }

    /// "+15 min": from the deadline (or from now if it passed), capped at 12 h.
    @discardableResult
    mutating func extendLimit(at now: Date) -> Bool {
        guard isActive, var plan = limit else { return false }
        let ok = plan.extend(now: now)
        limit = plan
        return ok
    }

    /// "No limit"
    @discardableResult
    mutating func removeLimit() -> Bool {
        guard isActive, var plan = limit, plan.deadline != nil else { return false }
        plan.removeLimit()
        limit = plan
        return true
    }

    // MARK: Markers

    /// Mark the current moment (only while the microphone is on). Its time is
    /// wall-clock, the same clock as the start; its offset is the seconds of
    /// audio before it, which the contract's `wallClock(atFileOffset:)` maps
    /// back to the same time. nil when not recording, at the cap, or a
    /// repeat of the same kind within `markDebounce`.
    @discardableResult
    mutating func mark(_ kind: MarkerKind = .default, at now: Date, id: UUID = UUID()) -> WatchMarker? {
        guard phase == .recording, markers.count < WatchTransfer.maxMarkers else { return nil }
        if let last = markers.last, last.kind == kind, abs(now.timeIntervalSince(last.at)) < Self.markDebounce { return nil }
        let marker = WatchMarker(id: id, kind: kind, at: now, offset: elapsed(at: now))
        markers.append(marker)
        return marker
    }

    // MARK: Stop

    /// Ends the recording and returns what the phone needs. A pause still
    /// open at stop has no audio after it, so it is dropped from `pauses`;
    /// `endedAt` still says when the user stopped.
    @discardableResult
    mutating func stop(at now: Date, appVersion: String, audioDuration: TimeInterval? = nil) throws -> WatchRecordingMetadata {
        guard isActive, let recordingID, let startedAt else { throw TransitionError.notAllowed(from: phase, event: "stop") }
        let recorded = elapsed(at: now)
        if pausedAt != nil {
            pauses.removeLast()
            pausedTotal += max(0, now.timeIntervalSince(pausedAt!))
            pausedAt = nil
        }
        endedAt = max(now, startedAt)
        phase = .finished
        return WatchRecordingMetadata(
            recordingID: recordingID, startedAt: startedAt, endedAt: endedAt!,
            // The recorder's own clock is the truth for the file; the
            // wall-clock estimate covers a recorder that reports nothing
            duration: audioDuration.flatMap { $0 > 0 && $0.isFinite ? $0 : nil } ?? recorded,
            appVersion: appVersion, pauses: pauses,
            kind: kind, notebook: notebook, plannedMinutes: plannedMinutes, markers: markers)
    }

    /// Back to idle (after a failed start, or when the result has been saved).
    mutating func reset() {
        self = RecorderStateMachine()
    }
}
