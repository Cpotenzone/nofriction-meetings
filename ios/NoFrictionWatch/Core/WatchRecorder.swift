import AVFoundation
import Foundation
import Observation
import WatchKit

/// Records a meeting on the watch: AVAudioRecorder → AAC mono 16 kHz in the
/// app's container, then hands the finished file to the transfer queue.
///
/// Each pause (or interruption) closes the current file, and Resume starts
/// the next one ("parts"), so a paused recording is always complete on disk
/// even if watchOS ends the app before the user comes back.
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
    /// Something the user should know (permission, failure)
    private(set) var notice: String?

    private let store: WatchRecordingStore
    private let queue: WatchTransferQueue
    private var recorder: AVAudioRecorder?
    private var meterTask: Task<Void, Never>?
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

    // MARK: Controls

    func start() async {
        // A second tap (or the App Intent) while the permission check awaits
        guard !machine.isActive, !starting else { return }
        starting = true
        defer { starting = false }
        notice = nil
        guard await AVAudioApplication.requestRecordPermission() else {
            notice = "Microphone access is off. Turn it on in the Watch app on your iPhone → Privacy → Microphone."
            WKInterfaceDevice.current().play(.failure)
            return
        }
        let id = UUID()
        let now = clock()
        let url = store.beginRecording(id: id, startedAt: now)
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .default, options: [])
            try session.setActive(true)
            recorder = try Self.makeRecorder(url)
            try machine.start(id: id, at: now)
            WKInterfaceDevice.current().play(.start)
            startMeter()
        } catch {
            recorder?.stop()
            recorder = nil
            store.discard(id)
            try? AVAudioSession.sharedInstance().setActive(false)
            machine.reset()
            notice = "Couldn't start recording. \(error.localizedDescription)"
            WKInterfaceDevice.current().play(.failure)
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
                WKInterfaceDevice.current().play(.click)
            } catch {
                recorder = nil
                store.dropLastPart(id)
                notice = "Couldn't resume. Stop to keep what was recorded, then start again."
                WKInterfaceDevice.current().play(.failure)
            }
        } else if machine.phase == .recording {
            closePart()
            try? machine.pause(at: clock(), reason: .user)
            store.recordPauses(id, machine.pauses)
            level = 0
            WKInterfaceDevice.current().play(.click)
        }
    }

    func stop() {
        guard machine.isActive, let id = machine.recordingID else { return }
        closePart()
        meterTask?.cancel()
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
            } else {
                notice = "Nothing was recorded."
            }
        }
        machine.reset()
        WKInterfaceDevice.current().play(.stop)
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
        WKInterfaceDevice.current().play(.retry)
    }

    private func startMeter() {
        meterTask?.cancel()
        meterTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(100))
                guard let self, self.machine.isActive else { return }
                // Paused (no file open), or screen off / wrist down: nobody sees the meter
                guard let recorder = self.recorder, self.machine.phase == .recording, self.meterVisible else {
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
    func showDemo(elapsed: TimeInterval, paused: Bool = false, level: Float = 0.55) {
        let now = Date()
        try? machine.start(id: UUID(), at: now.addingTimeInterval(-elapsed))
        if paused { try? machine.pause(at: now, reason: .user) }
        self.level = paused ? 0 : level
        clock = { now }
    }
    #endif
}
