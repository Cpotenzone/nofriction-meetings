import AVFoundation
import Foundation
import Speech

/// iOS 26+: Apple's SpeechAnalyzer + SpeechTranscriber. Built for long-form,
/// far-field audio like meetings, fully on-device, streaming "volatile"
/// (live) results that are later replaced by final ones — the same
/// partial → final model the Mac app uses.
@available(iOS 26.0, *)
final class AppleSpeechAnalyzerEngine: TranscriptionEngine, @unchecked Sendable {
    let name = "Apple on-device (SpeechAnalyzer)"

    static var isAvailable: Bool { SpeechTranscriber.isAvailable }

    private var analyzer: SpeechAnalyzer?
    private var inputContinuation: AsyncStream<AnalyzerInput>.Continuation?
    private var resultsTask: Task<Void, Never>?
    private var converter: AVAudioConverter?
    private var analyzerFormat: AVAudioFormat?
    private let lock = NSLock()

    func start(format: AVAudioFormat, vocabulary: [String]) async throws -> AsyncStream<TranscriptEvent> {
        let (events, eventContinuation) = AsyncStream<TranscriptEvent>.makeStream()

        var matched = await SpeechTranscriber.supportedLocale(equivalentTo: Locale.current)
        if matched == nil {
            matched = await SpeechTranscriber.supportedLocale(equivalentTo: Locale(identifier: "en-US"))
        }
        guard let locale = matched else {
            throw TranscriptionError.localeUnsupported(Locale.current.identifier)
        }

        let transcriber = SpeechTranscriber(
            locale: locale,
            transcriptionOptions: [],
            reportingOptions: [.volatileResults, .fastResults],
            attributeOptions: [.audioTimeRange]
        )

        // One-time model download (managed by the OS, shared across apps)
        if let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
            eventContinuation.yield(.status("Downloading the on-device speech model…"))
            try await request.downloadAndInstall()
            eventContinuation.yield(.status(nil))
        }

        let analyzerFormat = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber])
        let analyzer = SpeechAnalyzer(modules: [transcriber])

        if !vocabulary.isEmpty {
            let context = AnalysisContext()
            context.contextualStrings[.general] = vocabulary
            try? await analyzer.setContext(context)
        }

        let (inputs, inputContinuation) = AsyncStream<AnalyzerInput>.makeStream()
        lock.withLock {
            self.analyzer = analyzer
            self.inputContinuation = inputContinuation
            self.analyzerFormat = analyzerFormat
            if let analyzerFormat, analyzerFormat != format {
                self.converter = AVAudioConverter(from: format, to: analyzerFormat)
            }
        }

        let sessionStart = Date()
        resultsTask = Task {
            do {
                for try await result in transcriber.results {
                    let text = String(result.text.characters).trimmingCharacters(in: .whitespacesAndNewlines)
                    if result.isFinal {
                        guard !text.isEmpty else { continue }
                        let start = result.range.start.seconds.isFinite ? result.range.start.seconds : 0
                        let duration = result.range.duration.seconds.isFinite ? result.range.duration.seconds : 0
                        eventContinuation.yield(.final(
                            text: text,
                            start: sessionStart.addingTimeInterval(start),
                            duration: duration,
                            // The analyzer's timeline is the audio we fed it == the recording
                            audioOffset: start,
                            words: Self.wordTimings(result.text, trimmedTo: text)
                        ))
                        eventContinuation.yield(.partial(""))
                    } else {
                        eventContinuation.yield(.partial(text))
                    }
                }
            } catch {
                eventContinuation.yield(.status(error.localizedDescription))
            }
            eventContinuation.finish()
        }

        try await analyzer.start(inputSequence: inputs)
        return events
    }

    func append(_ buffer: AVAudioPCMBuffer) {
        let (continuation, converter, target) = lock.withLock { (inputContinuation, self.converter, analyzerFormat) }
        guard let continuation else { return }
        if let converter, let target {
            guard let converted = Self.convert(buffer, with: converter, to: target) else { return }
            continuation.yield(AnalyzerInput(buffer: converted))
        } else {
            continuation.yield(AnalyzerInput(buffer: buffer))
        }
    }

    func stop() async {
        let (analyzer, continuation) = lock.withLock { (self.analyzer, inputContinuation) }
        continuation?.finish()
        try? await analyzer?.finalizeAndFinishThroughEndOfInput()
        await resultsTask?.value
        lock.withLock {
            self.analyzer = nil
            self.inputContinuation = nil
            self.converter = nil
        }
    }

    /// Per-run audio time ranges (`.audioTimeRange` attribute) → word timings
    /// with UTF-16 offsets into the trimmed text.
    static func wordTimings(_ text: AttributedString, trimmedTo trimmed: String) -> [WordTiming] {
        let full = String(text.characters)
        let lead = (full as NSString).range(of: trimmed).location
        guard lead != NSNotFound else { return [] }
        let limit = (trimmed as NSString).length
        var out: [WordTiming] = []
        for run in text.runs {
            guard let range = run.audioTimeRange else { continue }
            let a = range.start.seconds, b = range.end.seconds
            guard a.isFinite, b.isFinite else { continue }
            let before = String(text.characters[text.startIndex..<run.range.lowerBound]) as NSString
            let piece = String(text.characters[run.range]) as NSString
            // Tighten to the non-whitespace part of the run
            let core = piece.range(of: "\\S(.*\\S)?", options: .regularExpression)
            guard core.location != NSNotFound else { continue }
            let location = before.length + core.location - lead
            guard location >= 0, location + core.length <= limit else { continue }
            out.append(WordTiming(location: location, length: core.length, start: a, end: b))
        }
        return out
    }

    private static func convert(_ buffer: AVAudioPCMBuffer, with converter: AVAudioConverter, to format: AVAudioFormat) -> AVAudioPCMBuffer? {
        let ratio = format.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount((Double(buffer.frameLength) * ratio).rounded(.up)) + 16
        guard let out = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity) else { return nil }
        var consumed = false
        var error: NSError?
        converter.convert(to: out, error: &error) { _, status in
            if consumed {
                status.pointee = .noDataNow
                return nil
            }
            consumed = true
            status.pointee = .haveData
            return buffer
        }
        return error == nil && out.frameLength > 0 ? out : nil
    }
}
