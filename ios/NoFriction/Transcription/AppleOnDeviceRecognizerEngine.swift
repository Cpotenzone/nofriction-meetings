import AVFoundation
import Foundation
import Speech

/// iOS 18 fallback: SFSpeechRecognizer forced on-device. It was designed
/// for dictation, not hour-long meetings — a single task accumulates text
/// until it ends — so this engine commits text at natural pauses: when the
/// hypothesis stops changing, the current task is ended (finalizing that
/// stretch) and a fresh one takes over without dropping audio.
final class AppleOnDeviceRecognizerEngine: TranscriptionEngine, @unchecked Sendable {
    let name = "Apple on-device (Speech Recognizer)"

    /// No new words for this long ⇒ commit the current stretch.
    private let pauseToCommit: TimeInterval = 1.2
    /// Tasks are also rotated after this long, well under system limits.
    private let maxTaskLength: TimeInterval = 45

    private var recognizer: SFSpeechRecognizer?
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var taskStartedAt = Date()
    private var lastText = ""
    private var lastChange = Date()
    private var vocabulary: [String] = []
    private var events: AsyncStream<TranscriptEvent>.Continuation?
    private var watchdog: Task<Void, Never>?
    private var running = false
    private let lock = NSLock()
    /// Audio fed so far (frames) — the same buffers the AAC file gets, so
    /// frames / sampleRate is a position in the recording
    private var framesAppended: AVAudioFramePosition = 0
    private var sampleRate: Double = 0
    /// Where the current task's audio starts in the recording (seconds)
    private var taskAudioStart: Double = 0
    /// Word timings of the latest hypothesis, already in recording seconds
    private var lastWords: [WordTiming] = []

    func start(format: AVAudioFormat, vocabulary: [String]) async throws -> AsyncStream<TranscriptEvent> {
        let status = await withCheckedContinuation { cont in
            SFSpeechRecognizer.requestAuthorization { cont.resume(returning: $0) }
        }
        guard status == .authorized else { throw TranscriptionError.notAuthorized }

        guard let recognizer = SFSpeechRecognizer(locale: Locale.current) ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US")),
              recognizer.supportsOnDeviceRecognition else {
            throw TranscriptionError.onDeviceUnavailable
        }

        let (stream, continuation) = AsyncStream<TranscriptEvent>.makeStream()
        lock.withLock {
            self.recognizer = recognizer
            self.vocabulary = vocabulary
            self.events = continuation
            self.running = true
            self.framesAppended = 0
            self.sampleRate = format.sampleRate
        }
        beginTask()

        // Commit on pauses / rotate long tasks
        watchdog = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(300))
                self?.commitIfPaused()
            }
        }
        return stream
    }

    func append(_ buffer: AVAudioPCMBuffer) {
        lock.withLock { () -> SFSpeechAudioBufferRecognitionRequest? in
            framesAppended += AVAudioFramePosition(buffer.frameLength)
            return request
        }?.append(buffer)
    }

    func stop() async {
        watchdog?.cancel()
        let (task, request) = lock.withLock { () -> (SFSpeechRecognitionTask?, SFSpeechAudioBufferRecognitionRequest?) in
            running = false
            return (self.task, self.request)
        }
        request?.endAudio()
        // Give the final result a moment to arrive, then close up
        try? await Task.sleep(for: .milliseconds(800))
        commit(force: true)
        task?.cancel()
        lock.withLock {
            events?.finish()
            events = nil
            self.task = nil
            self.request = nil
        }
    }

    // MARK: - Task lifecycle

    private func beginTask() {
        let request = SFSpeechAudioBufferRecognitionRequest()
        request.requiresOnDeviceRecognition = true
        request.shouldReportPartialResults = true
        request.addsPunctuation = true
        request.taskHint = .dictation

        let (recognizer, vocabulary) = lock.withLock { (self.recognizer, self.vocabulary) }
        request.contextualStrings = Array(vocabulary.prefix(100))
        guard let recognizer else { return }

        let started = Date()
        let task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            guard let self else { return }
            if let result {
                // Per-word times relative to this task's audio (0 until known)
                let words = result.bestTranscription.segments.compactMap { seg -> WordTiming? in
                    guard seg.timestamp > 0 || seg.duration > 0 else { return nil }
                    return WordTiming(location: seg.substringRange.location, length: seg.substringRange.length,
                                      start: seg.timestamp, end: seg.timestamp + seg.duration)
                }
                self.handle(text: result.bestTranscription.formattedString, words: words, isFinal: result.isFinal, taskStartedAt: started)
            } else if let error {
                // An error right after starting means the recognizer can't
                // run at all; don't spin restarting it
                if Date().timeIntervalSince(started) < 1 {
                    self.lock.withLock {
                        self.running = false
                        self.events?.yield(.status(error.localizedDescription))
                    }
                    return
                }
                // Errors after endAudio() (e.g. "no speech") are expected
                self.handle(text: "", words: [], isFinal: true, taskStartedAt: started)
            }
        }

        lock.withLock {
            self.request = request
            self.task = task
            self.taskStartedAt = started
            self.taskAudioStart = self.sampleRate > 0 ? Double(self.framesAppended) / self.sampleRate : 0
            self.lastText = ""
            self.lastWords = []
            self.lastChange = Date()
        }
    }

    private func handle(text: String, words: [WordTiming], isFinal: Bool, taskStartedAt: Date) {
        let continuation: AsyncStream<TranscriptEvent>.Continuation? = lock.withLock {
            // Ignore late callbacks from a task we've already rotated away from
            guard taskStartedAt == self.taskStartedAt else { return nil }
            if text != lastText {
                lastText = text
                lastChange = Date()
                lastWords = []
            }
            if !words.isEmpty {
                let base = taskAudioStart
                lastWords = words.map { WordTiming(location: $0.location, length: $0.length, start: base + $0.start, end: base + $0.end) }
            }
            return events
        }
        guard let continuation else { return }
        if isFinal {
            commit(force: true)
        } else {
            continuation.yield(.partial(text))
        }
    }

    private func commitIfPaused() {
        let (text, lastChange, startedAt, running) = lock.withLock { (lastText, self.lastChange, taskStartedAt, self.running) }
        guard running, !text.isEmpty else { return }
        let paused = Date().timeIntervalSince(lastChange) >= pauseToCommit
        let tooLong = Date().timeIntervalSince(startedAt) >= maxTaskLength
        if paused || tooLong { commit(force: false) }
    }

    /// Emit the current task's text as final and hand audio to a new task.
    private func commit(force _: Bool) {
        let snapshot: (String, Date, AsyncStream<TranscriptEvent>.Continuation?, SFSpeechAudioBufferRecognitionRequest?, SFSpeechRecognitionTask?, Bool, Double, [WordTiming]) = lock.withLock {
            let out = (self.lastText, self.taskStartedAt, self.events, self.request, self.task, self.running, self.taskAudioStart, self.lastWords)
            lastText = ""
            lastWords = []
            return out
        }
        let (text, startedAt, continuation, oldRequest, oldTask, running, audioStart, words) = snapshot
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty {
            continuation?.yield(.final(text: trimmed, start: startedAt, duration: Date().timeIntervalSince(startedAt),
                                       audioOffset: audioStart, words: Self.rebase(words, from: text, to: trimmed)))
        }
        continuation?.yield(.partial(""))
        guard running else { return }
        // Swap in the new task first so no audio lands on the finished one
        beginTask()
        oldRequest?.endAudio()
        oldTask?.finish()
    }

    /// Shift UTF-16 offsets for the whitespace trimmed off the front; drop any out of range.
    static func rebase(_ words: [WordTiming], from text: String, to trimmed: String) -> [WordTiming] {
        let lead = (text as NSString).range(of: trimmed).location
        guard lead != NSNotFound else { return [] }
        let limit = (trimmed as NSString).length
        return words.compactMap { w in
            var w = w
            w.location -= lead
            return w.location >= 0 && w.location + w.length <= limit && w.end >= w.start ? w : nil
        }
    }
}
