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
        case .notConfigured: "Set up an AI provider in Settings first."
        case .consentRequired(let p): "Allow sending meeting content to \(p) first."
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
    static let anthropicVersion = "2023-06-01"
    static let maxOutputTokens = 16_000

    /// Total time allowed for one completion.
    static func timeout(maxTokens: Int) -> TimeInterval { 60 + Double(maxTokens) / 8 }

    /// Tokens actually requested. Models that think before answering (Claude,
    /// OpenAI reasoning models) spend part of max_tokens on that, so give them room.
    static func outputBudget(proto: AIProtocol, requested: Int, completionTokens: Bool) -> Int {
        switch proto {
        case .anthropic: min(requested + 4096, maxOutputTokens)
        case .openai: completionTokens ? min(requested + 4096, maxOutputTokens) : requested
        case .foundationModels: requested
        }
    }

    static func chat(_ ep: AIEndpoint, messages: [ChatMessage], maxTokens: Int, temperature: Double?,
                     completionTokens: Bool = false) throws -> URLRequest {
        guard let base = ep.baseURL else { throw AIError.notConfigured }
        guard URLPolicy.isAllowed(base) else { throw AIError.insecureURL }
        switch ep.provider.proto {
        case .anthropic:
            var req = URLRequest(url: base.appending(path: "messages"))
            req.httpMethod = "POST"
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
            req.setValue(anthropicVersion, forHTTPHeaderField: "anthropic-version")
            if let key = ep.apiKey, !key.isEmpty { req.setValue(key, forHTTPHeaderField: "x-api-key") }
            let system = messages.filter { $0.role == "system" }.map(\.content).joined(separator: "\n\n")
            var body: [String: Any] = [
                "model": ep.model,
                "max_tokens": maxTokens,
                "messages": messages.filter { $0.role != "system" }.map { ["role": $0.role, "content": $0.content] },
            ]
            // No temperature: current Claude models reject sampling parameters.
            if !system.isEmpty { body["system"] = system }
            req.httpBody = try JSONSerialization.data(withJSONObject: body, options: [.sortedKeys])
            req.timeoutInterval = timeout(maxTokens: maxTokens)
            return req
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

    static func get(_ url: URL, _ ep: AIEndpoint) throws -> URLRequest {
        guard URLPolicy.isAllowed(url) else { throw AIError.insecureURL }
        var req = URLRequest(url: url)
        req.httpMethod = "GET"
        req.timeoutInterval = 15
        if let key = ep.apiKey, !key.isEmpty {
            if ep.provider.proto == .anthropic {
                req.setValue(key, forHTTPHeaderField: "x-api-key")
                req.setValue(anthropicVersion, forHTTPHeaderField: "anthropic-version")
            } else {
                req.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization")
            }
        }
        return req
    }

    static func models(_ ep: AIEndpoint) throws -> URLRequest {
        guard let base = ep.baseURL else { throw AIError.notConfigured }
        var url = base.appending(path: "models")
        if ep.provider.proto == .anthropic { url.append(queryItems: [URLQueryItem(name: "limit", value: "1000")]) }
        return try get(url, ep)
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
        case 400 where m.lowercased().contains("api key"): return .wrongKey(m)  // Gemini answers 400 for a bad key
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

    static func anthropicText(_ data: Data) throws -> String {
        guard let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let content = obj["content"] as? [[String: Any]] else { throw AIError.badResponse }
        let stop = obj["stop_reason"] as? String
        if stop == "refusal" { throw AIError.refused }
        let text = content.filter { ($0["type"] as? String) == "text" }
            .compactMap { $0["text"] as? String }.joined()
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if text.isEmpty { throw stop == "max_tokens" ? AIError.truncated : AIError.emptyAnswer }
        return text
    }

    /// Local reasoning models (Qwen, DeepSeek-R1 via Ollama) inline <think>…</think>.
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

    /// OpenAI-style `{data:[{id}]}`, a bare array (Together), Anthropic `{data:[{id,max_input_tokens}]}`,
    /// or Ollama `/api/tags` `{models:[{name}]}`.
    static func models(_ data: Data) throws -> [ModelInfo] {
        let obj = try JSONSerialization.jsonObject(with: data)
        let list: [[String: Any]]
        if let d = obj as? [String: Any] {
            list = (d["data"] as? [[String: Any]]) ?? (d["models"] as? [[String: Any]]) ?? []
        } else {
            list = (obj as? [[String: Any]]) ?? []
        }
        var seen = Set<String>()
        return list.compactMap { m in
            guard var id = (m["id"] as? String) ?? (m["name"] as? String) ?? (m["model"] as? String) else { return nil }
            if id.hasPrefix("models/") { id = String(id.dropFirst(7)) }  // Gemini
            guard !id.isEmpty, seen.insert(id).inserted else { return nil }
            let ctx = ["max_input_tokens", "context_length", "context_window", "max_context_length"]
                .lazy.compactMap { m[$0] as? Int }.first
            return ModelInfo(id: id, contextTokens: ctx)
        }
    }
}

enum ModelPicker {
    private static let nonChat = ["embed", "whisper", "tts", "dall-e", "moderation", "image", "audio",
                                  "realtime", "transcribe", "rerank", "guard", "davinci", "babbage", "sora", "omni-moderation"]

    static func isChatCapable(_ id: String) -> Bool {
        let l = id.lowercased()
        return !nonChat.contains { l.contains($0) }
    }

    /// First match from the preset's preference list (exact, then prefix), else the first chat model.
    static func defaultModel(for provider: AIProvider, from ids: [String]) -> String? {
        let chat = ids.filter(isChatCapable)
        for pref in provider.preferredModels {
            if let m = chat.first(where: { $0 == pref }) { return m }
        }
        for pref in provider.preferredModels {
            if let m = chat.sorted().first(where: { $0.hasPrefix(pref) }) { return m }
        }
        return chat.first ?? ids.first
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

    /// Models known to want max_completion_tokens and no temperature ("provider|model").
    private var completionTokenModels: Set<String>

    init() {
        completionTokenModels = Set(UserDefaults.standard.stringArray(forKey: Self.completionTokenModelsKey) ?? [])
    }

    func complete(_ messages: [ChatMessage], maxTokens: Int, temperature: Double = 0.3, endpoint ep: AIEndpoint) async throws -> String {
        if ep.needsConsent && !ep.consentGranted { throw AIError.consentRequired(ep.provider.name) }
        let requested = min(max(maxTokens, 16), 8192)

        if ep.provider.proto == .foundationModels {
            return try await AppleOnDevice.complete(messages, maxTokens: requested, temperature: temperature)
        }
        guard let base = ep.baseURL else { throw AIError.notConfigured }
        guard URLPolicy.isAllowed(base) else { throw AIError.insecureURL }

        let modelKey = "\(ep.provider.id)|\(ep.model)"
        var useCompletionTokens = completionTokenModels.contains(modelKey)
        for attempt in 0..<2 {
            let budget = RequestBuilder.outputBudget(proto: ep.provider.proto, requested: requested,
                                                     completionTokens: useCompletionTokens)
            let fitted = ContextFit.fit(messages, contextTokens: ep.contextTokens, maxTokens: budget)
            let req = try RequestBuilder.chat(ep, messages: fitted, maxTokens: budget, temperature: temperature,
                                              completionTokens: useCompletionTokens)
            let (data, status) = try await send(req, secrets: [ep.apiKey].compactMap { $0 })
            if (200..<300).contains(status) {
                return ep.provider.proto == .anthropic
                    ? try ResponseParser.anthropicText(data)
                    : try ResponseParser.openAIText(data)
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

    /// Check the key/URL by listing models. Returns the models on success.
    func validate(_ ep: AIEndpoint) async throws -> [ModelInfo] {
        let secrets = [ep.apiKey].compactMap { $0 }
        if ep.provider.proto == .foundationModels {
            if let reason = AppleOnDevice.unavailableReason { throw AIError.onDevice(reason) }
            return [ModelInfo(id: "apple-on-device", contextTokens: AppleOnDevice.contextTokens)]
        }
        if let check = ep.provider.validationURL {
            let (data, status) = try await send(try RequestBuilder.get(check, ep), secrets: secrets, total: 15)
            guard (200..<300).contains(status) else { throw ResponseParser.statusError(status, data, secrets: secrets) }
            if !ep.provider.staticModels.isEmpty {
                return ep.provider.staticModels.map { ModelInfo(id: $0, contextTokens: nil) }
            }
        }
        let (data, status) = try await send(try RequestBuilder.models(ep), secrets: secrets, total: 15)
        if (200..<300).contains(status) {
            let models = (try? ResponseParser.models(data)) ?? []
            if models.isEmpty && ep.provider.staticModels.isEmpty && !ep.provider.editableBaseURL {
                throw AIError.badResponse
            }
            return models.isEmpty ? ep.provider.staticModels.map { ModelInfo(id: $0, contextTokens: nil) } : models
        }
        // Older Ollama builds have no /v1/models; /api/tags lists the same models
        if status == 404, ep.provider == .ollama, let base = ep.baseURL {
            let root = base.lastPathComponent == "v1" ? base.deletingLastPathComponent() : base
            let (d2, s2) = try await send(try RequestBuilder.get(root.appending(path: "api/tags"), ep), secrets: secrets, total: 15)
            if (200..<300).contains(s2) { return (try? ResponseParser.models(d2)) ?? [] }
        }
        throw ResponseParser.statusError(status, data, secrets: secrets)
    }

    // MARK: Transport

    private func send(_ req: URLRequest, secrets: [String], total: TimeInterval? = nil) async throws -> (Data, Int) {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = req.timeoutInterval
        config.timeoutIntervalForResource = total ?? req.timeoutInterval
        config.waitsForConnectivity = false
        config.httpCookieStorage = nil
        config.urlCache = nil
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
