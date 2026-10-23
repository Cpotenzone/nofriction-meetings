import XCTest
@testable import noFriction

/// docs/AI_PROVIDERS.md "Connecting", iOS side: Connect in one tap, Paste,
/// and the OpenRouter sign-in (PKCE, loopback callback, key exchange). The
/// network is a URLProtocol stub; the sign-in's "browser" is a URLSession
/// request to the loopback listener.
final class ConnectTests: XCTestCase {
    final class Vault { var key: String? }

    @MainActor
    private func settings(_ vault: Vault) throws -> AISettings {
        let store = AISettings.KeyStore(get: { _ in vault.key }, set: { k, _ in vault.key = k }, delete: { _ in vault.key = nil })
        return AISettings(defaults: try XCTUnwrap(UserDefaults(suiteName: "nf-tests-\(UUID().uuidString)")), keyStore: store)
    }

    func testPKCEIsRFC7636S256AndTheVerifierStaysHere() {
        let p = ProviderSignIn.PKCE.from(verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk", state: "st")
        XCTAssertEqual(p.challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM")
        let a = ProviderSignIn.PKCE.make(), b = ProviderSignIn.PKCE.make()
        XCTAssertNotEqual(a.verifier, b.verifier)
        XCTAssertTrue((43...128).contains(a.verifier.count))
        let url = a.authorizeURL("https://sign-in.example/auth", callback: "http://localhost:5000/callback")!.absoluteString
        XCTAssertTrue(url.hasPrefix("https://sign-in.example/auth?callback_url=http%3A//localhost%3A5000/callback&code_challenge=") ||
                      url.hasPrefix("https://sign-in.example/auth?callback_url=http%3A%2F%2Flocalhost%3A5000%2Fcallback&code_challenge="), url)
        XCTAssertTrue(url.contains("code_challenge_method=S256"))
        XCTAssertFalse(url.contains(a.verifier))
    }

    func testOnlyLoopbackPeersAreServed() {
        XCTAssertTrue(ProviderSignIn.isLoopback(.hostPort(host: .ipv4(.loopback), port: 5000)))
        XCTAssertTrue(ProviderSignIn.isLoopback(.hostPort(host: .ipv6(.loopback), port: 5000)))
        XCTAssertFalse(ProviderSignIn.isLoopback(.hostPort(host: "192.168.1.20", port: 5000)))
    }

    func testCallbackParsing() {
        XCTAssertEqual(ProviderSignIn.parse("GET /callback?code=abc&state=s1 HTTP/1.1\r\nHost: x\r\n\r\n", state: "s1"), .code("abc"))
        // Without our state a code is ignored and doesn't end the sign-in
        XCTAssertEqual(ProviderSignIn.parse("GET /callback?code=abc&state=zz HTTP/1.1", state: "s1"), .other)
        XCTAssertEqual(ProviderSignIn.parse("GET /callback?code=abc HTTP/1.1", state: "s1"), .other)
        XCTAssertEqual(ProviderSignIn.parse("GET /callback HTTP/1.1", state: "s1"), .other)
        XCTAssertEqual(ProviderSignIn.parse("GET /callback?error=access_denied HTTP/1.1", state: "s1"), .denied)
        XCTAssertEqual(ProviderSignIn.parse("GET /favicon.ico HTTP/1.1", state: "s1"), .other)
        XCTAssertEqual(ProviderSignIn.parse("", state: "s1"), .other)
    }

    func testExchangeAnswerAndPastedKeys() throws {
        XCTAssertEqual(try ProviderSignIn.key(fromExchange: Data(#"{"key":" or-test-key-1234567890 "}"#.utf8)), "or-test-key-1234567890")
        XCTAssertThrowsError(try ProviderSignIn.key(fromExchange: Data(#"{"nope":1}"#.utf8)))
        XCTAssertThrowsError(try ProviderSignIn.key(fromExchange: Data(#"{"key":"x"}"#.utf8)))
        XCTAssertEqual(try AIConnectModel.key(fromPaste: ["  \"fixture-key-1234567890\"\n"], host: "console.x.ai").get(), "fixture-key-1234567890")
        XCTAssertEqual(try AIConnectModel.key(fromPaste: ["Bearer fixture-key-1234567890"], host: "h").get(), "fixture-key-1234567890")
        XCTAssertEqual(try AIConnectModel.key(fromPaste: ["export XAI_API_KEY=fixture-key-1234567890"], host: "h").get(), "fixture-key-1234567890")
        for bad in [[], [""], ["Here are the notes from the meeting today"], ["https://console.x.ai/keys"], ["short"],
                    ["Tr0ub4dor&3!horse#battery"], ["abcdefghij\nklmnopqrst\nuvw"], ["fixture-key-12345"]] {
            guard case .failure(let e) = AIConnectModel.key(fromPaste: bad, host: "console.x.ai") else { return XCTFail("\(bad)") }
            XCTAssertEqual(e.message, "Copy your key on console.x.ai first, then tap Paste again.")
        }
    }

    @MainActor
    func testConnectSavesTestsAndAsksConsentInOneTap() async throws {
        let vault = Vault()
        let s = try settings(vault)
        let form = AIConnectModel()
        let client = AIClient(protocolClasses: [StubURLProtocol.self])
        form.choose(AIPreset.byID("xai")!)
        await form.connect(s, using: client)
        XCTAssertEqual(form.status, .failed("Paste your key first."), "a key service needs a key")
        XCTAssertNil(s.saved["custom"])

        StubURLProtocol.handler = { req in
            XCTAssertEqual(req.url?.absoluteString, "https://api.x.ai/v1/chat/completions")
            XCTAssertEqual(req.value(forHTTPHeaderField: "Authorization"), "Bearer fixture-key-1234567890")
            return (200, Data(#"{"choices":[{"message":{"content":"Hi"}}]}"#.utf8))
        }
        await form.pasteAndConnect(s, ["fixture-key-1234567890"], using: client)
        XCTAssertEqual(form.status, .connected("Connected to Grok (xAI) · grok-4.7."))
        XCTAssertEqual(vault.key, "fixture-key-1234567890")
        XCTAssertEqual(form.serverKey, "", "the key is never kept in the form")
        XCTAssertEqual(form.consentPrompt, .custom, "a public endpoint asks before recordings are sent")
        let sent = String(decoding: StubURLProtocol.lastBody ?? Data(), as: UTF8.self)
        XCTAssertTrue(sent.contains("\"Hi\""), sent)

        // A typed key that isn't one is refused before anything changes
        await form.connect(s, key: "x y", using: client)
        XCTAssertEqual(form.status, .failed("That doesn't look like an API key. Copy it again and paste it."))
        XCTAssertEqual(vault.key, "fixture-key-1234567890", "the working key is untouched")

        // A wrong key says what to do
        StubURLProtocol.handler = { _ in (401, Data(#"{"error":{"message":"bad key"}}"#.utf8)) }
        form.consentPrompt = nil
        await form.connect(s, key: "other-fixture-key-123456", using: client)
        guard case .failed(let m) = form.status else { return XCTFail("\(form.status)") }
        XCTAssertTrue(m.hasSuffix("Copy the key again on console.x.ai, then paste it."), m)
        XCTAssertNil(form.consentPrompt)

        // Reconnecting the same endpoint keeps the saved key
        StubURLProtocol.handler = { req in
            XCTAssertEqual(req.value(forHTTPHeaderField: "Authorization"), "Bearer other-fixture-key-123456")
            return (200, Data(#"{"choices":[{"message":{"content":"Hi"}}]}"#.utf8))
        }
        await form.connect(s, using: client)
        XCTAssertEqual(form.status, .connected("Connected to Grok (xAI) · grok-4.7."))
    }

    @MainActor
    func testLocalEndpointConnectsWithoutAKeyOrConsent() async throws {
        let vault = Vault()
        let s = try settings(vault)
        let form = AIConnectModel()
        form.chooseCustom(s)
        form.serverURL = "http://192.168.1.9:11434/v1"
        form.serverModel = "llama3.2"
        StubURLProtocol.handler = { _ in (200, Data(#"{"choices":[{"message":{"content":"Hi"}}]}"#.utf8)) }
        await form.connect(s, using: AIClient(protocolClasses: [StubURLProtocol.self]))
        XCTAssertEqual(form.status, .connected("Connected to Your AI endpoint · llama3.2."))
        XCTAssertNil(form.consentPrompt, "a private-network endpoint needs no consent")
        XCTAssertNil(vault.key)
    }

    @MainActor
    func testSignInRoundTripThroughTheLoopbackListener() async throws {
        let flow = ProviderSignIn()
        let key = try await flow.run(signInURL: "https://sign-in.example/auth", open: { url in
            // The "browser": a stray request, then the callback with our state
            let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
            let callback = items.first { $0.name == "callback_url" }!.value!.replacingOccurrences(of: "localhost", with: "127.0.0.1")
            let state = items.first { $0.name == "state" }!.value!
            XCTAssertEqual(items.first { $0.name == "code_challenge_method" }?.value, "S256")
            Task.detached {
                let session = URLSession(configuration: .ephemeral)
                _ = try? await session.data(from: URL(string: callback.replacingOccurrences(of: "/callback", with: "/favicon.ico"))!)
                // A forged callback (wrong state) is answered and ignored
                _ = try? await session.data(from: URL(string: "\(callback)?code=evil&state=wrong")!)
                let (data, _) = try await session.data(from: URL(string: "\(callback)?code=one-time-code&state=\(state)")!)
                XCTAssertTrue(String(decoding: data, as: UTF8.self).contains("Signed in"))
            }
        }, exchange: { code, verifier in
            XCTAssertEqual(code, "one-time-code")
            XCTAssertGreaterThanOrEqual(verifier.count, 43)
            return "or-test-key-1234567890"
        }, timeout: .seconds(20))
        XCTAssertEqual(key, "or-test-key-1234567890")
    }

    @MainActor
    func testSignInCanBeCancelledAndTimesOut() async {
        let flow = ProviderSignIn()
        let cancelled = Task { @MainActor in
            try await flow.run(signInURL: "https://sign-in.example/auth", open: { _ in }, exchange: { _, _ in "never" })
        }
        try? await Task.sleep(for: .milliseconds(200))
        flow.cancel()
        do { _ = try await cancelled.value; XCTFail("expected cancel") } catch ProviderSignIn.Failure.cancelled {} catch { XCTFail("\(error)") }

        let slow = ProviderSignIn()
        do {
            _ = try await slow.run(signInURL: "https://sign-in.example/auth", open: { _ in }, exchange: { _, _ in "never" }, timeout: .milliseconds(300))
            XCTFail("expected timeout")
        } catch ProviderSignIn.Failure.timeout {} catch { XCTFail("\(error)") }
    }
}
