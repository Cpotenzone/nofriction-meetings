import StoreKit
import XCTest
@testable import noFriction

final class KeyDetectionTests: XCTestCase {
    func testDetectsEachPrefix() {
        XCTAssertEqual(KeyDetector.detect("sk-proj-abc123")?.id, "openai")
        XCTAssertEqual(KeyDetector.detect("sk-svcacct-abc123")?.id, "openai")
        XCTAssertEqual(KeyDetector.detect("sk-abc123")?.id, "openai")
        XCTAssertEqual(KeyDetector.detect("sk-ant-api03-abc")?.id, "anthropic")
        XCTAssertEqual(KeyDetector.detect("sk-or-v1-abc")?.id, "openrouter")
        XCTAssertEqual(KeyDetector.detect("AIzaSyAbc123")?.id, "gemini")
        XCTAssertEqual(KeyDetector.detect("xai-abc123")?.id, "xai")
        XCTAssertEqual(KeyDetector.detect("gsk_abc123")?.id, "groq")
        XCTAssertEqual(KeyDetector.detect("pplx-abc123")?.id, "perplexity")
        XCTAssertNil(KeyDetector.detect("abcdef123456"))
    }

    func testBareSkTriesOpenAIThenDeepSeek() {
        XCTAssertEqual(KeyDetector.candidates(for: "sk-abc123").map(\.id), ["openai", "deepseek"])
        XCTAssertEqual(KeyDetector.candidates(for: "sk-proj-abc").map(\.id), ["openai"])
        XCTAssertEqual(KeyDetector.candidates(for: "sk-ant-abc").map(\.id), ["anthropic"])
        XCTAssertEqual(KeyDetector.candidates(for: "unknown"), [])
    }

    func testNormalizeStripsQuotesBearerAndWhitespace() {
        XCTAssertEqual(KeyDetector.normalize("  \"sk-proj-abc\"\n"), "sk-proj-abc")
        XCTAssertEqual(KeyDetector.normalize("Bearer sk-ant-xyz"), "sk-ant-xyz")
        XCTAssertEqual(KeyDetector.normalize("'gsk_1 2'"), "gsk_12")
        XCTAssertEqual(KeyDetector.last4("sk-proj-abcdWXYZ"), "WXYZ")
    }
}

final class URLPolicyTests: XCTestCase {
    private func allowed(_ s: String) -> Bool { URLPolicy.isAllowed(URL(string: s)!) }

    func testCloudNeedsHTTPS() {
        XCTAssertTrue(allowed("https://api.openai.com/v1"))
        XCTAssertFalse(allowed("http://api.openai.com/v1"))
        XCTAssertFalse(allowed("http://example.com:11434/v1"))
        XCTAssertFalse(allowed("http://8.8.8.8/v1"))
        XCTAssertFalse(allowed("ftp://localhost/v1"))
        XCTAssertFalse(allowed("https://user:pw@api.openai.com/v1"))
    }

    func testHTTPAllowedForLocalAndPrivateHosts() {
        XCTAssertTrue(allowed("http://localhost:11434/v1"))
        XCTAssertTrue(allowed("http://127.0.0.1:1234/v1"))
        XCTAssertTrue(allowed("http://192.168.1.20:11434/v1"))
        XCTAssertTrue(allowed("http://10.0.0.5/v1"))
        XCTAssertTrue(allowed("http://172.16.4.2/v1"))
        XCTAssertTrue(allowed("http://100.64.0.1/v1"))
        XCTAssertTrue(allowed("http://100.127.255.254/v1"))
        XCTAssertTrue(allowed("http://studio.local:1234/v1"))
        XCTAssertTrue(allowed("http://box.tail1234.ts.net:8080/v1"))
        XCTAssertFalse(allowed("http://100.128.0.1/v1"))   // outside 100.64/10
        XCTAssertFalse(allowed("http://172.32.0.1/v1"))    // outside 172.16/12
        XCTAssertFalse(allowed("http://192.169.0.1/v1"))
        XCTAssertFalse(allowed("http://evil-ts.net.example.com/v1"))
    }

    func testConsentOnlyForCloud() {
        XCTAssertTrue(URLPolicy.needsConsent(provider: .openai, baseURL: AIProvider.openai.defaultBaseURL))
        XCTAssertFalse(URLPolicy.needsConsent(provider: .ollama, baseURL: URL(string: "http://192.168.1.2:11434/v1")))
        XCTAssertTrue(URLPolicy.needsConsent(provider: .custom, baseURL: URL(string: "https://my-proxy.example.com/v1")))
        XCTAssertFalse(URLPolicy.needsConsent(provider: .apple, baseURL: nil))
    }

    func testParsesTypedServerAddresses() {
        XCTAssertEqual(URLPolicy.parseBaseURL("192.168.1.5:11434/v1/")?.absoluteString, "http://192.168.1.5:11434/v1")
        XCTAssertEqual(URLPolicy.parseBaseURL("api.example.com/v1")?.absoluteString, "https://api.example.com/v1")
        XCTAssertNil(URLPolicy.parseBaseURL("   "))
    }

    func testCloudPresetsAreHTTPS() {
        for p in AIProvider.cloud {
            XCTAssertEqual(p.defaultBaseURL?.scheme, "https", p.id)
            if let v = p.validationURL { XCTAssertEqual(v.scheme, "https", p.id) }
        }
    }

    func testNoPrivateHostsShip() {
        for p in AIProvider.all {
            let hosts = [p.defaultBaseURL, p.validationURL, p.getKeyURL].compactMap { $0?.host() }
            XCTAssertFalse(hosts.contains { $0.hasSuffix(".ts.net") }, p.id)
        }
    }
}

final class RedactionTests: XCTestCase {
    func testRedactsKnownKeyShapes() {
        let text = "bad key sk-proj-ABCDEFGH12345 and sk-ant-api03-XYZXYZXYZ, AIzaSyA1234567890abc, xai-1234567890, gsk_abcdef123456, pplx-abcdef1234"
        let out = Redactor.redact(text)
        for secret in ["sk-proj-ABCDEFGH12345", "sk-ant-api03-XYZXYZXYZ", "AIzaSyA1234567890abc", "xai-1234567890", "gsk_abcdef123456", "pplx-abcdef1234"] {
            XCTAssertFalse(out.contains(secret), secret)
        }
        XCTAssertTrue(out.contains("bad key"))
    }

    func testRedactsBearerHeadersAndExplicitSecrets() {
        XCTAssertFalse(Redactor.redact("Authorization: Bearer abc.def.ghi").contains("abc.def.ghi"))
        XCTAssertFalse(Redactor.redact("\"x-api-key\": \"weirdkey999\"").contains("weirdkey999"))
        XCTAssertEqual(Redactor.redact("key=mistralKEY123 failed", secrets: ["mistralKEY123"]).contains("mistralKEY123"), false)
    }

    func testErrorBodyIsRedacted() {
        let body = Data(#"{"error":{"message":"Incorrect API key provided: sk-proj-SECRET123456."}}"#.utf8)
        let err = ResponseParser.statusError(401, body, secrets: ["sk-proj-SECRET123456"])
        guard case .wrongKey(let m) = err else { return XCTFail("\(err)") }
        XCTAssertFalse(m.contains("SECRET123456"))
        XCTAssertFalse(err.localizedDescription.contains("SECRET"))
    }

    func testStatusMapping() {
        XCTAssertEqual(ResponseParser.statusError(429, Data(), secrets: []), .noCredit(""))
        XCTAssertEqual(ResponseParser.statusError(402, Data(), secrets: []), .noCredit(""))
        let gemini = Data(#"[{"error":{"code":400,"message":"Please pass a valid API key"}}]"#.utf8)
        if case .wrongKey = ResponseParser.statusError(400, gemini, secrets: []) {} else { XCTFail("gemini bad key") }
        if case .server(500, _) = ResponseParser.statusError(500, Data("oops".utf8), secrets: []) {} else { XCTFail() }
    }
}

final class ContextFitTests: XCTestCase {
    func testShortPromptsAreUntouched() {
        let m = [ChatMessage(role: "user", content: "hello")]
        XCTAssertEqual(ContextFit.fit(m, maxTokens: 1500).first?.content, "hello")
    }

    func testLongTranscriptIsTrimmedInTheMiddle() {
        let transcript = "START " + String(repeating: "word ", count: 60_000) + " END"
        let m = [ChatMessage(role: "system", content: "Summarize."), ChatMessage(role: "user", content: transcript)]
        let out = ContextFit.fit(m, maxTokens: 1500)
        let budget = ContextFit.budgetChars(contextTokens: 32_768, maxTokens: 1500)
        XCTAssertLessThanOrEqual(out.reduce(0) { $0 + $1.content.count }, budget)
        XCTAssertTrue(out[1].content.hasPrefix("START"))
        XCTAssertTrue(out[1].content.hasSuffix("END"))
        XCTAssertTrue(out[1].content.contains("trimmed to fit"))
        XCTAssertEqual(out[0].content, "Summarize.")
    }

    func testRespectsSmallOnDeviceWindow() {
        let m = [ChatMessage(role: "user", content: String(repeating: "x", count: 50_000))]
        let out = ContextFit.fit(m, contextTokens: 4096, maxTokens: 1200)
        XCTAssertLessThanOrEqual(out[0].content.count, ContextFit.budgetChars(contextTokens: 4096, maxTokens: 1200))
    }
}

final class RequestBuildingTests: XCTestCase {
    private let msgs = [ChatMessage(role: "system", content: "Be brief."), ChatMessage(role: "user", content: "Hi")]

    private func ep(_ p: AIProvider, key: String? = "k-123456", base: URL? = nil) -> AIEndpoint {
        AIEndpoint(provider: p, baseURL: base ?? p.defaultBaseURL, apiKey: key, model: "m1",
                   contextTokens: 32_768, consentGranted: true)
    }

    private func body(_ r: URLRequest) throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(r.httpBody)) as? [String: Any])
    }

    func testOpenAIRequest() throws {
        let r = try RequestBuilder.chat(ep(.openai), messages: msgs, maxTokens: 700, temperature: 0.5)
        XCTAssertEqual(r.url?.absoluteString, "https://api.openai.com/v1/chat/completions")
        XCTAssertEqual(r.httpMethod, "POST")
        XCTAssertEqual(r.value(forHTTPHeaderField: "Authorization"), "Bearer k-123456")
        XCTAssertNil(r.value(forHTTPHeaderField: "x-api-key"))
        let b = try body(r)
        XCTAssertEqual(b["max_tokens"] as? Int, 700)
        XCTAssertEqual(b["temperature"] as? Double, 0.5)
        XCTAssertEqual(b["stream"] as? Bool, false)
        XCTAssertNil(b["response_format"])
        let sent = try XCTUnwrap(b["messages"] as? [[String: String]])
        XCTAssertEqual(sent.first?["role"], "system")   // system stays in messages
        XCTAssertEqual(r.timeoutInterval, 60 + 700.0 / 8, accuracy: 0.01)
    }

    func testOpenAICompletionTokensVariant() throws {
        let r = try RequestBuilder.chat(ep(.openai), messages: msgs, maxTokens: 900, temperature: 0.5, completionTokens: true)
        let b = try body(r)
        XCTAssertEqual(b["max_completion_tokens"] as? Int, 900)
        XCTAssertNil(b["max_tokens"])
        XCTAssertNil(b["temperature"])
    }

    func testAnthropicRequest() throws {
        let r = try RequestBuilder.chat(ep(.anthropic, key: "sk-ant-abc"), messages: msgs, maxTokens: 1500, temperature: 0.2)
        XCTAssertEqual(r.url?.absoluteString, "https://api.anthropic.com/v1/messages")
        XCTAssertEqual(r.value(forHTTPHeaderField: "x-api-key"), "sk-ant-abc")
        XCTAssertEqual(r.value(forHTTPHeaderField: "anthropic-version"), "2023-06-01")
        XCTAssertNil(r.value(forHTTPHeaderField: "Authorization"))
        let b = try body(r)
        XCTAssertEqual(b["system"] as? String, "Be brief.")
        XCTAssertEqual(b["max_tokens"] as? Int, 1500)
        XCTAssertNil(b["temperature"])
        XCTAssertNil(b["response_format"])
        let sent = try XCTUnwrap(b["messages"] as? [[String: String]])
        XCTAssertEqual(sent.map { $0["role"] }, ["user"])
    }

    func testModelsRequests() throws {
        let a = try RequestBuilder.models(ep(.anthropic, key: "sk-ant-abc"))
        XCTAssertEqual(a.url?.path(), "/v1/models")
        XCTAssertEqual(a.value(forHTTPHeaderField: "x-api-key"), "sk-ant-abc")
        XCTAssertEqual(a.value(forHTTPHeaderField: "anthropic-version"), "2023-06-01")
        let o = try RequestBuilder.models(ep(.groq, key: "gsk_x"))
        XCTAssertEqual(o.url?.absoluteString, "https://api.groq.com/openai/v1/models")
        XCTAssertEqual(o.value(forHTTPHeaderField: "Authorization"), "Bearer gsk_x")
        let local = try RequestBuilder.models(ep(.ollama, key: nil))
        XCTAssertNil(local.value(forHTTPHeaderField: "Authorization"))
    }

    func testRefusesInsecureCloudURL() {
        XCTAssertThrowsError(try RequestBuilder.chat(ep(.custom, base: URL(string: "http://api.example.com/v1")),
                                                     messages: msgs, maxTokens: 100, temperature: nil)) { e in
            XCTAssertEqual(e as? AIError, .insecureURL)
        }
    }

    func testOutputBudgetLeavesRoomForThinking() {
        XCTAssertEqual(RequestBuilder.outputBudget(proto: .openai, requested: 700, completionTokens: false), 700)
        XCTAssertGreaterThan(RequestBuilder.outputBudget(proto: .anthropic, requested: 700, completionTokens: false), 700)
        XCTAssertLessThanOrEqual(RequestBuilder.outputBudget(proto: .openai, requested: 8000, completionTokens: true), 16_000)
    }
}

final class ResponseParsingTests: XCTestCase {
    func testOpenAIAnswer() throws {
        let d = Data(#"{"choices":[{"message":{"content":"<think>hmm</think> Hello "},"finish_reason":"stop"}]}"#.utf8)
        XCTAssertEqual(try ResponseParser.openAIText(d), "Hello")
        let cut = Data(#"{"choices":[{"message":{"content":""},"finish_reason":"length"}]}"#.utf8)
        XCTAssertThrowsError(try ResponseParser.openAIText(cut)) { XCTAssertEqual($0 as? AIError, .truncated) }
    }

    func testAnthropicAnswerSkipsThinkingBlocks() throws {
        let d = Data(#"{"content":[{"type":"thinking","thinking":""},{"type":"text","text":"Notes"}],"stop_reason":"end_turn"}"#.utf8)
        XCTAssertEqual(try ResponseParser.anthropicText(d), "Notes")
        let refused = Data(#"{"content":[],"stop_reason":"refusal"}"#.utf8)
        XCTAssertThrowsError(try ResponseParser.anthropicText(refused)) { XCTAssertEqual($0 as? AIError, .refused) }
    }

    func testModelListShapes() throws {
        let openai = Data(#"{"object":"list","data":[{"id":"gpt-5-mini"},{"id":"text-embedding-3-small"}]}"#.utf8)
        XCTAssertEqual(try ResponseParser.models(openai).map(\.id), ["gpt-5-mini", "text-embedding-3-small"])
        let anthropic = Data(#"{"data":[{"id":"claude-opus-5","max_input_tokens":1000000}],"has_more":false}"#.utf8)
        XCTAssertEqual(try ResponseParser.models(anthropic).first?.contextTokens, 1_000_000)
        let together = Data(#"[{"id":"meta-llama/Llama-3.3-70B-Instruct-Turbo","context_length":131072}]"#.utf8)
        XCTAssertEqual(try ResponseParser.models(together).first?.contextTokens, 131_072)
        let gemini = Data(#"{"data":[{"id":"models/gemini-2.5-flash"}]}"#.utf8)
        XCTAssertEqual(try ResponseParser.models(gemini).first?.id, "gemini-2.5-flash")
        let ollama = Data(#"{"models":[{"name":"llama3.2:latest"}]}"#.utf8)
        XCTAssertEqual(try ResponseParser.models(ollama).first?.id, "llama3.2:latest")
    }

    func testDefaultModelPicksPreferenceThenChatModel() {
        XCTAssertEqual(ModelPicker.defaultModel(for: .openai, from: ["whisper-1", "gpt-4o-mini", "gpt-5-mini"]), "gpt-5-mini")
        XCTAssertEqual(ModelPicker.defaultModel(for: .gemini, from: ["embedding-001", "gemini-2.5-flash-preview-09"]), "gemini-2.5-flash-preview-09")
        XCTAssertEqual(ModelPicker.defaultModel(for: .ollama, from: ["nomic-embed-text", "llama3.2"]), "llama3.2")
        // Sonnet by default: users pay their own bill, and notes don't need Opus
        XCTAssertEqual(ModelPicker.defaultModel(for: .anthropic,
                                                from: ["claude-opus-5-5", "claude-haiku-4-5-20251001", "claude-sonnet-5"]), "claude-sonnet-5")
    }

    func testMaxCompletionTokensHint() {
        XCTAssertTrue(ResponseParser.wantsCompletionTokens("Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead."))
        XCTAssertTrue(ResponseParser.wantsCompletionTokens("Unsupported value: 'temperature' does not support 0.5 with this model. Only the default (1) value is supported."))
        XCTAssertFalse(ResponseParser.wantsCompletionTokens("model not found"))
    }
}

final class StoreEntitlementTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    func testActiveSubscriptionGrantsPro() {
        XCTAssertTrue(Store.grantsPro(productID: Store.monthlyID, revocationDate: nil,
                                      expirationDate: now.addingTimeInterval(3600), isUpgraded: false, now: now))
        XCTAssertTrue(Store.grantsPro(productID: Store.yearlyID, revocationDate: nil,
                                      expirationDate: nil, isUpgraded: false, now: now))
    }

    func testRevokedExpiredUpgradedOrUnknownDoNot() {
        XCTAssertFalse(Store.grantsPro(productID: Store.monthlyID, revocationDate: now,
                                       expirationDate: now.addingTimeInterval(3600), isUpgraded: false, now: now))
        XCTAssertFalse(Store.grantsPro(productID: Store.monthlyID, revocationDate: nil,
                                       expirationDate: now.addingTimeInterval(-1), isUpgraded: false, now: now))
        XCTAssertFalse(Store.grantsPro(productID: Store.monthlyID, revocationDate: nil,
                                       expirationDate: now.addingTimeInterval(3600), isUpgraded: true, now: now))
        XCTAssertFalse(Store.grantsPro(productID: "com.other.app.pro", revocationDate: nil,
                                       expirationDate: nil, isUpgraded: false, now: now))
    }

    func testPeriodText() {
        XCTAssertEqual(SubscriptionText.period(.month, value: 1), "month")
        XCTAssertEqual(SubscriptionText.period(.year, value: 1), "year")
        XCTAssertEqual(SubscriptionText.duration(.day, value: 7), "1 week")
        XCTAssertEqual(SubscriptionText.duration(.week, value: 1), "1 week")
        XCTAssertEqual(SubscriptionText.duration(.day, value: 3), "3 days")
    }
}

@MainActor
final class AISettingsTests: XCTestCase {
    func testConsentAndActiveProvider() throws {
        let defaults = try XCTUnwrap(UserDefaults(suiteName: "nf-tests-\(UUID().uuidString)"))
        let s = AISettings(defaults: defaults)
        let url = URL(string: "http://192.168.1.9:11434/v1")!
        try s.save(.ollama, key: nil, baseURL: url, models: [ModelInfo(id: "llama3.2", contextTokens: nil)])
        XCTAssertEqual(s.effectiveProvider, .ollama)
        XCTAssertEqual(s.model(for: "ollama"), "llama3.2")
        XCTAssertTrue(s.hasConsent(.ollama))                 // LAN server: no consent needed
        let ep = try XCTUnwrap(s.endpoint())
        XCTAssertEqual(ep.baseURL, url)
        XCTAssertFalse(ep.needsConsent)
        s.remove(.ollama)
        XCTAssertNil(s.saved["ollama"])
    }
}
