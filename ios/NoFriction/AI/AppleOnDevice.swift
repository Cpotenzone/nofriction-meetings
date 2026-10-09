import Foundation
#if canImport(FoundationModels)
import FoundationModels
#endif

/// Apple's on-device model (iOS 26+, Apple Intelligence on). No key, and
/// nothing leaves the device.
enum AppleOnDevice {
    /// The on-device model's window (prompt + answer).
    static let contextTokens = 4096
    static let maxAnswerTokens = 1200

    static var isAvailable: Bool { unavailableReason == nil }

    /// nil when the model is ready; otherwise why it can't be used.
    static var unavailableReason: String? {
        #if canImport(FoundationModels)
        if #available(iOS 26, *) {
            switch SystemLanguageModel.default.availability {
            case .available:
                return nil
            case .unavailable(let reason):
                switch reason {
                case .deviceNotEligible: return "This device doesn't support Apple Intelligence."
                case .appleIntelligenceNotEnabled: return "Turn on Apple Intelligence in Settings to use the on-device model."
                case .modelNotReady: return "Apple's on-device model is still downloading. Try again later."
                @unknown default: return "Apple's on-device model isn't available."
                }
            }
        }
        #endif
        return "Apple's on-device model needs iOS 26 or later."
    }

    static func complete(_ messages: [ChatMessage], maxTokens: Int, temperature: Double) async throws -> String {
        if let reason = unavailableReason { throw AIError.onDevice(reason) }
        #if canImport(FoundationModels)
        if #available(iOS 26, *) {
            let answer = min(maxTokens, maxAnswerTokens)
            var context = contextTokens
            for attempt in 0..<2 {
                let fitted = ContextFit.fit(messages, contextTokens: context, maxTokens: answer)
                let instructions = fitted.filter { $0.role == "system" }.map(\.content).joined(separator: "\n\n")
                let prompt = fitted.filter { $0.role != "system" }.map(\.content).joined(separator: "\n\n")
                let session = LanguageModelSession(instructions: instructions.isEmpty ? nil : instructions)
                do {
                    let response = try await session.respond(
                        to: prompt,
                        options: GenerationOptions(temperature: temperature, maximumResponseTokens: answer))
                    let text = response.content.trimmingCharacters(in: .whitespacesAndNewlines)
                    if text.isEmpty { throw AIError.emptyAnswer }
                    return text
                } catch let error as LanguageModelSession.GenerationError {
                    if case .exceededContextWindowSize = error, attempt == 0 {
                        // Tokenizer counts differ from our estimate; shrink and retry once
                        context = answer + (contextTokens - answer) / 2
                        continue
                    }
                    if case .guardrailViolation = error { throw AIError.refused }
                    throw AIError.onDevice(message(for: error))
                }
            }
        }
        #endif
        throw AIError.onDevice("Apple's on-device model isn't available.")
    }

    #if canImport(FoundationModels)
    /// Plain words for a generation error. `localizedDescription` alone read
    /// "The operation couldn't be completed. (FoundationModels.
    /// LanguageModelSession.GenerationError error -1.)".
    @available(iOS 26, *)
    static func message(for error: LanguageModelSession.GenerationError) -> String {
        switch error {
        case .assetsUnavailable:
            return "Apple's on-device model isn't ready. Check that Apple Intelligence is on and has finished downloading, then try again."
        case .unsupportedLanguageOrLocale:
            return "Apple's on-device model doesn't support this recording's language yet."
        case .rateLimited, .concurrentRequests:
            return "Apple's on-device model is busy. Try again in a moment."
        case .exceededContextWindowSize:
            return "This recording is too long for Apple's on-device model in one pass. Try again, or connect another AI in Settings."
        default:
            if let text = error.errorDescription, !text.isEmpty, !text.contains("GenerationError") { return text }
            return "Apple's on-device model couldn't finish. Try again, or connect another AI in Settings."
        }
    }
    #endif
}
