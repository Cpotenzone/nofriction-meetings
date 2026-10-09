import Foundation

struct ChatMessage: Codable, Sendable, Equatable {
    var role: String   // "system" | "user" | "assistant"
    var content: String
}

/// Everything needed for one call: resolved from AISettings + Keychain on the main actor.
struct AIEndpoint: Sendable {
    let provider: AIProvider
    let baseURL: URL?
    let apiKey: String?
    let model: String
    let contextTokens: Int
    let consentGranted: Bool

    var needsConsent: Bool { URLPolicy.needsConsent(provider: provider, baseURL: baseURL) }
}

struct ModelInfo: Codable, Sendable, Hashable {
    let id: String
    let contextTokens: Int?
}

enum AIError: LocalizedError, Equatable {
    case notConfigured
    case consentRequired(String)
    case insecureURL
    case wrongKey(String)
    case noCredit(String)
    case unreachable(String)
    case atsBlocked
    case server(Int, String)
    case emptyAnswer
    case truncated
    case tooLarge
    case refused
    case badResponse
    case onDevice(String)

    var errorDescription: String? {
        switch self {
        case .notConfigured: "Configure Apple on-device or enter your AI endpoint and model in Settings first."
        case .consentRequired(let p): "Allow sending recording content to \(p) first."
        case .insecureURL: "That address isn't allowed. Cloud services need https://. Plain http:// only works for this device, your local network or a Tailscale address."
        case .wrongKey(let m): "Wrong or revoked key." + (m.isEmpty ? "" : " \(m)")
        case .noCredit(let m): "No credit left, or rate-limited." + (m.isEmpty ? "" : " \(m)")
        case .unreachable(let m): "Can't reach the AI service." + (m.isEmpty ? "" : " \(m)")
        case .atsBlocked: "iOS blocks plain http:// to that host. Use https:// (Tailscale can issue a certificate with `tailscale cert`) or the machine's local IP address."
        case .server(let s, let m): "The AI service returned an error (\(s))." + (m.isEmpty ? "" : " \(m)")
        case .emptyAnswer: "The model returned no text. Try again, or pick another model in Settings."
        case .truncated: "The model ran out of room before answering. Try again, or pick another model in Settings."
        case .tooLarge: "The AI service sent back more than 4 MB. Stopped reading."
        case .refused: "The model declined this request."
        case .badResponse: "The AI service sent a response this app couldn't read."
        case .onDevice(let m): m
        }
    }
}

// MARK: - Context fitting

enum ContextFit {
    static let defaultContextTokens = 32_768
    static let charsPerToken = 3.2
    static let reserveTokens = 512
    static let marker = "\n\n[… middle of transcript trimmed to fit the model's context …]\n\n"

    static func budgetChars(contextTokens: Int, maxTokens: Int) -> Int {
        max(0, Int(Double(contextTokens - maxTokens - reserveTokens) * charsPerToken))
    }

    /// Trim the longest message in the middle so prompt + answer fit the window.
    static func fit(_ messages: [ChatMessage], contextTokens: Int = defaultContextTokens, maxTokens: Int) -> [ChatMessage] {
        let budget = budgetChars(contextTokens: contextTokens, maxTokens: maxTokens)
        let total = messages.reduce(0) { $0 + $1.content.count }
        guard total > budget,
              let i = messages.indices.max(by: { messages[$0].content.count < messages[$1].content.count })
        else { return messages }
        var out = messages
        let text = out[i].content
        let keep = max(0, text.count - (total - budget) - marker.count)
        let head = keep * 6 / 10
        out[i].content = String(text.prefix(head)) + marker + String(text.suffix(keep - head))
        return out
    }
}

// MARK: - Request building (pure; unit-tested)

enum RequestBuilder {
    static let maxOutputTokens = 16_000

    /// Total time allowed for one completion.
    static func timeout(maxTokens: Int) -> TimeInterval { 60 + Double(maxTokens) / 8 }

    /// Allow extra output budget when the configured model uses reasoning tokens.
    static func outputBudget(proto: AIProtocol, requested: Int, completionTokens: Bool) -> Int {
        switch proto {
        case .openai: completionTokens ? min(requested + 4096, maxOutputTokens) : requested
        case .foundationModels: requested
        }
    }

    static func chat(_ ep: AIEndpoint, messages: [ChatMessage], maxTokens: Int, temperature: Double?,
                     completionTokens: Bool = false) throws -> URLRequest {
        guard ep.provider == .custom, let base = ep.baseURL,
              !ep.model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { throw AIError.notConfigured }
        guard URLPolicy.isAllowed(base) else { throw AIError.insecureURL }
        switch ep.provider.proto {
        case .openai:
            var req = URLRequest(url: base.appending(path: "chat/completions"))
            req.httpMethod = "POST"
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
            if let key = ep.apiKey, !key.isEmpty { req.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization") }
            var body: [String: Any] = [
                "model": ep.model,
                "messages": messages.map { ["role": $0.role, "content": $0.content] },
                "stream": false,
            ]
            if completionTokens {
                body["max_completion_tokens"] = maxTokens
            } else {
                body["max_tokens"] = maxTokens
                if let temperature { body["temperature"] = temperature }
            }
            req.httpBody = try JSONSerialization.data(withJSONObject: body, options: [.sortedKeys])
            req.timeoutInterval = timeout(maxTokens: maxTokens)
            return req
        case .foundationModels:
            throw AIError.badResponse
        }
    }

    /// The connection test body: the single word "Hi" with a one-token answer.
    /// Never any recording content, so it needs no consent.
    static func probe(_ ep: AIEndpoint, completionTokens: Bool = false) throws -> URLRequest {
        var req = try chat(ep, messages: [ChatMessage(role: "user", content: "Hi")], maxTokens: 1,
                           temperature: nil, completionTokens: completionTokens)
        req.timeoutInterval = 30
        return req
    }

}

// MARK: - Connection test (explicit tap; one fixed word, never recording content)

enum ConnectionTest {
    enum Outcome: Equatable, Sendable {
        case connected(String)
        case wrongKey(String)
        case noCredit(String)
        case unreachable(String)
        case badURL(String)
        case modelMissing(String)
        case noKey(String)
        case other(String)

        var ok: Bool { if case .connected = self { return true } else { return false } }

        var message: String {
            switch self {
            case .connected(let m), .wrongKey(let m), .noCredit(let m), .unreachable(let m),
                 .badURL(let m), .modelMissing(let m), .noKey(let m), .other(let m): m
            }
        }
    }

    /// Plain words from the probe's status and body. Pure; unit tested.
    static func classify(host: String, model: String, status: Int, data: Data, secrets: [String]) -> Outcome {
        let msg = ResponseParser.errorMessage(data, secrets: secrets)
        let mentionsModel = msg.lowercased().contains("model")
        switch status {
        case 200..<300: return .connected("Connected to \(host) with model \(model).")
        case 401, 403: return .wrongKey("\(host) rejected the API key (\(status)). Check the key and save it again.")
        case 402, 429: return .noCredit("\(host) accepted the key but reported no credit or a rate limit (\(status)). \(msg)")
        case 300..<400: return .badURL("\(host) redirected the request (\(status)); the base URL is probably wrong.")
        case 404 where !mentionsModel: return .badURL("\(host) has no chat/completions at this base URL (404). It usually ends in /v1.")
        case 400, 404, 422: return mentionsModel
            ? .modelMissing("\(host) doesn't offer the model '\(model)'. \(msg)")
            : .other("\(host) answered \(status): \(msg)")
        default: return .other("\(host) answered \(status): \(msg)")
        }
    }

    /// Transport and configuration errors in the same plain words.
    static func outcome(for error: AIError, host: String) -> Outcome {
        switch error {
        case .notConfigured: return .badURL("Enter a base URL and model, then save the connection first.")
        case .insecureURL: return .badURL(error.localizedDescription)
        case .atsBlocked: return .unreachable(error.localizedDescription)
        case .unreachable(let m): return .unreachable("Couldn't reach \(host). \(m)")
        case .wrongKey(let m): return .wrongKey("\(host) rejected the API key. \(m)")
        case .noCredit(let m): return .noCredit("\(host) reported no credit or a rate limit. \(m)")
        case .tooLarge, .badResponse: return .other("\(host) sent a response this app couldn't read.")
        default: return .other(error.localizedDescription)
        }
    }
}

// MARK: - Response parsing (pure; unit-tested)

enum ResponseParser {
    /// Best-effort human message from an error body, redacted and short.
    static func errorMessage(_ data: Data, secrets: [String]) -> String {
        var msg = ""
        let obj = try? JSONSerialization.jsonObject(with: data)
        let root = (obj as? [String: Any]) ?? ((obj as? [Any])?.first as? [String: Any])
        if let err = root?["error"] as? [String: Any], let m = err["message"] as? String {
            msg = m
        } else if let m = root?["error"] as? String {
            msg = m
        } else if let m = root?["message"] as? String {
            msg = m
        } else if let m = root?["detail"] as? String {
            msg = m
        } else if obj == nil {
            msg = String(decoding: data.prefix(200), as: UTF8.self)
        }
        msg = msg.trimmingCharacters(in: .whitespacesAndNewlines)
        if msg.count > 200 { msg = String(msg.prefix(200)) + "…" }
        return Redactor.redact(msg, secrets: secrets)
    }

    static func statusError(_ status: Int, _ data: Data, secrets: [String]) -> AIError {
        let m = errorMessage(data, secrets: secrets)
        switch status {
        case 401, 403: return .wrongKey(m)
        case 402, 429: return .noCredit(m)
        case 400 where m.lowercased().contains("api key"): return .wrongKey(m)
        default: return .server(status, m)
        }
    }

    /// A 400 that says this model wants max_completion_tokens / default temperature.
    static func wantsCompletionTokens(_ message: String) -> Bool {
        let m = message.lowercased()
        return m.contains("max_completion_tokens") || m.contains("max_tokens")
            || (m.contains("temperature") && (m.contains("unsupported") || m.contains("not support") || m.contains("default")))
    }

    static func openAIText(_ data: Data) throws -> String {
        guard let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let choice = (obj["choices"] as? [[String: Any]])?.first else { throw AIError.badResponse }
        let message = choice["message"] as? [String: Any]
        var text = ""
        if let s = message?["content"] as? String {
            text = s
        } else if let parts = message?["content"] as? [[String: Any]] {
            text = parts.compactMap { $0["text"] as? String }.joined()
        }
        text = stripThinking(text).trimmingCharacters(in: .whitespacesAndNewlines)
        if text.isEmpty {
            if (choice["finish_reason"] as? String) == "length" { throw AIError.truncated }
            if message?["refusal"] is String { throw AIError.refused }
            throw AIError.emptyAnswer
        }
        return text
    }

    /// Some compatible models inline reasoning in <think>…</think>.
    static func stripThinking(_ s: String) -> String {
        guard s.contains("<think>") else { return s }
        var out = s
        while let open = out.range(of: "<think>") {
            if let close = out.range(of: "</think>", range: open.upperBound..<out.endIndex) {
                out.removeSubrange(open.lowerBound..<close.upperBound)
            } else {
                out.removeSubrange(open.lowerBound..<out.endIndex)
            }
        }
        return out
    }

}

// MARK: - Client

/// One place every AI request goes through, with the guardrails from the spec:
/// max_tokens always, fitted context, no response_format, timeouts, HTTPS for
/// cloud, redacted errors, a 4 MB response cap and no redirects.
actor AIClient {
    static let shared = AIClient()
    static let maxResponseBytes = 4 * 1024 * 1024
    private static let completionTokenModelsKey = "ai.completionTokenModels"

    /// Models known to want max_completion_tokens and no temperature ("endpoint|model").
    private var completionTokenModels: Set<String>
    /// URLProtocol classes for tests (a stub answers instead of the network). Empty in the app.
    private let protocolClasses: [AnyClass]

    init(protocolClasses: [AnyClass] = []) {
        completionTokenModels = Set(UserDefaults.standard.stringArray(forKey: Self.completionTokenModelsKey) ?? [])
        self.protocolClasses = protocolClasses
    }

    /// The connection test: one fixed word with a one-token answer to the saved
    /// endpoint with its saved key. It carries no recording content, so it is
    /// the only request allowed before consent, and it runs only on an
    /// explicit tap (never at launch or on save). Failures come back as plain
    /// words, never thrown.
    func testConnection(_ ep: AIEndpoint) async -> ConnectionTest.Outcome {
        if ep.provider.proto == .foundationModels {
            return AppleOnDevice.isAvailable ? .connected("Apple on-device model is available.") : .unreachable("Apple on-device model is not available on this device.")
        }
        guard ep.provider == .custom, let base = ep.baseURL,
              !ep.model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return ConnectionTest.outcome(for: .notConfigured, host: ep.provider.name)
        }
        let host = base.host() ?? base.absoluteString
        guard URLPolicy.isAllowed(base) else { return ConnectionTest.outcome(for: .insecureURL, host: host) }
        let secrets = [ep.apiKey].compactMap { $0 }
        var useCompletionTokens = completionTokenModels.contains("\(base.absoluteString)|\(ep.model)")
        for attempt in 0..<2 {
            do {
                let req = try RequestBuilder.probe(ep, completionTokens: useCompletionTokens)
                let (data, status) = try await send(req, secrets: secrets)
                if status == 400, attempt == 0, !useCompletionTokens,
                   ResponseParser.wantsCompletionTokens(ResponseParser.errorMessage(data, secrets: secrets)) {
                    useCompletionTokens = true
                    continue
                }
                return ConnectionTest.classify(host: host, model: ep.model, status: status, data: data, secrets: secrets)
            } catch let e as AIError {
                return ConnectionTest.outcome(for: e, host: host)
            } catch {
                return .unreachable("Couldn't reach \(host). \(Redactor.redact(error.localizedDescription, secrets: secrets))")
            }
        }
        return .other("\(host) rejected the request twice.")
    }

    func complete(_ messages: [ChatMessage], maxTokens: Int, temperature: Double = 0.3, endpoint ep: AIEndpoint) async throws -> String {
        if ep.needsConsent && !ep.consentGranted { throw AIError.consentRequired(ep.baseURL?.host() ?? ep.provider.name) }
        let requested = min(max(maxTokens, 16), 8192)

        if ep.provider.proto == .foundationModels {
            return try await AppleOnDevice.complete(messages, maxTokens: requested, temperature: temperature)
        }
        guard ep.provider == .custom, let base = ep.baseURL,
              !ep.model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { throw AIError.notConfigured }
        guard URLPolicy.isAllowed(base) else { throw AIError.insecureURL }

        let modelKey = "\(base.absoluteString)|\(ep.model)"
        var useCompletionTokens = completionTokenModels.contains(modelKey)
        for attempt in 0..<2 {
            let budget = RequestBuilder.outputBudget(proto: ep.provider.proto, requested: requested,
                                                     completionTokens: useCompletionTokens)
            let fitted = ContextFit.fit(messages, contextTokens: ep.contextTokens, maxTokens: budget)
            let req = try RequestBuilder.chat(ep, messages: fitted, maxTokens: budget, temperature: temperature,
                                              completionTokens: useCompletionTokens)
            let (data, status) = try await send(req, secrets: [ep.apiKey].compactMap { $0 })
            if (200..<300).contains(status) {
                return try ResponseParser.openAIText(data)
            }
            let message = ResponseParser.errorMessage(data, secrets: [ep.apiKey].compactMap { $0 })
            if status == 400, attempt == 0, ep.provider.proto == .openai, !useCompletionTokens,
               ResponseParser.wantsCompletionTokens(message) {
                useCompletionTokens = true
                completionTokenModels.insert(modelKey)
                UserDefaults.standard.set(Array(completionTokenModels), forKey: Self.completionTokenModelsKey)
                continue
            }
            throw ResponseParser.statusError(status, data, secrets: [ep.apiKey].compactMap { $0 })
        }
        throw AIError.badResponse
    }

    // MARK: Transport

    private func send(_ req: URLRequest, secrets: [String], total: TimeInterval? = nil) async throws -> (Data, Int) {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = req.timeoutInterval
        config.timeoutIntervalForResource = total ?? req.timeoutInterval
        config.waitsForConnectivity = false
        config.httpCookieStorage = nil
        config.urlCache = nil
        if !protocolClasses.isEmpty { config.protocolClasses = protocolClasses }
        let session = URLSession(configuration: config, delegate: NoRedirects(), delegateQueue: nil)
        defer { session.finishTasksAndInvalidate() }
        do {
            let (bytes, resp) = try await session.bytes(for: req)
            guard let http = resp as? HTTPURLResponse else { throw AIError.badResponse }
            if http.expectedContentLength > Self.maxResponseBytes { bytes.task.cancel(); throw AIError.tooLarge }
            var data = Data()
            data.reserveCapacity(Int(max(0, min(http.expectedContentLength, Int64(Self.maxResponseBytes)))))
            for try await byte in bytes {
                data.append(byte)
                if data.count > Self.maxResponseBytes { bytes.task.cancel(); throw AIError.tooLarge }
            }
            return (data, http.statusCode)
        } catch let e as AIError {
            throw e
        } catch let e as URLError {
            if e.code == .appTransportSecurityRequiresSecureConnection { throw AIError.atsBlocked }
            if e.code == .cancelled { throw CancellationError() }
            throw AIError.unreachable(Redactor.redact(e.localizedDescription, secrets: secrets))
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            throw AIError.unreachable(Redactor.redact(error.localizedDescription, secrets: secrets))
        }
    }
}

/// Keys must never follow a redirect to another host.
private final class NoRedirects: NSObject, URLSessionTaskDelegate, Sendable {
    func urlSession(_: URLSession, task _: URLSessionTask, willPerformHTTPRedirection _: HTTPURLResponse,
                    newRequest _: URLRequest) async -> URLRequest? { nil }
}
