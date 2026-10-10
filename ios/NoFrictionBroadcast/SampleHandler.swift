import CoreMedia
import Foundation
import ReplayKit

/// noFriction's screen-capture broadcast extension (docs/SCREEN_CAPTURE_IOS.md).
///
/// ReplayKit hands it every frame of the screen while the user captures. It
/// keeps a screen only when it changed (at most one per second), as a
/// downscaled JPEG in the App Group container, and, only when the app says
/// so (noFriction Pro and "Transcribe what's playing" on), the audio of the
/// app that is playing as an AAC file. The noFriction app imports both into
/// the recording and deletes them here.
///
/// It never touches the network, never reads the microphone stream (the
/// app records the microphone itself), and holds no frame after it returns.
/// Extensions get about 50 MB: everything here is sized well under that.
final class SampleHandler: RPBroadcastSampleHandler {
    private let darwin = DarwinListener()
    /// Set on start, cleared at the end; ReplayKit and Darwin callbacks arrive on different threads
    private let lock = NSLock()
    private var _recorder: BroadcastRecorder?
    private var recorder: BroadcastRecorder? {
        get { lock.withLock { _recorder } }
        set { lock.withLock { _recorder = newValue } }
    }

    override func broadcastStarted(withSetupInfo setupInfo: [String: NSObject]?) {
        guard let root = ScreenCaptureContract.root() else {
            finish(message: "noFriction couldn't open its storage for screen capture. Reinstall noFriction and try again.")
            return
        }
        let shared = ScreenCaptureContract.defaults()
        // A stop left over from an earlier capture must not end this one
        shared?.removeObject(forKey: ScreenCaptureContract.Key.stopRequest)
        let appAudio = shared?.bool(forKey: ScreenCaptureContract.Key.appAudio) ?? false
        do {
            recorder = try BroadcastRecorder(root: root, appAudio: appAudio)
        } catch {
            finish(message: "noFriction couldn't start capturing your screen. Check that there is free space and try again.")
            return
        }
        recorder?.setPaused(shared?.bool(forKey: ScreenCaptureContract.Key.paused) ?? false)
        darwin.observe(ScreenCaptureContract.Signal.control) { [weak self] in self?.readControl() }
        DarwinListener.post(ScreenCaptureContract.Signal.started)
    }

    override func broadcastPaused() {
        recorder?.setSystemPaused(true)
    }

    override func broadcastResumed() {
        recorder?.setSystemPaused(false)
    }

    override func broadcastFinished() {
        endCapture()
    }

    override func processSampleBuffer(_ sampleBuffer: CMSampleBuffer, with sampleBufferType: RPSampleBufferType) {
        switch sampleBufferType {
        case .video:
            recorder?.video(sampleBuffer)
        case .audioApp:
            recorder?.appAudio(sampleBuffer)
        case .audioMic:
            // The app records the microphone itself; never kept here
            break
        @unknown default:
            break
        }
    }

    // MARK: Control from the app

    /// The app paused, resumed or stopped the recording.
    private func readControl() {
        let shared = ScreenCaptureContract.defaults()
        recorder?.setPaused(shared?.bool(forKey: ScreenCaptureContract.Key.paused) ?? false)
        guard let request = shared?.string(forKey: ScreenCaptureContract.Key.stopRequest),
              let id = recorder?.id, request == "*" || request == id.uuidString else { return }
        shared?.removeObject(forKey: ScreenCaptureContract.Key.stopRequest)
        endCapture()
        // ReplayKit shows this text when it ends the broadcast
        finish(message: "Your recording in noFriction ended, so screen capture stopped.")
    }

    private func endCapture() {
        darwin.stop()
        let current = lock.withLock { () -> BroadcastRecorder? in
            defer { _recorder = nil }
            return _recorder
        }
        guard let current else { return }
        current.finish()
        DarwinListener.post(ScreenCaptureContract.Signal.finished)
    }

    private func finish(message: String) {
        let error = NSError(domain: "com.nofriction.meetings.broadcast", code: 1,
                            userInfo: [NSLocalizedDescriptionKey: message])
        finishBroadcastWithError(error)
    }
}

/// Darwin notifications: names only, between this extension and the app.
final class DarwinListener {
    private var handler: (() -> Void)?
    private var name: String?

    func observe(_ name: String, _ handler: @escaping () -> Void) {
        stop()
        self.handler = handler
        self.name = name
        let center = CFNotificationCenterGetDarwinNotifyCenter()
        CFNotificationCenterAddObserver(center, Unmanaged.passUnretained(self).toOpaque(), { _, observer, _, _, _ in
            guard let observer else { return }
            Unmanaged<DarwinListener>.fromOpaque(observer).takeUnretainedValue().handler?()
        }, name as CFString, nil, .deliverImmediately)
    }

    func stop() {
        guard let name else { return }
        CFNotificationCenterRemoveObserver(CFNotificationCenterGetDarwinNotifyCenter(), Unmanaged.passUnretained(self).toOpaque(),
                                           CFNotificationName(name as CFString), nil)
        self.name = nil
        handler = nil
    }

    deinit { stop() }

    static func post(_ name: String) {
        CFNotificationCenterPostNotification(CFNotificationCenterGetDarwinNotifyCenter(), CFNotificationName(name as CFString),
                                             nil, nil, true)
    }
}
