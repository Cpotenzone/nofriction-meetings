import AVFoundation
import Foundation
import Speech

/// Transcribing a recorded file (Apple Watch imports), as opposed to the live
/// microphone engines in Transcription/. Both recognizers run on the device
/// only; the Speech framework doesn't exist on watchOS, so the watch sends
/// its audio here.

/// A line recognized from a file. Times are seconds into that file; word
/// timings use UTF-16 offsets into `text`.
struct TranscribedLine: Equatable, Sendable {
    var text: String
    var start: Double
    var end: Double
    var words: [WordTiming]
}

protocol FileTranscriber: Sendable {
    var name: String { get }
    /// Longest stretch of audio handed over in one call
    var preferredChunkSeconds: Double { get }
    /// Permissions / model. `interactive` false: never show a prompt (the
    /// app is in the background) — throws `FileTranscriptionError.needsForeground`.
    func prepare(interactive: Bool) async throws
    func transcribe(fileAt url: URL, vocabulary: [String]) async throws -> [TranscribedLine]
}

enum FileTranscriptionError: LocalizedError, Equatable {
    /// Speech permission hasn't been asked yet and the app isn't on screen
    case needsForeground

    var errorDescription: String? {
        "Open noFriction to transcribe this recording."
    }
}

enum FileTranscribers {
    /// The best on-device file transcriber this OS offers.
    static func best() -> FileTranscriber {
        if #available(iOS 26.0, *), SpeechTranscriber.isAvailable {
            return AnalyzerFileTranscriber()
        }
        return RecognizerFileTranscriber()
    }
}

// MARK: - iOS 26+: SpeechAnalyzer

@available(iOS 26.0, *)
final class AnalyzerFileTranscriber: FileTranscriber {
    let name = "Apple on-device (SpeechAnalyzer)"
    /// Long-form model: big chunks are fine; chunking is for checkpoints
    let preferredChunkSeconds = 300.0

    func prepare(interactive: Bool) async throws {}

    func transcribe(fileAt url: URL, vocabulary: [String]) async throws -> [TranscribedLine] {
        var matched = await SpeechTranscriber.supportedLocale(equivalentTo: Locale.current)
        if matched == nil { matched = await SpeechTranscriber.supportedLocale(equivalentTo: Locale(identifier: "en-US")) }
        guard let locale = matched else { throw TranscriptionError.localeUnsupported(Locale.current.identifier) }

        let transcriber = SpeechTranscriber(locale: locale, transcriptionOptions: [], reportingOptions: [],
                                            attributeOptions: [.audioTimeRange])
        if let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
            try await request.downloadAndInstall()
        }
        let analyzer = SpeechAnalyzer(modules: [transcriber])
        if !vocabulary.isEmpty {
            let context = AnalysisContext()
            context.contextualStrings[.general] = vocabulary
            try? await analyzer.setContext(context)
        }
        let collector = Task { () throws -> [TranscribedLine] in
            var lines: [TranscribedLine] = []
            for try await result in transcriber.results where result.isFinal {
                let text = String(result.text.characters).trimmingCharacters(in: .whitespacesAndNewlines)
                guard !text.isEmpty else { continue }
                let start = result.range.start.seconds.isFinite ? result.range.start.seconds : 0
                let length = result.range.duration.seconds.isFinite ? result.range.duration.seconds : 0
                let words = AppleSpeechAnalyzerEngine.wordTimings(result.text, trimmedTo: text)
                let split = LineSplitter.split(text: text, words: words)
                lines += split.isEmpty ? [TranscribedLine(text: text, start: start, end: start + length, words: [])] : split
            }
            return lines
        }
        do {
            let file = try AVAudioFile(forReading: url)
            if let end = try await analyzer.analyzeSequence(from: file) {
                try await analyzer.finalizeAndFinish(through: end)
            } else {
                await analyzer.cancelAndFinishNow()
            }
        } catch {
            collector.cancel()
            await analyzer.cancelAndFinishNow()
            throw error
        }
        return try await collector.value
    }
}

// MARK: - iOS 18–25: SFSpeechRecognizer, forced on-device

final class RecognizerFileTranscriber: FileTranscriber {
    let name = "Apple on-device (Speech Recognizer)"
    /// Dictation recognizer: stay under a minute per request
    let preferredChunkSeconds = 55.0

    func prepare(interactive: Bool) async throws {
        switch SFSpeechRecognizer.authorizationStatus() {
        case .authorized:
            return
        case .notDetermined:
            guard interactive else { throw FileTranscriptionError.needsForeground }
            let status = await withCheckedContinuation { cont in
                SFSpeechRecognizer.requestAuthorization { cont.resume(returning: $0) }
            }
            guard status == .authorized else { throw TranscriptionError.notAuthorized }
        default:
            throw TranscriptionError.notAuthorized
        }
    }

    func transcribe(fileAt url: URL, vocabulary: [String]) async throws -> [TranscribedLine] {
        guard let recognizer = SFSpeechRecognizer(locale: Locale.current) ?? SFSpeechRecognizer(locale: Locale(identifier: "en-US")),
              recognizer.supportsOnDeviceRecognition else {
            throw TranscriptionError.onDeviceUnavailable
        }
        let request = SFSpeechURLRecognitionRequest(url: url)
        request.requiresOnDeviceRecognition = true
        request.shouldReportPartialResults = false
        request.addsPunctuation = true
        request.taskHint = .dictation
        request.contextualStrings = Array(vocabulary.prefix(100))

        let once = Once()
        let transcription: SFTranscription? = try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { (cont: CheckedContinuation<SFTranscription?, Error>) in
                let task = recognizer.recognitionTask(with: request) { result, error in
                    if let result, result.isFinal {
                        if once.claim() { cont.resume(returning: result.bestTranscription) }
                    } else if let error {
                        // 1110: no speech in this stretch — an empty chunk, not a failure
                        let nsError = error as NSError
                        if once.claim() {
                            if nsError.code == 1110 { cont.resume(returning: nil) } else { cont.resume(throwing: error) }
                        }
                    }
                }
                once.task = task
            }
        } onCancel: {
            once.task?.cancel()
        }
        guard let transcription else { return [] }
        let words = transcription.segments.compactMap { seg -> WordTiming? in
            guard seg.timestamp >= 0, seg.duration >= 0 else { return nil }
            return WordTiming(location: seg.substringRange.location, length: seg.substringRange.length,
                              start: seg.timestamp, end: seg.timestamp + seg.duration)
        }
        return LineSplitter.split(text: transcription.formattedString, words: words)
    }

    /// Resume a continuation exactly once; the handler can fire again after the final result.
    private final class Once: @unchecked Sendable {
        private let lock = NSLock()
        private var done = false
        var task: SFSpeechRecognitionTask? {
            get { lock.withLock { _task } }
            set { lock.withLock { _task = newValue } }
        }
        private var _task: SFSpeechRecognitionTask?

        func claim() -> Bool {
            lock.withLock {
                guard !done else { return false }
                done = true
                return true
            }
        }
    }
}

// MARK: - Splitting a file's text into transcript lines

/// A recognizer returns one long text for a file; the transcript wants
/// lines. Break at pauses and sentence ends, the way people read a transcript.
enum LineSplitter {
    /// Silence this long between words always starts a new line
    static let pauseBreak = 1.2
    /// After a sentence end, a shorter pause is enough once the line has some words
    static let sentencePause = 0.4
    static let sentenceMinWords = 6
    /// Keep lines readable
    static let maxWords = 45

    static func split(text: String, words: [WordTiming]) -> [TranscribedLine] {
        let ns = text as NSString
        let sorted = words.filter { $0.location >= 0 && $0.length > 0 && $0.location + $0.length <= ns.length }
            .sorted { $0.location < $1.location }
        guard !sorted.isEmpty else { return [] }

        var groups: [[WordTiming]] = []
        var current: [WordTiming] = []
        for w in sorted {
            if let last = current.last {
                let gap = w.start - last.end
                let lastText = ns.substring(with: last.range)
                let sentenceEnd = lastText.last.map { ".?!…".contains($0) } ?? false
                let clauseEnd = lastText.last.map { ",;:".contains($0) } ?? false
                if gap >= pauseBreak
                    || (sentenceEnd && gap >= sentencePause && current.count >= sentenceMinWords)
                    || (current.count >= maxWords && (sentenceEnd || clauseEnd))
                    || current.count >= maxWords * 2 {
                    groups.append(current)
                    current = []
                }
            }
            current.append(w)
        }
        if !current.isEmpty { groups.append(current) }

        return groups.compactMap { group in
            guard let first = group.first, let last = group.last else { return nil }
            let from = first.location
            let to = last.location + last.length
            // Keep trailing punctuation that follows the last word directly
            var end = to
            while end < ns.length, let scalar = Unicode.Scalar(ns.character(at: end)),
                  CharacterSet.punctuationCharacters.contains(scalar) { end += 1 }
            let lineText = ns.substring(with: NSRange(location: from, length: end - from))
            let rebased = group.map { WordTiming(location: $0.location - from, length: $0.length, start: $0.start, end: $0.end) }
            return TranscribedLine(text: lineText, start: first.start, end: max(last.end, first.start), words: rebased)
        }
    }
}

// MARK: - Audio: levels, chunk plan, chunk export

enum AudioChunks {
    /// Level per window, 0…1 (roughly -50…0 dBFS, the live meter's scale).
    struct Levels: Sendable {
        var values: [Float]
        var window: Double
        var duration: Double

        /// Loudest window overlapping `range` (seconds); nil outside the file.
        func peak(_ range: ClosedRange<Double>) -> Float? {
            guard window > 0, !values.isEmpty else { return nil }
            let a = max(0, Int((range.lowerBound / window).rounded(.down)))
            let b = min(values.count - 1, Int((range.upperBound / window).rounded(.down)))
            guard a <= b else { return nil }
            return values[a...b].max()
        }
    }

    static func levels(of url: URL, window: Double = 0.1) throws -> Levels {
        let file = try AVAudioFile(forReading: url)
        let format = file.processingFormat
        let rate = format.sampleRate
        let perWindow = max(1, AVAudioFrameCount(rate * window))
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: perWindow * 64) else {
            throw AudioSilencer.Failure.unreadable("no buffer")
        }
        var values: [Float] = []
        var sum: Float = 0
        var count = 0
        while file.framePosition < file.length {
            try file.read(into: buffer, frameCount: buffer.frameCapacity)
            let n = Int(buffer.frameLength)
            if n == 0 { break }
            guard let data = buffer.floatChannelData?[0] else { break }
            for i in 0..<n {
                sum += data[i] * data[i]
                count += 1
                if count == Int(perWindow) {
                    values.append(level(rms: sqrt(sum / Float(count))))
                    sum = 0
                    count = 0
                }
            }
        }
        if count > 0 { values.append(level(rms: sqrt(sum / Float(count)))) }
        return Levels(values: values, window: window, duration: Double(file.length) / rate)
    }

    static func level(rms: Float) -> Float {
        let db = 20 * log10(max(rms, 1e-6))
        return max(0, min(1, (db + 50) / 50))
    }

    /// Chunks from `from` to the end, each at most `maxChunk` long. Each cut
    /// is moved to the quietest window in the `search` seconds before the
    /// limit, so words are rarely split across chunks.
    static func plan(from: Double, duration: Double, maxChunk: Double, levels: Levels?, search: Double = 10) -> [Range<Double>] {
        guard duration > from, maxChunk > 0 else { return [] }
        var chunks: [Range<Double>] = []
        var start = max(0, from)
        while duration - start > 0.05 {
            var end = min(duration, start + maxChunk)
            if end < duration, let levels, levels.window > 0, !levels.values.isEmpty {
                let lo = max(start + maxChunk / 2, end - search)
                let a = Int((lo / levels.window).rounded(.up))
                let b = min(levels.values.count - 1, Int((end / levels.window).rounded(.down)) - 1)
                if a <= b {
                    // Quietest window; the latest among equals keeps chunks long
                    var best = b
                    for i in stride(from: b, through: a, by: -1) where levels.values[i] < levels.values[best] { best = i }
                    end = (Double(best) + 0.5) * levels.window
                }
            }
            chunks.append(start..<end)
            start = end
        }
        return chunks
    }

    /// Write `range` (seconds) of `source` as 16-bit PCM CAF at `destination`.
    static func export(_ source: URL, range: Range<Double>, to destination: URL) throws {
        let input = try AVAudioFile(forReading: source)
        let format = input.processingFormat
        let rate = format.sampleRate
        let first = AVAudioFramePosition((range.lowerBound * rate).rounded(.down))
        let last = min(input.length, AVAudioFramePosition((range.upperBound * rate).rounded(.up)))
        guard first < last else { throw AudioSilencer.Failure.unreadable("empty range") }
        input.framePosition = first
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatLinearPCM,
            AVSampleRateKey: rate,
            AVNumberOfChannelsKey: format.channelCount,
            AVLinearPCMBitDepthKey: 16,
            AVLinearPCMIsFloatKey: false,
            AVLinearPCMIsBigEndianKey: false,
        ]
        let output = try AVAudioFile(forWriting: destination, settings: settings,
                                     commonFormat: format.commonFormat, interleaved: format.isInterleaved)
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 32_768) else {
            throw AudioSilencer.Failure.unreadable("no buffer")
        }
        var remaining = last - first
        while remaining > 0 {
            let want = AVAudioFrameCount(min(AVAudioFramePosition(buffer.frameCapacity), remaining))
            try input.read(into: buffer, frameCount: want)
            if buffer.frameLength == 0 { break }
            try output.write(from: buffer)
            remaining -= AVAudioFramePosition(buffer.frameLength)
        }
    }
}
