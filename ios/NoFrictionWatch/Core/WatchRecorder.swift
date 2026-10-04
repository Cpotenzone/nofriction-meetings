import AVFoundation
import Foundation
import Observation
import WatchKit

/// Records a meeting on the watch: AVAudioRecorder → AAC mono 16 kHz in the
/// app's container, then hands the finished file to the transfer queue.
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
        guard !machine.isActive else { return }
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
            let recorder = try AVAudioRecorder(url: url, settings: [
                AVFormatIDKey: kAudioFormatMPEG4AAC,
                AVSampleRateKey: WatchTransfer.sampleRate,
                AVNumberOfChannelsKey: WatchTransfer.channels,
                AVEncoderBitRateKey: WatchTransfer.bitRate,
                AVEncoderAudioQualityKey: AVAudioQuality.high.rawValue,
            ])
            recorder.isMeteringEnabled = true
            guard recorder.prepareToRecord(), recorder.record() else { throw RecorderError.couldNotStart }
            self.recorder = recorder
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
        guard let recorder else { return }
        if machine.isPaused {
            // Resuming needs the app in the foreground (it is: the user tapped)
            do {
                try AVAudioSession.sharedInstance().setActive(true)
                guard recorder.record() else { throw RecorderError.couldNotStart }
                try machine.resume(at: clock())
                notice = nil
                WKInterfaceDevice.current().play(.click)
            } catch {
                notice = "Couldn't resume. Stop to keep what was recorded, then start again."
                WKInterfaceDevice.current().play(.failure)
            }
        } else if machine.phase == .recording {
            recorder.pause()
            try? machine.pause(at: clock(), reason: .user)
            level = 0
            WKInterfaceDevice.current().play(.click)
        }
    }

    func stop() {
        guard machine.isActive, let recorder else { return }
        let audioSeconds = recorder.currentTime
        recorder.stop()
        self.recorder = nil
        meterTask?.cancel()
        level = 0
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        let metadata = try? machine.stop(at: clock(), appVersion: Self.appVersion,
                                         audioDuration: audioSeconds > 0 ? audioSeconds : nil)
        if let metadata {
            // Ask the file itself; the recorder's clock stops at pauses too
            var final = metadata
            if let seconds = Self.audioLength(of: store.fileURL(for: metadata.recordingID)), seconds > 0 {
                final.duration = seconds
            }
            store.finish(final)
            queue.sendPending()
        }
        machine.reset()
        WKInterfaceDevice.current().play(.stop)
    }

    /// Readable audio length in seconds, nil if the file can't be opened
    /// (e.g. the app was killed before the recorder finished writing it).
    nonisolated static func audioLength(of url: URL) -> TimeInterval? {
        guard let file = try? AVAudioFile(forReading: url), file.fileFormat.sampleRate > 0 else { return nil }
        return Double(file.length) / file.fileFormat.sampleRate
    }

    // MARK: Internals

    private func interrupted() {
        guard machine.phase == .recording || machine.isPaused else { return }
        try? machine.pause(at: clock(), reason: .interruption)
        level = 0
        notice = "Paused by a call or Siri. Tap Resume to keep recording."
        WKInterfaceDevice.current().play(.retry)
    }

    private func startMeter() {
        meterTask?.cancel()
        meterTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(100))
                guard let self, let recorder = self.recorder else { return }
                // Screen off / wrist down: nobody sees the meter, save the battery
                guard self.machine.phase == .recording, self.meterVisible else { self.level = 0; continue }
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
