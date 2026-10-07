#if DEBUG
import AVFoundation
import Foundation

/// Debug builds only. Launch arguments for screenshots and simulator tests:
///
///     -NFWatchDemo idle|recording|paused|list   sample state, no microphone,
///                                               no WatchConnectivity
///     -NFWatchDemo length|notebook|controls     the Record flow's later steps; the
///                                               Discreet controls (layout checks)
///     -NFWatchDemo start|class|warning|discreet the Record flow; a Class recording
///                                               in a notebook with time left and
///                                               marks; its 5-minute warning; the
///                                               Discreet screen
///     -NFWatchSendTestRecording                 write a 3-second synthetic tone and
///                                               send it to the paired iPhone (E2E check)
///     -NFWatchAutoRecord                        the real recorder: 2 s, pause 2 s,
///                                               2 s more, stop and send (simulator check)
///
/// The demo uses a throwaway store in tmp, never the real recordings.
enum DemoMode: String {
    case idle, recording, paused, list
    case start, `class`, warning, discreet
    /// Layout checks: the Record flow's later steps and the Discreet controls
    case length, notebook, controls

    /// Sample notebook names, as the iPhone would send them
    static let notebooks = ["BIO 101", "Acme project", "Health"]

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
            model.page = .recordings
        case .start, .length, .notebook:
            model.connection.showDemoNotebooks(notebooks)
            model.demoStartPath = mode == .length ? [.length(.class)] : mode == .notebook ? [.length(.class), .notebook(.class, .minutes(60))] : []
            model.showStartFlow = true
        case .controls:
            model.recorder.showDemo(elapsed: 31 * 60 + 5,
                                    options: WatchStartOptions(kind: .class, limit: .minutes(90), notebook: "BIO 101", discreet: true))
            model.demoShowDiscreetControls = true
        case .class:
            // 60-minute class, 17:48 in: 42:12 left, three moments marked
            model.recorder.showDemo(elapsed: 17 * 60 + 48, level: 0.5,
                                    options: WatchStartOptions(kind: .class, limit: .minutes(60), notebook: "BIO 101"),
                                    marks: [.important, .test, .question])
        case .warning:
            model.recorder.showDemo(elapsed: 55 * 60 + 40, level: 0.4,
                                    options: WatchStartOptions(kind: .class, limit: .minutes(60), notebook: "BIO 101"),
                                    marks: [.important, .test], warning: true)
        case .discreet:
            model.recorder.showDemo(elapsed: 31 * 60 + 5,
                                    options: WatchStartOptions(kind: .class, limit: .minutes(90), notebook: "BIO 101", discreet: true),
                                    marks: [.important])
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
            // The one still on its way shows its type, notebook and marks
            let sending = status == .sending
            let marks = sending ? [MarkerKind.important, .test, .question].enumerated().map { i, kind in
                WatchMarker(kind: kind, at: start.addingTimeInterval(Double(i + 1) * 300))
            } : []
            store.finish(WatchRecordingMetadata(recordingID: id, startedAt: start, endedAt: start.addingTimeInterval(seconds),
                                                duration: seconds, appVersion: "demo", kind: sending ? .class : .meeting,
                                                notebook: sending ? "BIO 101" : nil, markers: marks))
            switch status {
            case .sending: store.markSending(id)
            case .delivered: store.markConfirmed(id, part: 0)
            default: break
            }
        }
    }

    /// Simulator check of the real microphone path, with one pause (two parts).
    @MainActor
    static func autoRecord(_ model: WatchAppModel) {
        model.noticeAccepted = true
        Task { @MainActor in
            await model.recorder.start()
            try? await Task.sleep(for: .seconds(2))
            model.recorder.togglePause()
            try? await Task.sleep(for: .seconds(2))
            model.recorder.togglePause()
            try? await Task.sleep(for: .seconds(2))
            model.recorder.stop()
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
