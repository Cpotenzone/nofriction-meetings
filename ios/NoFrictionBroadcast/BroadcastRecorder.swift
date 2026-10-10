import AVFoundation
import CoreImage
import CoreMedia
import CoreVideo
import Foundation
import ImageIO
import ReplayKit

/// One broadcast's folder in the App Group container: screens as JPEGs
/// named by their capture time, app audio as AAC parts, and the manifest
/// the app reads (docs/SCREEN_CAPTURE_IOS.md).
///
/// Memory: no sample or pixel buffer outlives the call that delivered it.
/// A frame is reduced to a 64-cell brightness grid (a few KB), and only a
/// frame that is kept is encoded, through one reused Core Image context,
/// inside an autorelease pool. Video and audio each have a serial queue,
/// and the work runs synchronously on the ReplayKit thread that delivered
/// it, so nothing queues up behind a slow encode (ReplayKit drops frames
/// instead).
final class BroadcastRecorder: @unchecked Sendable {
    let id = UUID()
    private let folder: URL
    private let appAudioAllowed: Bool

    /// Look at a frame at most this often (seconds); the detector decides what to keep
    static let analyzeEvery = 0.25
    /// Longest side of a saved screen, in pixels
    static let maxSide: CGFloat = 1280
    static let jpegQuality = 0.6

    // Shared state (lock)
    private let lock = NSLock()
    private var manifest: ScreenCaptureManifest
    private var lastWrite = Date.distantPast
    private var userPaused = false
    private var systemPaused = false
    private var finished = false
    private var heartbeat: DispatchSourceTimer?

    // Video (videoQueue)
    private let videoQueue = DispatchQueue(label: "com.nofriction.broadcast.video")
    private var detector = ScreenChangeDetector()
    private var hidden = HiddenVideoTracker()
    private var lastAnalyzed: TimeInterval?
    private lazy var imageContext = CIContext(options: [.cacheIntermediates: false, .priorityRequestLow: true])

    // App audio (audioQueue)
    private let audioQueue = DispatchQueue(label: "com.nofriction.broadcast.audio")
    private var writer: AVAssetWriter?
    private var writerInput: AVAssetWriterInput?
    private var partIndex = 0

    init(root: URL, appAudio: Bool) throws {
        folder = root.appending(path: id.uuidString, directoryHint: .isDirectory)
        appAudioAllowed = appAudio
        manifest = ScreenCaptureManifest(id: id, startedAt: Date(), appAudio: appAudio)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        // The manifest comes first: a folder without one is never a live capture
        try manifest.write(to: folder)
        lastWrite = Date()
        startHeartbeat()
    }

    private var isTaking: Bool {
        lock.withLock { !finished && !userPaused && !systemPaused }
    }

    // MARK: Pause / finish

    /// The noFriction recording was paused or resumed: nothing is kept while paused.
    func setPaused(_ paused: Bool) {
        let changed = lock.withLock { () -> Bool in
            defer { userPaused = paused }
            return userPaused != paused
        }
        if changed && paused { audioQueue.sync { closePart() } }
    }

    /// ReplayKit paused or resumed the broadcast.
    func setSystemPaused(_ paused: Bool) {
        lock.withLock { systemPaused = paused }
        if paused { audioQueue.sync { closePart() } }
    }

    /// Close the audio file and mark the broadcast ended. Safe to call twice.
    func finish() {
        let first = lock.withLock { () -> Bool in
            defer { finished = true }
            return !finished
        }
        guard first else { return }
        heartbeat?.cancel()
        videoQueue.sync {}
        audioQueue.sync { closePart() }
        update(force: true) { $0.endedAt = Date() }
    }

    // MARK: Video

    func video(_ sampleBuffer: CMSampleBuffer) {
        guard isTaking else { return }
        videoQueue.sync {
            autoreleasepool { analyze(sampleBuffer) }
        }
    }

    private func analyze(_ sampleBuffer: CMSampleBuffer) {
        let time = CMSampleBufferGetPresentationTimeStamp(sampleBuffer).seconds
        guard time.isFinite, let pixels = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        if let last = lastAnalyzed, time >= last, time - last < Self.analyzeEvery { return }
        lastAnalyzed = time
        guard let grid = Self.grid(of: pixels) else { return }
        let decision = detector.consider(grid, at: time)
        hidden.note(hidden: decision == .hidden, at: time)
        var kept = false
        if decision == .keep {
            kept = save(pixels, orientation: Self.orientation(of: sampleBuffer))
        }
        let hiddenSeconds = hidden.seconds
        update(force: kept) { m in
            if kept { m.framesKept += 1 }
            m.hiddenSeconds = hiddenSeconds
        }
    }

    /// The frame's brightness grid, read straight from its luma plane (or BGRA).
    static func grid(of pixels: CVPixelBuffer) -> LumaGrid? {
        guard CVPixelBufferLockBaseAddress(pixels, .readOnly) == kCVReturnSuccess else { return nil }
        defer { CVPixelBufferUnlockBaseAddress(pixels, .readOnly) }
        switch CVPixelBufferGetPixelFormatType(pixels) {
        case kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange:
            guard let base = CVPixelBufferGetBaseAddressOfPlane(pixels, 0) else { return nil }
            return LumaGrid.fromLuma(base.assumingMemoryBound(to: UInt8.self),
                                     width: CVPixelBufferGetWidthOfPlane(pixels, 0),
                                     height: CVPixelBufferGetHeightOfPlane(pixels, 0),
                                     bytesPerRow: CVPixelBufferGetBytesPerRowOfPlane(pixels, 0))
        case kCVPixelFormatType_32BGRA:
            guard let base = CVPixelBufferGetBaseAddress(pixels) else { return nil }
            return LumaGrid.fromBGRA(base.assumingMemoryBound(to: UInt8.self),
                                     width: CVPixelBufferGetWidth(pixels), height: CVPixelBufferGetHeight(pixels),
                                     bytesPerRow: CVPixelBufferGetBytesPerRow(pixels))
        default:
            return nil
        }
    }

    static func orientation(of sampleBuffer: CMSampleBuffer) -> CGImagePropertyOrientation? {
        guard let value = CMGetAttachment(sampleBuffer, key: RPVideoSampleOrientationKey as CFString, attachmentModeOut: nil) as? NSNumber else {
            return nil
        }
        return CGImagePropertyOrientation(rawValue: value.uint32Value)
    }

    /// Downscale to `maxSide`, encode as JPEG, write `f-<ms>.jpg`. Returns false if nothing was written.
    private func save(_ pixels: CVPixelBuffer, orientation: CGImagePropertyOrientation?) -> Bool {
        var image = CIImage(cvPixelBuffer: pixels)
        if let orientation { image = image.oriented(orientation) }
        let longest = max(image.extent.width, image.extent.height)
        if longest > Self.maxSide {
            let scale = Self.maxSide / longest
            image = image.transformed(by: CGAffineTransform(scaleX: scale, y: scale))
        }
        guard let space = CGColorSpace(name: CGColorSpace.sRGB),
              let data = imageContext.jpegRepresentation(
                  of: image, colorSpace: space,
                  options: [CIImageRepresentationOption(rawValue: kCGImageDestinationLossyCompressionQuality as String): Self.jpegQuality])
        else { return false }
        let url = folder.appending(path: ScreenCaptureContract.frameName(at: Date()))
        do {
            try data.write(to: url, options: .atomic)
            return true
        } catch {
            return false
        }
    }

    // MARK: App audio

    /// "Transcribe what's playing": only when the app allowed it for this broadcast.
    func appAudio(_ sampleBuffer: CMSampleBuffer) {
        guard appAudioAllowed, isTaking else { return }
        audioQueue.sync { append(sampleBuffer) }
    }

    private func append(_ sampleBuffer: CMSampleBuffer) {
        if writer == nil { openPart(for: sampleBuffer) }
        guard let writer, let writerInput, writer.status == .writing else { return }
        if writerInput.isReadyForMoreMediaData { writerInput.append(sampleBuffer) }
    }

    /// A new AAC file starting at this buffer.
    private func openPart(for sampleBuffer: CMSampleBuffer) {
        guard let format = CMSampleBufferGetFormatDescription(sampleBuffer),
              let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(format)?.pointee else { return }
        let channels = min(2, max(1, Int(asbd.mChannelsPerFrame)))
        let rate = asbd.mSampleRate > 0 ? min(asbd.mSampleRate, 48_000) : 44_100
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: rate,
            AVNumberOfChannelsKey: channels,
            AVEncoderBitRateKey: channels == 1 ? 64_000 : 96_000,
        ]
        partIndex += 1
        let name = ScreenCaptureContract.audioPartName(partIndex)
        let url = folder.appending(path: name)
        do {
            let writer = try AVAssetWriter(outputURL: url, fileType: .m4a)
            let input = AVAssetWriterInput(mediaType: .audio, outputSettings: settings, sourceFormatHint: format)
            input.expectsMediaDataInRealTime = true
            guard writer.canAdd(input) else { return }
            writer.add(input)
            guard writer.startWriting() else { return }
            writer.startSession(atSourceTime: CMSampleBufferGetPresentationTimeStamp(sampleBuffer))
            self.writer = writer
            self.writerInput = input
            update(force: true) { $0.parts.append(.init(file: name, start: Date(), end: nil)) }
        } catch {
            try? FileManager.default.removeItem(at: url)
        }
    }

    /// Finish the open file (after a pause and at the end). A file that
    /// couldn't be completed is deleted and dropped from the manifest.
    private func closePart() {
        guard let writer, let writerInput else { return }
        self.writer = nil
        self.writerInput = nil
        let name = writer.outputURL.lastPathComponent
        var completed = false
        if writer.status == .writing {
            writerInput.markAsFinished()
            let done = DispatchSemaphore(value: 0)
            writer.finishWriting { done.signal() }
            _ = done.wait(timeout: .now() + 5)
            completed = writer.status == .completed
        }
        if !completed {
            writer.cancelWriting()
            try? FileManager.default.removeItem(at: writer.outputURL)
        }
        update(force: true) { m in
            guard let i = m.parts.firstIndex(where: { $0.file == name }) else { return }
            if completed { m.parts[i].end = Date() } else { m.parts.remove(at: i) }
        }
    }

    // MARK: Manifest

    private func startHeartbeat() {
        let timer = DispatchSource.makeTimerSource(queue: DispatchQueue.global(qos: .utility))
        timer.schedule(deadline: .now() + ScreenCaptureContract.heartbeat, repeating: ScreenCaptureContract.heartbeat)
        timer.setEventHandler { [weak self] in self?.update(force: false) { _ in } }
        timer.resume()
        heartbeat = timer
    }

    /// Change the manifest; write it when forced or once per heartbeat.
    private func update(force: Bool, _ change: (inout ScreenCaptureManifest) -> Void) {
        lock.withLock {
            change(&manifest)
            let now = Date()
            guard force || now.timeIntervalSince(lastWrite) >= ScreenCaptureContract.heartbeat else { return }
            manifest.heartbeat = now
            lastWrite = now
            try? manifest.write(to: folder)
        }
    }
}
