import AuthenticationServices
import CryptoKit
import Foundation
import Network
import UIKit

/// A preset's own one-click sign-in (docs/AI_PROVIDERS.md "Connecting"):
/// OpenRouter's OAuth PKCE, which issues the user's own key.
///
/// 1. A loopback-only listener starts on this iPhone (127.0.0.1, loopback
///    interface): nothing off the device can reach it.
/// 2. Apple's web sign-in sheet opens the service's page with
///    `callback_url=http://localhost:<port>/callback` and an S256 challenge.
/// 3. When the user approves, the page goes to the callback; the listener
///    takes `code`, answers "Signed in", and the sheet closes.
/// 4. `code` + the verifier are exchanged for the key, which goes straight to
///    the Keychain through `AISettings.save` (never shown).
/// Runs only from the Sign in button. It sends nothing of the user's.
@MainActor
final class ProviderSignIn: NSObject {
    enum Failure: LocalizedError {
        case cancelled, denied, timeout, listener, interrupted, exchange(String)
        var errorDescription: String? {
            switch self {
            case .cancelled: "Sign-in cancelled."
            case .denied: "The sign-in was cancelled on the page."
            case .timeout: "The sign-in timed out. Try again."
            case .listener: "Couldn't start the sign-in. Try again."
            case .interrupted: "The sign-in was interrupted (noFriction was in the background too long). Try again."
            case .exchange(let s): s
            }
        }
    }

    struct PKCE: Equatable {
        let verifier: String
        let challenge: String
        let state: String

        static func make() -> PKCE {
            from(verifier: randomURLSafe(48), state: randomURLSafe(18))
        }

        static func from(verifier: String, state: String) -> PKCE {
            PKCE(verifier: verifier, challenge: base64URL(Data(SHA256.hash(data: Data(verifier.utf8)))), state: state)
        }

        func authorizeURL(_ signIn: String, callback: String) -> URL? {
            guard var c = URLComponents(string: signIn) else { return nil }
            c.queryItems = (c.queryItems ?? []) + [
                URLQueryItem(name: "callback_url", value: callback),
                URLQueryItem(name: "code_challenge", value: challenge),
                URLQueryItem(name: "code_challenge_method", value: "S256"),
                URLQueryItem(name: "state", value: state),
                URLQueryItem(name: "key_label", value: "noFriction"),
            ]
            // "+" and ":" in values must be escaped for the service's parser
            c.percentEncodedQuery = c.percentEncodedQuery?
                .replacingOccurrences(of: "+", with: "%2B")
                .replacingOccurrences(of: ":", with: "%3A")
            return c.url
        }
    }

    /// `other`: not our callback (a favicon, a stray or forged request); it is
    /// answered and the sign-in keeps waiting, so only the real callback ends it.
    enum Callback: Equatable {
        case code(String), denied, other
    }

    /// Read `GET /callback?code=…&state=… HTTP/1.1`. A code counts only with our `state`.
    nonisolated static func parse(_ request: String, state: String) -> Callback {
        guard let line = request.split(separator: "\r\n").first ?? request.split(separator: "\n").first else { return .other }
        let parts = line.split(separator: " ")
        guard parts.count >= 2, let c = URLComponents(string: "http://localhost" + parts[1]), c.path == "/callback" else { return .other }
        let get = { (k: String) in c.queryItems?.first { $0.name == k }?.value }
        if let code = get("code"), get("state") == state, !code.isEmpty, code.count <= 2048 { return .code(code) }
        if get("code") == nil, get("error") != nil { return .denied }
        return .other
    }

    // MARK: Flow

    typealias Exchange = @Sendable (_ code: String, _ verifier: String) async throws -> String
    typealias Open = @MainActor (_ url: URL) -> Void

    private var listener: NWListener?
    private var session: ASWebAuthenticationSession?
    private var continuation: CheckedContinuation<String, Error>?
    private var timer: Task<Void, Never>?
    /// Cancel was tapped: honoured at every step, not only while waiting
    private var cancelled = false

    /// The sign-in, with the browser step and the exchange injectable (tests).
    func run(signInURL: String, open: Open? = nil, exchange: Exchange, timeout: Duration = .seconds(600)) async throws -> String {
        cancelled = false
        let pkce = PKCE.make()
        let port = try await startListener(state: pkce.state)
        defer { stop() }
        if cancelled { throw Failure.cancelled }
        guard let url = pkce.authorizeURL(signInURL, callback: "http://localhost:\(port)/callback") else { throw Failure.listener }
        let code: String = try await withCheckedThrowingContinuation { cont in
            continuation = cont
            // The timer resumes the same continuation, so nothing is left waiting
            timer = Task { @MainActor [weak self] in
                try? await Task.sleep(for: timeout)
                if !Task.isCancelled { self?.finish(.failure(Failure.timeout)) }
            }
            if let open { open(url) } else { present(url) }
        }
        timer?.cancel()
        session?.cancel()
        if cancelled { throw Failure.cancelled }
        let key = try await exchange(code, pkce.verifier)
        if cancelled { throw Failure.cancelled }
        return key
    }

    /// Stop waiting (the user tapped Cancel).
    func cancel() {
        cancelled = true
        finish(.failure(Failure.cancelled))
        session?.cancel()
    }

    private func finish(_ result: Result<String, Error>) {
        guard let c = continuation else { return }
        continuation = nil
        c.resume(with: result)
    }

    private func stop() {
        timer?.cancel()
        timer = nil
        listener?.cancel()
        listener = nil
        session = nil
    }

    private func present(_ url: URL) {
        // The callback is the loopback page, not a scheme: the sheet is closed
        // by `run` once the listener has the code. The scheme below never fires.
        let s = ASWebAuthenticationSession(url: url, callbackURLScheme: "nofriction-signin") { [weak self] _, error in
            Task { @MainActor in
                if let e = error as? ASWebAuthenticationSessionError, e.code == .canceledLogin {
                    self?.finish(.failure(Failure.cancelled))
                }
            }
        }
        s.presentationContextProvider = self
        // Reuse the user's Safari sign-in to the service: fewer steps
        s.prefersEphemeralWebBrowserSession = false
        session = s
        if !s.start() { finish(.failure(Failure.listener)) }
    }

    private func startListener(state: String) async throws -> UInt16 {
        let params = NWParameters.tcp
        // Bound to 127.0.0.1: nothing off this iPhone can connect
        params.requiredLocalEndpoint = NWEndpoint.hostPort(host: .ipv4(.loopback), port: .any)
        let l = try NWListener(using: params)
        listener = l
        l.newConnectionHandler = { [weak self] conn in
            // And only from this device (the bind already guarantees it)
            guard Self.isLoopback(conn.endpoint) else { conn.cancel(); return }
            conn.start(queue: .main)
            conn.receive(minimumIncompleteLength: 1, maximumLength: 8192) { data, _, _, _ in
                let request = data.flatMap { String(data: $0, encoding: .utf8) } ?? ""
                let result = ProviderSignIn.parse(request, state: state)
                let page: String
                switch result {
                case .code: page = Self.page("Signed in")
                case .denied: page = Self.page("Sign-in didn't finish")
                case .other: page = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                }
                conn.send(content: Data(page.utf8), completion: .contentProcessed { _ in conn.cancel() })
                Task { @MainActor in
                    switch result {
                    case .code(let code): self?.finish(.success(code))
                    case .denied: self?.finish(.failure(Failure.denied))
                    case .other: break
                    }
                }
            }
        }
        return try await withCheckedThrowingContinuation { cont in
            var resumed = false
            l.stateUpdateHandler = { [weak self] st in
                guard !resumed else {
                    // iOS can reclaim the socket while the app is in the
                    // background: fail now instead of waiting for the timer
                    if case .failed = st { Task { @MainActor in self?.finish(.failure(Failure.interrupted)) } }
                    return
                }
                switch st {
                case .ready:
                    resumed = true
                    cont.resume(returning: l.port?.rawValue ?? 0)
                case .failed, .cancelled:
                    resumed = true
                    cont.resume(throwing: Failure.listener)
                default: break
                }
            }
            l.start(queue: .main)
        }
    }

    nonisolated static func isLoopback(_ endpoint: NWEndpoint) -> Bool {
        guard case .hostPort(let host, _) = endpoint else { return false }
        switch host {
        case .ipv4(let a): return a.isLoopback
        case .ipv6(let a): return a.isLoopback || a == .loopback
        case .name(let n, _): return n == "localhost"
        @unknown default: return false
        }
    }

    nonisolated private static func page(_ title: String) -> String {
        let body = "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width\"><title>noFriction</title><body style=\"font:17px -apple-system,sans-serif;margin:20vh 24px;text-align:center\"><h2>\(title)</h2><p>Go back to noFriction.</p></body>"
        return "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: \(body.utf8.count)\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n\(body)"
    }

    // MARK: Exchange

    /// Trade the one-time code for the user's own key (the preset's exchange URL).
    nonisolated static func exchange(at urlString: String) -> Exchange {
        { code, verifier in
            guard let url = URL(string: urlString), url.scheme == "https" else { throw Failure.exchange("The sign-in address is invalid.") }
            var req = URLRequest(url: url, timeoutInterval: 30)
            req.httpMethod = "POST"
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
            req.httpBody = try JSONSerialization.data(withJSONObject: ["code": code, "code_verifier": verifier, "code_challenge_method": "S256"])
            let data: Data
            let response: URLResponse
            do {
                (data, response) = try await URLSession.shared.data(for: req)
            } catch {
                throw Failure.exchange("Couldn't reach the service to finish signing in. Check your internet connection.")
            }
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            if status == 400 || status == 403 { throw Failure.exchange("The service didn't accept the sign-in (it may have taken too long). Try again.") }
            guard (200..<300).contains(status) else { throw Failure.exchange("The service couldn't finish the sign-in (HTTP \(status)). Try again in a moment.") }
            return try key(fromExchange: data)
        }
    }

    nonisolated static func key(fromExchange data: Data) throws -> String {
        guard let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any], let raw = o["key"] as? String else {
            throw Failure.exchange("The service didn't send a key.")
        }
        let key = KeyDetector.normalize(raw)
        guard key.count >= 8, key.count <= 512 else { throw Failure.exchange("The service sent an unusable key.") }
        return key
    }

    nonisolated private static func base64URL(_ d: Data) -> String {
        d.base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
    }

    nonisolated private static func randomURLSafe(_ bytes: Int) -> String {
        var b = [UInt8](repeating: 0, count: bytes)
        _ = SecRandomCopyBytes(kSecRandomDefault, bytes, &b)
        return base64URL(Data(b))
    }
}

extension ProviderSignIn: ASWebAuthenticationPresentationContextProviding {
    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
                .flatMap(\.windows).first { $0.isKeyWindow } ?? ASPresentationAnchor()
        }
    }
}
