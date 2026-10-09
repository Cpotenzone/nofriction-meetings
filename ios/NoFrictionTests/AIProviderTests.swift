import StoreKit
import XCTest
@testable import noFriction

final class AIProviderCatalogTests: XCTestCase {
    func testOnlyExplicitEndpointAndAppleAreAvailable() {
        XCTAssertEqual(AIProvider.all.map(\.id), ["custom", "apple"])
        XCTAssertEqual(AIProvider.custom.proto, .openai)
        XCTAssertFalse(AIProvider.custom.isOnDevice)
        XCTAssertTrue(AIProvider.apple.isOnDevice)
    }

    func testLegacyNamedProvidersAreUnavailable() {
        for id in ["openai", "anthropic", "gemini", "xai", "groq", "openrouter", "mistral",
                   "deepseek", "perplexity", "together", "ollama", "lmstudio"] {
            XCTAssertNil(AIProvider.byID(id), id)
        }
    }

    func testNormalizeStripsQuotesBearerAndWhitespaceWithoutChoosingAHost() {
        XCTAssertEqual(KeyDetector.normalize("  \"fixture-key\"\n"), "fixture-key")
        XCTAssertEqual(KeyDetector.normalize("Bearer fixture-key"), "fixture-key")
        XCTAssertEqual(KeyDetector.normalize("'fixture 1 2'"), "fixture12")
        XCTAssertEqual(KeyDetector.last4("fixture-abcdWXYZ"), "WXYZ")
    }
}

final class AIPresetTests: XCTestCase {
    func testPresetTableIsStaticHTTPSAndNeverAProviderOrDefault() {
        XCTAssertEqual(AIPreset.all.map(\.id), ["openai", "anthropic", "xai", "mistral"])
        for p in AIPreset.all {
            let url = try! XCTUnwrap(URL(string: p.baseURL), p.id)
            XCTAssertTrue(p.baseURL.hasPrefix("https://"), p.id)
            XCTAssertTrue(URLPolicy.isAllowed(url), p.id)
            XCTAssertTrue(URLPolicy.needsConsent(provider: .custom, baseURL: url), "\(p.id): a preset is public and needs consent")
            XCTAssertTrue(p.keyURL.hasPrefix("https://"), p.id)
            XCTAssertFalse(p.defaultModel.isEmpty, p.id)
            XCTAssertNil(AIProvider.byID(p.id), "\(p.id) must not be a provider id; the saved connection stays custom")
            XCTAssertEqual(AIPreset.matching(url)?.id, p.id)
            XCTAssertEqual(AIPreset.matching(p.baseURL + "/")?.id, p.id)
        }
        XCTAssertNil(AIPreset.matching("https://proxy.example/v1"))
        XCTAssertNil(AIPreset.matching("http://127.0.0.1:11434/v1"))
        XCTAssertNil(AIPreset.matching(nil as URL?))
    }

    @MainActor
    func testNoCardIsSelectedOrSavedByDefault() throws {
        let form = AIConnectModel()
        XCTAssertNil(form.selectedCard)
        XCTAssertNil(form.chosenPreset)
        XCTAssertTrue(form.serverURL.isEmpty && form.serverModel.isEmpty && form.serverKey.isEmpty)
        let s = AISettings(defaults: try XCTUnwrap(UserDefaults(suiteName: "nf-tests-\(UUID().uuidString)")), keyStore: .init(get: { _ in nil }, set: { _, _ in }, delete: { _ in }))
        form.loadSaved(s)
        XCTAssertNil(form.selectedCard, "a fresh install highlights nothing")
        XCTAssertTrue(s.saved.isEmpty)
        XCTAssertNil(s.activeProviderID)
    }

    @MainActor
    func testChoosingAPresetFillsURLAndModelAndClearsTypedKey() {
        let form = AIConnectModel()
        let openai = AIPreset.byID("openai")!
        form.serverKey = "typed-key-fixture"
        form.choose(openai)
        XCTAssertEqual(form.selectedCard, "openai")
        XCTAssertEqual(form.serverURL, openai.baseURL)
        XCTAssertEqual(form.serverModel, openai.defaultModel)
        XCTAssertEqual(form.serverKey, "", "a key typed for one host is never carried to another")
        XCTAssertEqual(form.formPreset?.id, "openai")
        XCTAssertTrue(form.whatWillBeSent.contains("ChatGPT (OpenAI) at api.openai.com"))
        XCTAssertTrue(form.whatWillBeSent.contains("\"Hi\""))
        form.serverKey = "another-typed-key"
        form.choose(AIPreset.byID("mistral")!)
        XCTAssertEqual(form.serverURL, "https://api.mistral.ai/v1")
        XCTAssertEqual(form.serverModel, "mistral-large-latest")
        XCTAssertEqual(form.serverKey, "")
        // Editing the URL away from the preset drops the preset match
        form.serverURL = "https://proxy.example/v1"
        XCTAssertNil(form.formPreset)
    }

    @MainActor
    func testSwitchingPresetsOnSaveDeletesOldKeyAndConsent() throws {
        var vault: String?
        var deletions = 0
        let store = AISettings.KeyStore(get: { _ in vault }, set: { key, _ in vault = key }, delete: { _ in deletions += 1; vault = nil })
        let s = AISettings(defaults: try XCTUnwrap(UserDefaults(suiteName: "nf-tests-\(UUID().uuidString)")), keyStore: store)
        let form = AIConnectModel()
        form.choose(AIPreset.byID("openai")!)
        form.serverKey = "synthetic-openai-key-fixture"
        form.saveEndpoint(s)
        s.grantConsent(.custom)
        XCTAssertEqual(s.endpoint()?.baseURL?.host(), "api.openai.com")
        XCTAssertEqual(s.endpoint()?.apiKey, "synthetic-openai-key-fixture")
        XCTAssertEqual(s.displayName(for: .custom), "ChatGPT (OpenAI)")
        XCTAssertTrue(s.hasConsent(.custom))
        XCTAssertEqual(form.consentPrompt, .custom, "a preset is a public endpoint: consent is asked")
        XCTAssertTrue(form.canTest(s))

        form.choose(AIPreset.byID("anthropic")!)
        XCTAssertFalse(form.canTest(s), "the form no longer shows the saved endpoint")
        form.saveEndpoint(s)
        XCTAssertEqual(s.endpoint()?.baseURL?.host(), "api.anthropic.com")
        XCTAssertNil(s.endpoint()?.apiKey, "the OpenAI key must not travel to Anthropic")
        XCTAssertNil(vault)
        XCTAssertGreaterThan(deletions, 0)
        XCTAssertFalse(s.hasConsent(.custom), "consent is for a destination, not a brand")
        XCTAssertEqual(s.displayName(for: .custom), "Anthropic (Claude)")

        // Reopening the screen highlights the saved preset; a custom URL highlights Custom
        let again = AIConnectModel()
        again.loadSaved(s)
        XCTAssertEqual(again.selectedCard, "anthropic")
        again.chooseCustom(s)
        XCTAssertEqual(again.selectedCard, AIConnectModel.customCard)
        XCTAssertTrue(again.serverURL.isEmpty, "custom starts blank: no default remote URL")
        again.serverURL = "http://192.168.1.9:8080/v1"
        again.serverModel = "local-model"
        again.saveEndpoint(s)
        XCTAssertEqual(s.displayName(for: .custom), AIProvider.custom.name)
        let third = AIConnectModel()
        third.loadSaved(s)
        XCTAssertEqual(third.selectedCard, AIConnectModel.customCard)
    }
}

/// Answers every request from a canned handler, so no test touches the network.
final class StubURLProtocol: URLProtocol {
    nonisolated(unsafe) static var handler: ((URLRequest) throws -> (Int, Data))?
    nonisolated(unsafe) static var lastBody: Data?

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        var body = request.httpBody
        if body == nil, let stream = request.httpBodyStream {
            stream.open()
            var data = Data()
            let buf = UnsafeMutablePointer<UInt8>.allocate(capacity: 4096)
            defer { buf.deallocate() }
            while stream.hasBytesAvailable {
                let n = stream.read(buf, maxLength: 4096)
                if n <= 0 { break }
                data.append(buf, count: n)
            }
            body = data
        }
        Self.lastBody = body
        do {
            let (status, data) = try Self.handler!(request)
            let resp = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: "HTTP/1.1", headerFields: ["Content-Type": "application/json"])!
            client?.urlProtocol(self, didReceive: resp, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch {
            client?.urlProtocol(self, didFailWithError: error)
        }
    }
    override func stopLoading() {}
}

final class ConnectionTestTests: XCTestCase {
    private func ep(key: String? = "stub-key-value-1234", base: String = "https://endpoint.example/v1", model: String = "stub-model") -> AIEndpoint {
        AIEndpoint(provider: .custom, baseURL: URL(string: base), apiKey: key, model: model,
                   contextTokens: 32_768, consentGranted: false)
    }

    func testProbeIsOneFixedWordWithOneToken() throws {
        let r = try RequestBuilder.probe(ep())
        XCTAssertEqual(r.url?.absoluteString, "https://endpoint.example/v1/chat/completions")
        let b = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(r.httpBody)) as? [String: Any])
        XCTAssertEqual(b["max_tokens"] as? Int, 1)
        XCTAssertNil(b["temperature"])
        let msgs = try XCTUnwrap(b["messages"] as? [[String: String]])
        XCTAssertEqual(msgs.count, 1)
        XCTAssertEqual(msgs.first?["content"], "Hi")
        XCTAssertEqual(r.value(forHTTPHeaderField: "Authorization"), "Bearer stub-key-value-1234")
        let c = try RequestBuilder.probe(ep(), completionTokens: true)
        let cb = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(c.httpBody)) as? [String: Any])
        XCTAssertEqual(cb["max_completion_tokens"] as? Int, 1)
        XCTAssertNil(cb["max_tokens"])
    }

    func testClassifiesStatusesInPlainWordsAndRedacts() {
        let ok = ConnectionTest.classify(host: "h.example", model: "m1", status: 200, data: Data("{}".utf8), secrets: [])
        XCTAssertTrue(ok.ok)
        XCTAssertEqual(ok, .connected("Connected to h.example with model m1."))
        let bad = ConnectionTest.classify(host: "h.example", model: "m1", status: 401,
                                          data: Data(#"{"error":{"message":"Incorrect API key provided: sk-proj-SECRET123456"}}"#.utf8), secrets: ["sk-proj-SECRET123456"])
        guard case .wrongKey(let m) = bad else { return XCTFail("\(bad)") }
        XCTAssertFalse(m.contains("SECRET123456"))
        XCTAssertTrue(m.contains("h.example"))
        if case .wrongKey = ConnectionTest.classify(host: "h", model: "m", status: 403, data: Data(), secrets: []) {} else { XCTFail() }
        if case .noCredit = ConnectionTest.classify(host: "h", model: "m", status: 429, data: Data(), secrets: []) {} else { XCTFail() }
        if case .noCredit = ConnectionTest.classify(host: "h", model: "m", status: 402, data: Data(), secrets: []) {} else { XCTFail() }
        if case .badURL = ConnectionTest.classify(host: "h", model: "m", status: 404, data: Data("<html>no</html>".utf8), secrets: []) {} else { XCTFail() }
        if case .modelMissing = ConnectionTest.classify(host: "h", model: "m", status: 404, data: Data(#"{"error":{"message":"The model `m` does not exist"}}"#.utf8), secrets: []) {} else { XCTFail() }
        if case .modelMissing = ConnectionTest.classify(host: "h", model: "m", status: 400, data: Data(#"{"error":{"message":"invalid model"}}"#.utf8), secrets: []) {} else { XCTFail() }
        if case .badURL = ConnectionTest.classify(host: "h", model: "m", status: 302, data: Data(), secrets: []) {} else { XCTFail() }
        if case .other = ConnectionTest.classify(host: "h", model: "m", status: 500, data: Data("oops".utf8), secrets: []) {} else { XCTFail() }
        if case .unreachable(let m) = ConnectionTest.outcome(for: .unreachable("dns failed"), host: "h") {
            XCTAssertTrue(m.contains("h") && m.contains("dns failed"))
        } else { XCTFail() }
        if case .badURL = ConnectionTest.outcome(for: .insecureURL, host: "h") {} else { XCTFail() }
    }

    func testProbeReportsSuccessAuthFailureAndNetworkErrorWithoutConsent() async throws {
        let client = AIClient(protocolClasses: [StubURLProtocol.self])
        StubURLProtocol.handler = { _ in (200, Data(#"{"choices":[{"message":{"content":""},"finish_reason":"length"}]}"#.utf8)) }
        let ok = await client.testConnection(ep())
        XCTAssertTrue(ok.ok, ok.message)
        XCTAssertTrue(ok.message.contains("endpoint.example"))
        let sent = String(decoding: StubURLProtocol.lastBody ?? Data(), as: UTF8.self)
        XCTAssertTrue(sent.contains("\"Hi\""), sent)
        XCTAssertFalse(sent.contains("transcript"), sent)

        StubURLProtocol.handler = { _ in (401, Data(#"{"error":{"message":"Incorrect API key provided: stub-key-value-1234"}}"#.utf8)) }
        let bad = await client.testConnection(ep())
        guard case .wrongKey(let m) = bad else { return XCTFail("\(bad)") }
        XCTAssertFalse(m.contains("stub-key-value-1234"))

        StubURLProtocol.handler = { _ in throw URLError(.cannotConnectToHost) }
        let down = await client.testConnection(ep())
        guard case .unreachable(let u) = down else { return XCTFail("\(down)") }
        XCTAssertTrue(u.hasPrefix("Couldn't reach endpoint.example"), u)

        // Misconfiguration never reaches the network
        StubURLProtocol.handler = { _ in XCTFail("no request expected"); return (200, Data()) }
        if case .badURL = await client.testConnection(ep(base: "http://public.example/v1")) {} else { XCTFail() }
        if case .badURL = await client.testConnection(ep(model: " ")) {} else { XCTFail() }
    }

    @MainActor
    func testFormTestUsesSavedEndpointAndReportsWords() async throws {
        var vault: String?
        let store = AISettings.KeyStore(get: { _ in vault }, set: { key, _ in vault = key }, delete: { _ in vault = nil })
        let s = AISettings(defaults: try XCTUnwrap(UserDefaults(suiteName: "nf-tests-\(UUID().uuidString)")), keyStore: store)
        let form = AIConnectModel()
        let client = AIClient(protocolClasses: [StubURLProtocol.self])
        await form.testConnection(s, using: client)
        XCTAssertEqual(form.testStatus, .failed("Save the connection first."))
        form.choose(AIPreset.byID("xai")!)
        form.serverKey = "synthetic-xai-key-fixture"
        form.saveEndpoint(s)
        XCTAssertFalse(s.hasConsent(.custom), "the test must not need consent")
        StubURLProtocol.handler = { req in
            XCTAssertEqual(req.url?.absoluteString, "https://api.x.ai/v1/chat/completions")
            XCTAssertEqual(req.value(forHTTPHeaderField: "Authorization"), "Bearer synthetic-xai-key-fixture")
            return (200, Data(#"{"choices":[{"message":{"content":"Hi"}}]}"#.utf8))
        }
        await form.testConnection(s, using: client)
        XCTAssertEqual(form.testStatus, .connected("Connected to api.x.ai with model grok-4.7."))
        XCTAssertFalse(form.testing)
    }
}

final class URLPolicyTests: XCTestCase {
    private func allowed(_ s: String) -> Bool { URLPolicy.isAllowed(URL(string: s)!) }

    func testCloudNeedsHTTPS() {
        XCTAssertTrue(allowed("https://api.example.com/v1"))
        XCTAssertFalse(allowed("http://api.example.com/v1"))
        XCTAssertFalse(allowed("http://example.com:11434/v1"))
        XCTAssertFalse(allowed("http://8.8.8.8/v1"))
        XCTAssertFalse(allowed("ftp://localhost/v1"))
        XCTAssertFalse(allowed("https://user:pw@api.example.com/v1"))
        XCTAssertFalse(allowed("https://api.example.com/v1?api_key=fixture"))
        XCTAssertFalse(allowed("https://api.example.com/v1#fixture"))
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
        XCTAssertTrue(URLPolicy.needsConsent(provider: .custom, baseURL: URL(string: "https://api.example.com/v1")))
        XCTAssertFalse(URLPolicy.needsConsent(provider: .custom, baseURL: URL(string: "http://192.168.1.2:11434/v1")))
        XCTAssertTrue(URLPolicy.needsConsent(provider: .custom, baseURL: URL(string: "https://my-proxy.example.com/v1")))
        XCTAssertFalse(URLPolicy.needsConsent(provider: .apple, baseURL: nil))
    }

    func testParsesTypedServerAddresses() {
        XCTAssertEqual(URLPolicy.parseBaseURL("192.168.1.5:11434/v1/")?.absoluteString, "http://192.168.1.5:11434/v1")
        XCTAssertEqual(URLPolicy.parseBaseURL("api.example.com/v1")?.absoluteString, "https://api.example.com/v1")
        XCTAssertNil(URLPolicy.parseBaseURL("   "))
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
        let invalidKey = Data(#"[{"error":{"code":400,"message":"Please pass a valid API key"}}]"#.utf8)
        if case .wrongKey = ResponseParser.statusError(400, invalidKey, secrets: []) {} else { XCTFail("invalid key response") }
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

    private func ep(key: String? = "fixture-key", base: URL? = URL(string: "https://endpoint.example/v1"),
                    model: String = "user-selected-model", consent: Bool = true) -> AIEndpoint {
        AIEndpoint(provider: .custom, baseURL: base, apiKey: key, model: model,
                   contextTokens: 32_768, consentGranted: consent)
    }

    private func body(_ r: URLRequest) throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(r.httpBody)) as? [String: Any])
    }

    func testCompatibleRequestUsesOnlyEnteredEndpointAndModel() throws {
        let r = try RequestBuilder.chat(ep(), messages: msgs, maxTokens: 700, temperature: 0.5)
        XCTAssertEqual(r.url?.absoluteString, "https://endpoint.example/v1/chat/completions")
        XCTAssertEqual(r.httpMethod, "POST")
        XCTAssertEqual(r.value(forHTTPHeaderField: "Authorization"), "Bearer fixture-key")
        XCTAssertNil(r.value(forHTTPHeaderField: "x-api-key"))
        let b = try body(r)
        XCTAssertEqual(b["model"] as? String, "user-selected-model")
        XCTAssertEqual(b["max_tokens"] as? Int, 700)
        XCTAssertEqual(b["temperature"] as? Double, 0.5)
        XCTAssertEqual(b["stream"] as? Bool, false)
        XCTAssertNil(b["response_format"])
        let sent = try XCTUnwrap(b["messages"] as? [[String: String]])
        XCTAssertEqual(sent.first?["role"], "system")
        XCTAssertEqual(r.timeoutInterval, 60 + 700.0 / 8, accuracy: 0.01)
    }

    func testReasoningTokenVariantRetainsExplicitModel() throws {
        let r = try RequestBuilder.chat(ep(), messages: msgs, maxTokens: 900, temperature: 0.5, completionTokens: true)
        let b = try body(r)
        XCTAssertEqual(b["model"] as? String, "user-selected-model")
        XCTAssertEqual(b["max_completion_tokens"] as? Int, 900)
        XCTAssertNil(b["max_tokens"])
        XCTAssertNil(b["temperature"])
    }

    func testExplicitLocalEndpointNeedsNoKey() throws {
        let r = try RequestBuilder.chat(ep(key: nil, base: URL(string: "http://127.0.0.1:8080/v1")),
                                        messages: msgs, maxTokens: 100, temperature: nil)
        XCTAssertEqual(r.url?.absoluteString, "http://127.0.0.1:8080/v1/chat/completions")
        XCTAssertNil(r.value(forHTTPHeaderField: "Authorization"))
    }

    func testMissingEndpointOrModelCannotBuildRequest() {
        for endpoint in [ep(base: nil), ep(model: " \n")] {
            XCTAssertThrowsError(try RequestBuilder.chat(endpoint, messages: msgs, maxTokens: 100, temperature: nil)) {
                XCTAssertEqual($0 as? AIError, .notConfigured)
            }
        }
    }

    func testRefusesInsecureRemoteURL() {
        XCTAssertThrowsError(try RequestBuilder.chat(ep(base: URL(string: "http://api.example.com/v1")),
                                                     messages: msgs, maxTokens: 100, temperature: nil)) {
            XCTAssertEqual($0 as? AIError, .insecureURL)
        }
    }

    func testRemoteRequestRequiresConsentBeforeNetwork() async {
        do {
            _ = try await AIClient().complete(msgs, maxTokens: 100, endpoint: ep(consent: false))
            XCTFail("Unconsented remote request must fail before transport")
        } catch {
            XCTAssertEqual(error as? AIError, .consentRequired("endpoint.example"))
        }
    }

    func testOutputBudgetLeavesRoomForThinking() {
        XCTAssertEqual(RequestBuilder.outputBudget(proto: .openai, requested: 700, completionTokens: false), 700)
        XCTAssertGreaterThan(RequestBuilder.outputBudget(proto: .openai, requested: 700, completionTokens: true), 700)
        XCTAssertLessThanOrEqual(RequestBuilder.outputBudget(proto: .openai, requested: 8000, completionTokens: true), 16_000)
    }
}

final class ResponseParsingTests: XCTestCase {
    func testCompatibleAnswer() throws {
        let d = Data(#"{"choices":[{"message":{"content":"<think>hmm</think> Hello "},"finish_reason":"stop"}]}"#.utf8)
        XCTAssertEqual(try ResponseParser.openAIText(d), "Hello")
        let cut = Data(#"{"choices":[{"message":{"content":""},"finish_reason":"length"}]}"#.utf8)
        XCTAssertThrowsError(try ResponseParser.openAIText(cut)) { XCTAssertEqual($0 as? AIError, .truncated) }
    }

    func testRefusedCompatibleAnswer() {
        let d = Data(#"{"choices":[{"message":{"refusal":"Declined"}}]}"#.utf8)
        XCTAssertThrowsError(try ResponseParser.openAIText(d)) { XCTAssertEqual($0 as? AIError, .refused) }
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
    private final class FixtureVault {
        var value: String?
        var failDeletion = false
        var reads = 0
        enum Failure: Error { case deletion }
        var store: AISettings.KeyStore {
            AISettings.KeyStore(get: { _ in self.reads += 1; return self.value },
                                set: { key, _ in self.value = key },
                                delete: { _ in
                if self.failDeletion { throw Failure.deletion }
                self.value = nil
            })
        }
    }

    private func defaults() throws -> UserDefaults {
        try XCTUnwrap(UserDefaults(suiteName: "nf-tests-\(UUID().uuidString)"))
    }

    func testExplicitLocalEndpointAndActiveProvider() throws {
        let s = AISettings(defaults: try defaults(), keyStore: FixtureVault().store)
        let url = URL(string: "http://192.168.1.9:8080/v1")!
        try s.save(.custom, key: nil, baseURL: url, models: [ModelInfo(id: "my-model", contextTokens: nil)])
        XCTAssertEqual(s.effectiveProvider, .custom)
        XCTAssertEqual(s.model(for: "custom"), "my-model")
        XCTAssertTrue(s.hasConsent(.custom))
        let ep = try XCTUnwrap(s.endpoint())
        XCTAssertEqual(ep.baseURL, url)
        XCTAssertFalse(ep.needsConsent)
        s.remove(.custom)
        XCTAssertNil(s.saved["custom"])
    }

    func testLegacySelectionsFailClosedAndPreserveSavedContent() throws {
        for id in ["openai", "anthropic", "gemini", "xai", "groq", "openrouter", "mistral",
                   "deepseek", "perplexity", "together", "ollama", "lmstudio"] {
            let d = try defaults()
            let saved = [id: AISettings.Saved(last4: "TEST", baseURL: nil, model: "previous-model", models: [])]
            d.set(try JSONEncoder().encode(saved), forKey: "ai.providers")
            d.set(id, forKey: "ai.activeProvider")
            d.set([id], forKey: "ai.consent")
            d.set("existing meeting fixture", forKey: "meeting-preservation-fixture")
            let s = AISettings(defaults: d, keyStore: FixtureVault().store)
            XCTAssertEqual(s.activeProviderID, id)
            XCTAssertEqual(s.saved, saved)
            XCTAssertNil(s.effectiveProvider, id)
            XCTAssertNil(s.endpoint(), id)
            XCTAssertFalse(s.isConfigured, id)
            XCTAssertTrue(s.needsEndpointSetup)
            XCTAssertEqual(d.string(forKey: "meeting-preservation-fixture"), "existing meeting fixture")
        }
    }

    func testMissingSavedSelectionDoesNotFallback() throws {
        let d = try defaults()
        d.set("gemini", forKey: "ai.activeProvider")
        let s = AISettings(defaults: d, keyStore: FixtureVault().store)
        XCTAssertNil(s.effectiveProvider)
        XCTAssertNil(s.endpoint())
        XCTAssertEqual(s.activeProviderID, "gemini")
    }

    func testIncompleteStoredCustomEndpointCannotRun() throws {
        for saved in [AISettings.Saved(last4: nil, baseURL: nil, model: "my-model", models: []),
                      AISettings.Saved(last4: nil, baseURL: "https://endpoint.example/v1", model: "", models: [])] {
            let d = try defaults()
            d.set(try JSONEncoder().encode(["custom": saved]), forKey: "ai.providers")
            d.set("custom", forKey: "ai.activeProvider")
            let s = AISettings(defaults: d, keyStore: FixtureVault().store)
            XCTAssertNil(s.effectiveProvider)
            XCTAssertNil(s.endpoint())
        }
    }

    func testSaveRejectsMissingEndpointAndModelBeforeChangingConfiguration() throws {
        let s = AISettings(defaults: try defaults(), keyStore: FixtureVault().store)
        XCTAssertThrowsError(try s.save(.custom, key: nil, baseURL: nil,
                                        models: [ModelInfo(id: "my-model", contextTokens: nil)]))
        XCTAssertThrowsError(try s.save(.custom, key: nil, baseURL: URL(string: "https://endpoint.example/v1"), models: []))
        XCTAssertThrowsError(try s.save(.custom, key: nil, baseURL: URL(string: "https://endpoint.example/v1"),
                                        models: [ModelInfo(id: " \n", contextTokens: nil)]))
        XCTAssertTrue(s.saved.isEmpty)
        XCTAssertNil(s.activeProviderID)
    }

    func testChangingEndpointClearsOldKeyAndSharingConsent() throws {
        let vault = FixtureVault()
        let s = AISettings(defaults: try defaults(), keyStore: vault.store)
        let models = [ModelInfo(id: "my-model", contextTokens: nil)]
        try s.save(.custom, key: "synthetic-endpoint-A-key", baseURL: URL(string: "https://first.example/v1"), models: models)
        s.grantConsent(.custom)
        XCTAssertTrue(s.hasConsent(.custom))
        XCTAssertEqual(s.endpoint()?.apiKey, "synthetic-endpoint-A-key")
        try s.save(.custom, key: nil, baseURL: URL(string: "https://second.example/v1"), models: models)
        XCTAssertNil(s.endpoint()?.apiKey)
        XCTAssertNil(s.saved["custom"]?.last4)
        XCTAssertFalse(s.hasConsent(.custom))
        XCTAssertEqual(s.endpoint()?.baseURL?.host(), "second.example")
    }

    func testNewKeyMustBeExplicitForChangedEndpoint() throws {
        let vault = FixtureVault()
        let s = AISettings(defaults: try defaults(), keyStore: vault.store)
        let models = [ModelInfo(id: "my-model", contextTokens: nil)]
        try s.save(.custom, key: "synthetic-endpoint-A-key", baseURL: URL(string: "https://first.example/v1"), models: models)
        s.grantConsent(.custom)
        try s.save(.custom, key: "synthetic-endpoint-B-key", baseURL: URL(string: "https://second.example/v1"), models: models)
        XCTAssertEqual(s.endpoint()?.apiKey, "synthetic-endpoint-B-key")
        XCTAssertFalse(s.hasConsent(.custom))
    }

    func testFailedKeyDeletionCannotChangeDestinationOrConsent() throws {
        let vault = FixtureVault()
        let s = AISettings(defaults: try defaults(), keyStore: vault.store)
        let models = [ModelInfo(id: "my-model", contextTokens: nil)]
        let originalURL = URL(string: "https://first.example/v1")!
        try s.save(.custom, key: "synthetic-original-key", baseURL: originalURL, models: models)
        s.grantConsent(.custom)
        vault.failDeletion = true
        XCTAssertThrowsError(try s.save(.custom, key: nil,
                                        baseURL: URL(string: "https://second.example/v1"), models: models))
        XCTAssertEqual(s.endpoint()?.baseURL, originalURL)
        XCTAssertEqual(s.endpoint()?.apiKey, "synthetic-original-key")
        XCTAssertTrue(s.hasConsent(.custom))
    }

    func testKeylessSavedEndpointNeverReadsOrphanedCredential() throws {
        let d = try defaults()
        let saved = AISettings.Saved(last4: nil, baseURL: "http://127.0.0.1:8080/v1", model: "my-model", models: [])
        d.set(try JSONEncoder().encode(["custom": saved]), forKey: "ai.providers")
        d.set("custom", forKey: "ai.activeProvider")
        let vault = FixtureVault()
        vault.value = "synthetic-orphaned-key"
        let s = AISettings(defaults: d, keyStore: vault.store)
        XCTAssertNotNil(s.endpoint())
        XCTAssertNil(s.endpoint()?.apiKey)
        XCTAssertEqual(vault.reads, 0)
    }

    func testSetupStartsBlankAndRejectsKeyOnlyInput() throws {
        let s = AISettings(defaults: try defaults(), keyStore: FixtureVault().store)
        let form = AIConnectModel()
        XCTAssertTrue(form.serverURL.isEmpty)
        XCTAssertTrue(form.serverModel.isEmpty)
        XCTAssertTrue(form.serverKey.isEmpty)
        form.serverKey = "synthetic-key-never-probed"
        XCTAssertFalse(form.canSave)
        form.saveEndpoint(s)
        XCTAssertTrue(s.saved.isEmpty)
        if case .failed = form.status {} else { XCTFail("Key-only setup must require an endpoint") }
    }

    func testSetupStoresExplicitConfigurationWithoutConnecting() throws {
        let s = AISettings(defaults: try defaults(), keyStore: FixtureVault().store)
        let form = AIConnectModel()
        form.serverURL = "https://unreachable-fixture.invalid/v1"
        form.serverModel = "entered-model"
        XCTAssertTrue(form.canSave)
        form.saveEndpoint(s)
        // An unreachable host can be saved because saving is local and never probes it.
        XCTAssertEqual(s.endpoint()?.baseURL?.host(), "unreachable-fixture.invalid")
        XCTAssertEqual(s.endpoint()?.model, "entered-model")
        XCTAssertEqual(form.consentPrompt, .custom)
        XCTAssertFalse(s.hasConsent(.custom))
        if case .connected = form.status {} else { XCTFail("Explicit setup should be saved locally") }
    }
}
