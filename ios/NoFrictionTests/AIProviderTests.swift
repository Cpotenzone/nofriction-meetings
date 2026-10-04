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
