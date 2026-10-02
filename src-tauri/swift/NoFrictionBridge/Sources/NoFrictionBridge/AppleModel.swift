// Apple on-device model (Foundation Models, macOS 26+ with Apple
// Intelligence on). Text only. Compiled into both flavors; the framework is
// weak-linked (build.rs) so the app still launches on macOS 12–15, where
// availability reports "requires_macos_26".

import Foundation
#if canImport(FoundationModels)
import FoundationModels
#endif

func nfAppleAvailability() -> (Bool, String) {
    #if canImport(FoundationModels)
    if #available(macOS 26.0, *) {
        switch SystemLanguageModel.default.availability {
        case .available:
            return (true, "")
        case .unavailable(let reason):
            switch reason {
            case .deviceNotEligible:
                return (false, "device_not_eligible")
            case .appleIntelligenceNotEnabled:
                return (false, "apple_intelligence_off")
            case .modelNotReady:
                return (false, "model_not_ready")
            @unknown default:
                return (false, "unavailable")
            }
        @unknown default:
            return (false, "unavailable")
        }
    }
    return (false, "requires_macos_26")
    #else
    return (false, "sdk_without_foundation_models")
    #endif
}

/// `{"available": bool, "reason": String}` (malloc'd; free with nf_free)
@_cdecl("nf_apple_model_availability")
public func nf_apple_model_availability() -> UnsafeMutablePointer<CChar>? {
    let (ok, reason) = nfAppleAvailability()
    return nfCString(["available": ok, "reason": reason])
}

/// One-shot generation. Result: `{"ok":true,"text":…}` or `{"ok":false,"error":…}`.
/// `temperature < 0` means "model default".
@_cdecl("nf_apple_generate")
public func nf_apple_generate(
    _ instructions: UnsafePointer<CChar>,
    _ prompt: UnsafePointer<CChar>,
    _ maxTokens: Int32,
    _ temperature: Double,
    _ ctx: UnsafeMutableRawPointer?,
    _ cb: NFCallback
) {
    let context = NFContext(ptr: ctx, cb: cb)
    let instructionsText = String(cString: instructions)
    let promptText = String(cString: prompt)
    #if canImport(FoundationModels)
    if #available(macOS 26.0, *) {
        Task.detached {
            do {
                let session = instructionsText.isEmpty
                    ? LanguageModelSession()
                    : LanguageModelSession(instructions: instructionsText)
                let options = GenerationOptions(
                    temperature: temperature >= 0 ? temperature : nil,
                    maximumResponseTokens: maxTokens > 0 ? Int(maxTokens) : nil
                )
                let response = try await session.respond(to: promptText, options: options)
                context.send(["ok": true, "text": response.content])
            } catch {
                context.fail(error)
            }
        }
        return
    }
    #endif
    context.send(["ok": false, "error": nfAppleAvailability().1])
}
