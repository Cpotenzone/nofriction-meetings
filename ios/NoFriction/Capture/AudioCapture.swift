import AVFoundation
import Foundation

/// Microphone capture. Every buffer goes to the transcriber and to an AAC
/// file, so the meeting's audio is kept and can be re-transcribed later.
/// Keeps running with the screen locked (UIBackgroundModes: audio).
final class AudioCapture: @unchecked Sendable {
    private let engine = AVAudioEngine()
    private var file: AVAudioFile?
    private let lock = NSLock()
    private var paused = false

    /// Called on the audio thread for every buffer.
    var onBuffer: (@Sendable (AVAudioPCMBuffer) -> Void)?
    /// Input level 0…1, for the live meter.
    private(set) var level: Float = 0
    /// Screen capture is on: keep Bluetooth headphones in high-quality playback (A2DP, not HFP)
    var keepsPlaybackQuality = false

    var inputFormat: AVAudioFormat {
        #if DEBUG
        if let f = testAudioFormat { return f }
        #endif
        return engine.inputNode.outputFormat(forBus: 0)
    }

    static func requestPermission() async -> Bool {
        await AVAudioApplication.requestRecordPermission()
    }

    func configureSession() throws {
        #if DEBUG
        if testAudioFormat != nil { return }
        #endif
        let session = AVAudioSession.sharedInstance()
        // .spokenAudio + voice processing off: meetings are far-field speech,
        // and echo cancellation would suppress the other side on speaker
        let options = Self.categoryOptions(keepsPlaybackQuality: keepsPlaybackQuality)
        try session.setCategory(.playAndRecord, mode: .spokenAudio, options: options)
        try session.setActive(true, options: [])
    }

    /// .mixWithOthers: another app's audio (a video watched while the screen
    /// is captured) keeps playing; noFriction never interrupts or ducks it
    /// (no .duckOthers). .defaultToSpeaker keeps that audio on the speaker,
    /// not the earpiece. docs/SCREEN_CAPTURE_IOS.md "Audio session".
    static func categoryOptions(keepsPlaybackQuality: Bool) -> AVAudioSession.CategoryOptions {
        var options: AVAudioSession.CategoryOptions = [.defaultToSpeaker, .mixWithOthers]
        if keepsPlaybackQuality {
            // Capturing the screen: Bluetooth headphones stay in high-quality
            // playback (A2DP); the hands-free profile would turn the video's
            // sound into call quality. The iPhone's own mic records.
            options.insert(.allowBluetoothA2DP)
        } else if #available(iOS 26.0, *) {
            options.insert(.allowBluetoothHFP)
        } else {
            options.insert(.allowBluetooth)
        }
        return options
    }

    func start(recordingTo url: URL) throws {
        #if DEBUG
        // Test hook: NF_TEST_AUDIO=<file> plays a file through the pipeline in
        // real time instead of using the microphone (simulator testing)
        if let url = Self.testAudioURL {
            try startFromFile(url)
            return
        }
        #endif
        try configureSession()
        let format = inputFormat
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: format.sampleRate,
            AVNumberOfChannelsKey: 1,
            AVEncoderBitRateKey: 64_000,
        ]
        let file = try AVAudioFile(forWriting: url, settings: settings,
                                   commonFormat: format.commonFormat, interleaved: format.isInterleaved)
        lock.withLock { self.file = file; paused = false }

        engine.inputNode.removeTap(onBus: 0)
        engine.inputNode.installTap(onBus: 0, bufferSize: 4096, format: format) { [weak self] buffer, _ in
            guard let self else { return }
            let (file, paused) = self.lock.withLock { (self.file, self.paused) }
            guard !paused else { return }
            self.level = Self.rms(buffer)
            try? file?.write(from: buffer)
            self.onBuffer?(buffer)
        }
        engine.prepare()
        try engine.start()
    }

    func setPaused(_ value: Bool) {
        lock.withLock { paused = value }
        if value { level = 0 }
    }

    func stop() {
        #if DEBUG
        fileFeed?.cancel()
        #endif
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        lock.withLock { file = nil }
        level = 0
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    #if DEBUG
    private var fileFeed: Task<Void, Never>?

    /// Absolute path, or a file name inside the app's Documents (devices)
    static var testAudioURL: URL? {
        guard let value = ProcessInfo.processInfo.environment["NF_TEST_AUDIO"] else { return nil }
        return value.hasPrefix("/") ? URL(fileURLWithPath: value) : Storage.documents.appending(path: value)
    }

    var testAudioFormat: AVAudioFormat? {
        Self.testAudioURL.flatMap { try? AVAudioFile(forReading: $0).processingFormat }
    }

    private func startFromFile(_ url: URL) throws {
        let file = try AVAudioFile(forReading: url)
        let format = file.processingFormat
        let chunk = AVAudioFrameCount(format.sampleRate / 10)
        fileFeed = Task.detached { [weak self] in
            while !Task.isCancelled {
                guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: chunk) else { return }
                do { try file.read(into: buffer, frameCount: chunk) } catch { break }
                if buffer.frameLength == 0 { break }
                self?.level = Self.rms(buffer)
                self?.onBuffer?(buffer)
                try? await Task.sleep(for: .milliseconds(100))
            }
            self?.level = 0
        }
    }
    #endif

    private static func rms(_ buffer: AVAudioPCMBuffer) -> Float {
        guard let data = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return 0 }
        let n = Int(buffer.frameLength)
        var sum: Float = 0
        for i in 0..<n { sum += data[i] * data[i] }
        // Map roughly -50…0 dBFS to 0…1
        let db = 20 * log10(max(sqrt(sum / Float(n)), 1e-6))
        return max(0, min(1, (db + 50) / 50))
    }
}
