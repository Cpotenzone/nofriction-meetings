#if DEBUG
import AVFoundation
import Foundation

/// Debug builds only. Launch arguments for screenshots and simulator tests:
///
///     -NFWatchDemo idle|recording|paused|list   sample state, no microphone,
///                                               no WatchConnectivity
///     -NFWatchSendTestRecording                 write a 3-second synthetic tone and
///                                               send it to the paired iPhone (E2E check)
///
/// The demo uses a throwaway store in tmp, never the real recordings.
enum DemoMode: String {
    case idle, recording, paused, list

    static var current: DemoMode? {
        let args = ProcessInfo.processInfo.arguments
        guard let i = args.firstIndex(of: "-NFWatchDemo"), i + 1 < args.count else { return nil }
        return DemoMode(rawValue: args[i + 1])
    }

    @MainActor
    static func makeStore() -> WatchRecordingStore {
        let dir = URL.temporaryDirectory.appending(path: "nf-watch-demo-\(UUID().uuidString)", directoryHint: .isDirectory)
        return WatchRecordingStore(directory: dir)
    }

    @MainActor
    static func apply(_ mode: DemoMode, to model: WatchAppModel) {
        model.noticeAccepted = true
        switch mode {
        case .idle:
            break
        case .recording:
            model.recorder.showDemo(elapsed: 25 * 60 + 12)
        case .paused:
            model.recorder.showDemo(elapsed: 12 * 60 + 40, paused: true)
        case .list:
            seedList(model.store)
        }
        model.refresh()
    }

    /// Three sample rows, one per transfer state. No audio files.
    @MainActor
    private static func seedList(_ store: WatchRecordingStore) {
        let today = Calendar.current.date(bySettingHour: 9, minute: 41, second: 0, of: .now) ?? .now
        let rows: [(TimeInterval, TimeInterval, WatchRecordingEntry.Status)] = [
            (-30 * 60, 24 * 60 + 18, .sending),
            (-2 * 3600, 47 * 60 + 5, .delivered),
            (-26 * 3600, 31 * 60 + 52, .delivered),
        ]
        for (offset, seconds, status) in rows {
            let id = UUID()
            let start = today.addingTimeInterval(offset)
            store.beginRecording(id: id, startedAt: start)
            store.finish(WatchRecordingMetadata(recordingID: id, startedAt: start, endedAt: start.addingTimeInterval(seconds),
                                                duration: seconds, appVersion: "demo"))
            switch status {
            case .sending: store.markSending(id)
            case .delivered: store.markDelivered(id)
            default: break
            }
        }
    }

    /// Simulator end-to-end check: a short synthetic recording (a tone, no
    /// speech, no microphone) goes through the real store and transfer path.
    @MainActor
    static func queueSyntheticRecording(_ model: WatchAppModel) {
        let id = UUID()
        let start = Date().addingTimeInterval(-3)
        let url = model.store.beginRecording(id: id, startedAt: start)
        do {
            let settings: [String: Any] = [
                AVFormatIDKey: kAudioFormatMPEG4AAC,
                AVSampleRateKey: WatchTransfer.sampleRate,
                AVNumberOfChannelsKey: WatchTransfer.channels,
                AVEncoderBitRateKey: WatchTransfer.bitRate,
            ]
            // Scoped: the writer must close before the file is read back
            do {
                let file = try AVAudioFile(forWriting: url, settings: settings, commonFormat: .pcmFormatFloat32, interleaved: false)
                let frames = AVAudioFrameCount(WatchTransfer.sampleRate * 3)
                guard let buffer = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: frames) else { throw CocoaError(.fileWriteUnknown) }
                buffer.frameLength = frames
                for i in 0..<Int(frames) { buffer.floatChannelData![0][i] = 0.2 * sin(Float(i) * 2 * .pi * 440 / Float(WatchTransfer.sampleRate)) }
                try file.write(from: buffer)
            }
            let seconds = WatchRecorder.audioLength(of: url) ?? 3
            model.store.finish(WatchRecordingMetadata(recordingID: id, startedAt: start, endedAt: start.addingTimeInterval(seconds),
                                                      duration: seconds, appVersion: WatchRecorder.appVersion))
            model.queue.sendPending()
        } catch {
            model.store.discard(id)
        }
    }
}
#endif
