import AVFoundation
import Foundation

/// What the UI and store receive from a transcription engine.
enum TranscriptEvent: Sendable {
    /// Words still being spoken; replaces the previous partial.
    case partial(String)
    /// Settled text. `start` is wall-clock time of the first word.
    /// `audioOffset` is where it starts in the recording (seconds of audio
    /// fed to the engine, which is exactly what the AAC file holds), and
    /// `words` are per-word timings when the engine provides them — used to
    /// silence exactly the words a user deletes or strikes.
    case final(text: String, start: Date, duration: TimeInterval, audioOffset: TimeInterval?, words: [WordTiming])
    /// Engine-level problem worth showing (e.g. model downloading, denied).
    case status(String?)
}

/// An on-device speech-to-text engine. Audio buffers arrive from the
/// microphone tap on a realtime thread, so `append` must be thread-safe and
/// must not block.
protocol TranscriptionEngine: AnyObject, Sendable {
    /// Human name for Settings ("Apple on-device (SpeechAnalyzer)").
    var name: String { get }
    /// Begin a session. `vocabulary` (attendee names, company names from the
    /// calendar) is used as a recognition hint where the engine supports it.
    func start(format: AVAudioFormat, vocabulary: [String]) async throws -> AsyncStream<TranscriptEvent>
    func append(_ buffer: AVAudioPCMBuffer)
    /// Flush and finalize everything heard so far.
    func stop() async
}

enum TranscriptionEngines {
    /// The best engine this OS offers. Both are fully on-device.
    static func best() -> TranscriptionEngine {
        if #available(iOS 26.0, *), AppleSpeechAnalyzerEngine.isAvailable {
            return AppleSpeechAnalyzerEngine()
        }
        return AppleOnDeviceRecognizerEngine()
    }
}

enum TranscriptionError: LocalizedError {
    case notAuthorized
    case onDeviceUnavailable
    case localeUnsupported(String)

    var errorDescription: String? {
        switch self {
        case .notAuthorized:
            return "Speech recognition is off. Turn it on in Settings → Privacy & Security → Speech Recognition."
        case .onDeviceUnavailable:
            return "On-device transcription isn't available for this language on this device."
        case .localeUnsupported(let id):
            return "Transcription doesn't support \(id) yet."
        }
    }
}
