import AVFoundation
import Foundation
import Observation
import UserNotifications
import WatchKit

/// Records on the watch: AVAudioRecorder → AAC mono 16 kHz in the app's
/// container, then hands the finished file to the transfer queue.
///
/// Each pause (or interruption) closes the current file, and Resume starts
/// the next one ("parts"), so a paused recording is always complete on disk
/// even if watchOS ends the app before the user comes back.
///
/// A recording starts with the Record flow's choices (`WatchStartOptions`):
/// what it is, how long, which notebook, and the Discreet display. "How
/// long?" is enforced here on wall-clock time from the start (pausing
/// doesn't move it): a warning 5 minutes before the end (2 for 15 minutes),
/// then `stop()` at the deadline, the same path as the Stop button.
///
/// What watchOS allows (docs/WATCH_APP.md, "Recording limits"):
/// - With the `audio` background mode, a recording started in the foreground
///   keeps going when the wrist drops and the screen turns off, and the app
///   comes back when the wrist is raised.
/// - A call, Siri or another app taking the microphone interrupts it. watchOS
///   doesn't let an app restart recording from the background, so the
///   recording pauses and the user taps Resume.
@MainActor
@Observable
final class WatchRecorder: NSObject {
    private(set) var machine = RecorderStateMachine()
    /// Input level 0…1 for the meter
    private(set) var level: Float = 0
    /// Something the user should know (permission, failure, a stop at the limit)
    private(set) var notice: String?
    /// The recording in progress uses the Discreet display
    private(set) var discreet = false
    /// "5 minutes left" is showing (cleared by +15 min, No limit, dismiss or stop)
    private(set) var timeWarningVisible = false
    /// The most recent mark, for its brief confirmation
    private(set) var lastMark: WatchMarker?

    private let store: WatchRecordingStore
    private let queue: WatchTransferQueue
    private var recorder: AVAudioRecorder?
    private var meterTask: Task<Void, Never>?
    private var limitTask: Task<Void, Never>?
    private var interruptionObserver: NSObjectProtocol?
    /// Injectable for tests and demo screenshots
    var clock: () -> Date = Date.init
    /// False while the screen is off (scene not active)
    var meterVisible = true
    private var starting = false

    init(store: WatchRecordingStore, queue: WatchTransferQueue) {
        self.store = store
        self.queue = queue
        super.init()
        interruptionObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.interruptionNotification, object: nil, queue: .main
        ) { [weak self] note in
            let began = (note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt)
                .flatMap(AVAudioSession.InterruptionType.init(rawValue:)) == .began
            MainActor.assumeIsolated { if began { self?.interrupted() } }
        }
    }

    static var appVersion: String {
        let info = Bundle.main.infoDictionary
        return "\(info?["CFBundleShortVersionString"] as? String ?? "?") (\(info?["CFBundleVersion"] as? String ?? "?"))"
    }

    var elapsed: TimeInterval { machine.elapsed(at: clock()) }
    /// Seconds left with a time limit; nil without one
    var timeLeft: TimeInterval? { machine.timeLeft(at: clock()) }

    // MARK: Controls

    /// `options` default: the remembered choices (the App Intent).
    func start(_ options: WatchStartOptions = .remembered()) async {
        // A second tap (or the App Intent) while the permission check awaits
        guard !machine.isActive, !starting else { return }
        starting = true
        defer { starting = false }
        notice = nil
        guard await AVAudioApplication.requestRecordPermission() else {
            notice = "Microphone access is off. Turn it on in the Watch app on your iPhone → Privacy → Microphone."
            WatchHaptics.play(.failure, discreet: options.discreet)
            return
        }
        let id = UUID()
        let now = clock()
        let url = store.beginRecording(id: id, startedAt: now, kind: options.kind, notebook: options.notebook,
                                       plannedMinutes: options.limit.minutes)
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .default, options: [])
            try session.setActive(true)
            recorder = try Self.makeRecorder(url)
            try machine.start(id: id, at: now, kind: options.kind, notebook: options.notebook, limit: options.limit)
            discreet = options.discreet
            timeWarningVisible = false
            lastMark = nil
            WatchHaptics.play(.start, discreet: discreet)
            startMeter()
            startLimitTimer()
        } catch {
            recorder?.stop()
            recorder = nil
            store.discard(id)
            try? AVAudioSession.sharedInstance().setActive(false)
            machine.reset()
            discreet = false
            notice = "Couldn't start recording. \(error.localizedDescription)"
            WatchHaptics.play(.failure, discreet: options.discreet)
        }
    }

    func togglePause() {
        guard let id = machine.recordingID else { return }
        if machine.isPaused {
            // Resuming needs the app in the foreground (it is: the user
            // tapped). It continues in a new file (part).
            guard let url = store.beginPart(id) else { return }
            do {
                try AVAudioSession.sharedInstance().setActive(true)
                recorder = try Self.makeRecorder(url)
                try machine.resume(at: clock())
                store.recordPauses(id, machine.pauses)
                notice = nil
                WatchHaptics.play(.pauseResume, discreet: discreet)
            } catch {
                recorder = nil
                store.dropLastPart(id)
                notice = "Couldn't resume. Stop to keep what was recorded, then start again."
                WatchHaptics.play(.failure, discreet: discreet)
            }
        } else if machine.phase == .recording {
            closePart()
            try? machine.pause(at: clock(), reason: .user)
            store.recordPauses(id, machine.pauses)
            level = 0
            WatchHaptics.play(.pauseResume, discreet: discreet)
        }
    }

    /// The Stop button (and the Discreet stop control).
    func stop() { finish(atLimit: false) }

    /// Mark this moment (★ with one tap; ? or ✎ from the follow-up choice).
    /// Saved at once with the recording, so a crash keeps it.
    @discardableResult
    func mark(_ kind: MarkerKind = .default) -> WatchMarker? {
        guard let id = machine.recordingID, let marker = machine.mark(kind, at: clock()) else { return nil }
        store.recordMarkers(id, machine.markers)
        lastMark = marker
        WatchHaptics.play(.mark, discreet: discreet)
        return marker
    }

    // MARK: Time limit

    /// "+15 min" (screen or notification)
    func extendLimit() {
        guard let id = machine.recordingID, machine.extendLimit(at: clock()) else { return }
        store.recordPlan(id, plannedMinutes: machine.plannedMinutes)
        timeWarningVisible = machine.warned
        if limitTask == nil { startLimitTimer() } else { scheduleWarningNotification() }
        WatchHaptics.play(.pauseResume, discreet: discreet)
    }

    /// "No limit" (screen or notification)
    func removeLimit() {
        guard let id = machine.recordingID, machine.removeLimit() else { return }
        store.recordPlan(id, plannedMinutes: nil)
        timeWarningVisible = false
        limitTask?.cancel()
        limitTask = nil
        WatchTimeLimitNotifier.cancel()
        WatchHaptics.play(.pauseResume, discreet: discreet)
    }

    func dismissWarning() { timeWarningVisible = false }

    /// One tick of the time limit. Runs every second while recording or
    /// paused, and when the app comes back to the foreground (a paused,
    /// suspended app catches up: past the deadline it stops right away).
    func tickLimit() {
        switch machine.tickLimit(at: clock()) {
        case .none:
            break
        case .warn:
            timeWarningVisible = true
            WatchHaptics.play(.warning, discreet: discreet)
        case .stop:
            finish(atLimit: true)
        }
    }

    private func startLimitTimer() {
        limitTask?.cancel()
        limitTask = nil
        guard machine.deadline != nil else { return }
        scheduleWarningNotification()
        limitTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self, !Task.isCancelled else { return }
                self.tickLimit()
            }
        }
    }

    /// The warning also as a local notification, for when the app is in the
    /// background at that moment (the watch face is showing). Asked for in
    /// context, at the first timed recording; never at launch.
    private func scheduleWarningNotification() {
        guard let plan = machine.limit, plan.deadline != nil, !plan.warned, let id = machine.recordingID else {
            WatchTimeLimitNotifier.cancel()
            return
        }
        Task { [weak self] in
            await WatchTimeLimitNotifier.requestAuthorizationIfNeeded()
            // Still the same plan of the same recording
            guard let self, self.machine.recordingID == id, let current = self.machine.limit, current == plan else { return }
            await WatchTimeLimitNotifier.schedule(for: plan)
        }
    }

    // MARK: Stop

    /// Every stop goes through here: Stop, the Discreet stop control, and
    /// the deadline. The parts are finalized and queued for the iPhone.
    private func finish(atLimit: Bool) {
        guard machine.isActive, let id = machine.recordingID else { return }
        let planned = machine.plannedMinutes
        let wasDiscreet = discreet
        closePart()
        meterTask?.cancel()
        limitTask?.cancel()
        limitTask = nil
        WatchTimeLimitNotifier.cancel()
        level = 0
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        // The files are the truth: each part's real length (nil if it can't
        // be read), every pause where a part ends. Empty parts (Resume then
        // straight back to Pause) are dropped and their pauses merged.
        let lengths = (store.entry(id).map(store.partURLs) ?? []).map { Self.audioLength(of: $0) }
        let total = lengths.compactMap { $0 }.reduce(0, +)
        if let metadata = try? machine.stop(at: clock(), appVersion: Self.appVersion, audioDuration: total > 0 ? total : nil) {
            if store.finish(metadata, partLengths: lengths) {
                queue.sendPending()
                if atLimit {
                    notice = planned.map { "Stopped at its \($0)-minute limit. It's on its way to your iPhone." }
                        ?? "Stopped at its time limit. It's on its way to your iPhone."
                }
            } else {
                notice = "Nothing was recorded."
            }
        }
        machine.reset()
        discreet = false
        timeWarningVisible = false
        lastMark = nil
        WatchHaptics.play(.stop, discreet: wasDiscreet)
    }

    /// Finish the current file so it's complete and readable on its own.
    private func closePart() {
        recorder?.stop()
        recorder = nil
    }

    private static func makeRecorder(_ url: URL) throws -> AVAudioRecorder {
        let recorder = try AVAudioRecorder(url: url, settings: [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: WatchTransfer.sampleRate,
            AVNumberOfChannelsKey: WatchTransfer.channels,
            AVEncoderBitRateKey: WatchTransfer.bitRate,
            AVEncoderAudioQualityKey: AVAudioQuality.high.rawValue,
        ])
        recorder.isMeteringEnabled = true
        guard recorder.prepareToRecord(), recorder.record() else { throw RecorderError.couldNotStart }
        return recorder
    }

    /// Readable audio length in seconds, nil if the file can't be opened
    /// (e.g. the app was killed before the recorder finished writing it).
    nonisolated static func audioLength(of url: URL) -> TimeInterval? {
        guard let file = try? AVAudioFile(forReading: url), file.fileFormat.sampleRate > 0 else { return nil }
        return Double(file.length) / file.fileFormat.sampleRate
    }

    // MARK: Internals

    private func interrupted() {
        guard machine.phase == .recording || machine.isPaused, let id = machine.recordingID else { return }
        // Close the file now: if nobody comes back, what was recorded is
        // already complete (watchOS won't let the app resume on its own)
        if machine.phase == .recording { closePart() }
        try? machine.pause(at: clock(), reason: .interruption)
        store.recordPauses(id, machine.pauses)
        level = 0
        notice = "Paused by a call or Siri. Tap Resume to keep recording."
        WatchHaptics.play(.interrupted, discreet: discreet)
    }

    private func startMeter() {
        meterTask?.cancel()
        meterTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(100))
                guard let self, self.machine.isActive else { return }
                // Paused (no file open), Discreet (no meter shown), or screen
                // off / wrist down: nobody sees the meter
                guard let recorder = self.recorder, self.machine.phase == .recording, self.meterVisible, !self.discreet else {
                    self.level = 0
                    continue
                }
                recorder.updateMeters()
                // Map roughly -50…0 dBFS to 0…1 (same scale as the iPhone meter)
                let db = recorder.averagePower(forChannel: 0)
                let target = max(0, min(1, (db + 50) / 50))
                self.level = self.level * 0.5 + target * 0.5
            }
        }
    }

    enum RecorderError: LocalizedError {
        case couldNotStart
        var errorDescription: String? { "The microphone didn't start." }
    }

    #if DEBUG
    /// Screenshots: show a recording in progress without the microphone.
    func showDemo(elapsed: TimeInterval, paused: Bool = false, level: Float = 0.55,
                  options: WatchStartOptions = WatchStartOptions(), marks: [MarkerKind] = [], warning: Bool = false) {
        let now = Date()
        let start = now.addingTimeInterval(-elapsed)
        try? machine.start(id: UUID(), at: start, kind: options.kind, notebook: options.notebook, limit: options.limit)
        for (i, kind) in marks.enumerated() {
            // Spread over the recording so far
            machine.mark(kind, at: start.addingTimeInterval(elapsed * Double(i + 1) / Double(marks.count + 1)))
        }
        lastMark = machine.markers.last
        if paused { try? machine.pause(at: now, reason: .user) }
        if warning { _ = machine.tickLimit(at: now) }
        discreet = options.discreet
        timeWarningVisible = warning
        self.level = paused ? 0 : level
        clock = { now }
    }

    /// Film footage (-NFWatchFilm): the demo's clock runs and its meter moves
    /// (screenshots freeze both). Still no microphone.
    func filmDemoMotion() {
        clock = Date.init
        meterTask?.cancel()
        meterTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(120))
                guard let self else { return }
                guard self.machine.isActive, !self.machine.isPaused else { continue }
                self.level = Float.random(in: 0.25...0.8)
            }
        }
    }
    #endif
}

/// The "5 minutes left" warning as a watch notification with +15 min and
/// No limit, scheduled ahead so it still arrives while the app is in the
/// background. Only when notification permission is granted; asked for at
/// the first timed recording. Generic text: no title or notebook name.
@MainActor
enum WatchTimeLimitNotifier {
    nonisolated static let category = "WATCH_TIME_LIMIT"
    nonisolated static let extendAction = "WATCH_TIME_LIMIT_EXTEND"
    nonisolated static let noLimitAction = "WATCH_TIME_LIMIT_REMOVE"
    nonisolated static let requestID = "watch-time-limit-warning"

    nonisolated static var notificationCategory: UNNotificationCategory {
        let extend = UNNotificationAction(identifier: extendAction, title: "+15 min", options: [])
        let noLimit = UNNotificationAction(identifier: noLimitAction, title: "No limit", options: [])
        return UNNotificationCategory(identifier: category, actions: [extend, noLimit], intentIdentifiers: [], options: [])
    }

    nonisolated static func title(secondsLeft: Int) -> String {
        let minutes = max(1, Int((Double(secondsLeft) / 60).rounded(.up)))
        return minutes == 1 ? "1 minute left in this recording" : "\(minutes) minutes left in this recording"
    }

    static func requestAuthorizationIfNeeded() async {
        let center = UNUserNotificationCenter.current()
        guard await center.notificationSettings().authorizationStatus == .notDetermined else { return }
        _ = try? await center.requestAuthorization(options: [.alert, .sound])
    }

    static func schedule(for plan: TimeLimitPlan) async {
        cancel()
        guard let warnAt = plan.warnAt, let deadline = plan.deadline, !plan.warned else { return }
        let center = UNUserNotificationCenter.current()
        let status = await center.notificationSettings().authorizationStatus
        guard status == .authorized || status == .provisional else { return }
        let content = UNMutableNotificationContent()
        content.title = title(secondsLeft: Int(deadline.timeIntervalSince(warnAt)))
        content.body = "It stops at \(deadline.formatted(date: .omitted, time: .shortened)). Add 15 minutes or remove the limit."
        content.categoryIdentifier = category
        content.sound = .default
        let trigger = UNTimeIntervalNotificationTrigger(timeInterval: max(1, warnAt.timeIntervalSinceNow), repeats: false)
        try? await center.add(UNNotificationRequest(identifier: requestID, content: content, trigger: trigger))
    }

    static func cancel() {
        let center = UNUserNotificationCenter.current()
        center.removePendingNotificationRequests(withIdentifiers: [requestID])
        center.removeDeliveredNotifications(withIdentifiers: [requestID])
    }
}
