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
/// wall-clock time on the phone).
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
    /// Wall-clock time spent paused, closed pauses only
    private var pausedTotal: TimeInterval = 0
    private var pausedAt: Date?

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

    mutating func start(id: UUID, at now: Date) throws {
        guard phase == .idle || phase == .finished else { throw TransitionError.notAllowed(from: phase, event: "start") }
        self = RecorderStateMachine()
        phase = .recording
        recordingID = id
        startedAt = now
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
            appVersion: appVersion, pauses: pauses)
    }

    /// Back to idle (after a failed start, or when the result has been saved).
    mutating func reset() {
        self = RecorderStateMachine()
    }
}
