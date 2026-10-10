import Foundation
import Network
import Security

/// One TLS connection to a Mac, pinned to its certificate's SHA-256
/// (docs/SYNC.md "Discovery and transport"). Local network only.
final class SyncConnection: @unchecked Sendable {
    private let connection: NWConnection
    private let queue = DispatchQueue(label: "com.nofriction.sync.connection")

    init(endpoint: NWEndpoint, fingerprint: String) {
        let tls = NWProtocolTLS.Options()
        let sec = tls.securityProtocolOptions
        sec_protocol_options_set_min_tls_protocol_version(sec, .TLSv12)
        let pin = fingerprint.lowercased()
        sec_protocol_options_set_verify_block(sec, { _, trust, complete in
            let ref = sec_trust_copy_ref(trust).takeRetainedValue()
            guard let chain = SecTrustCopyCertificateChain(ref) as? [SecCertificate], let leaf = chain.first else {
                complete(false)
                return
            }
            let der = SecCertificateCopyData(leaf) as Data
            complete(SyncCrypto.sha256Hex(der) == pin)
        }, queue)
        let params = NWParameters(tls: tls, tcp: NWProtocolTCP.Options())
        params.includePeerToPeer = false
        params.prohibitedInterfaceTypes = [.cellular]
        connection = NWConnection(to: endpoint, using: params)
    }

    /// Connect, or fail after `timeout` seconds.
    func start(timeout: TimeInterval = 8) async throws {
        try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Void, Error>) in
            let once = Once()
            connection.stateUpdateHandler = { state in
                switch state {
                case .ready: once.run { cont.resume() }
                case .failed(let e): once.run { cont.resume(throwing: SyncError.unreachable(e.localizedDescription)) }
                case .waiting(let e):
                    // A TLS failure here means the certificate didn't match the pin
                    if case .tls = e { once.run { cont.resume(throwing: SyncError.wrongMac) } }
                case .cancelled: once.run { cont.resume(throwing: SyncError.unreachable("cancelled")) }
                default: break
                }
            }
            connection.start(queue: queue)
            queue.asyncAfter(deadline: .now() + timeout) { [weak self] in
                once.run {
                    self?.connection.cancel()
                    cont.resume(throwing: SyncError.unreachable("timed out"))
                }
            }
        }
    }

    func send(_ m: SyncMessage) async throws {
        let data = m.frame()
        try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Void, Error>) in
            connection.send(content: data, completion: .contentProcessed { e in
                if let e { cont.resume(throwing: SyncError.unreachable(e.localizedDescription)) } else { cont.resume() }
            })
        }
    }

    private func read(_ n: Int) async throws -> Data {
        var out = Data()
        while out.count < n {
            let chunk: Data = try await withCheckedThrowingContinuation { cont in
                connection.receive(minimumIncompleteLength: 1, maximumLength: n - out.count) { data, _, done, e in
                    if let e { cont.resume(throwing: SyncError.unreachable(e.localizedDescription)) }
                    else if let data, !data.isEmpty { cont.resume(returning: data) }
                    else if done { cont.resume(throwing: SyncError.unreachable("the Mac closed the connection")) }
                    else { cont.resume(returning: Data()) }
                }
            }
            out.append(chunk)
        }
        return out
    }

    func receive() async throws -> SyncMessage {
        let len = try await read(4).reduce(0) { ($0 << 8) | Int($1) }
        guard len <= SyncWire.maxFrame else { throw SyncWireError.frameTooLarge }
        return try SyncMessage.decode(try await read(len))
    }

    func cancel() { connection.cancel() }
}

private final class Once: @unchecked Sendable {
    private var done = false
    private let lock = NSLock()
    func run(_ f: () -> Void) {
        lock.lock()
        defer { lock.unlock() }
        guard !done else { return }
        done = true
        f()
    }
}

enum SyncError: LocalizedError, Equatable {
    case notPro
    case notFound(String)
    case unreachable(String)
    case wrongMac
    case refused(code: String, message: String)
    case protocolError(String)

    var errorDescription: String? {
        switch self {
        case .notPro: "Sync is part of noFriction Pro."
        case .notFound(let name): "Couldn't find \(name) on this Wi-Fi. Open noFriction on your Mac and turn on Sync."
        case .unreachable(let why): "Couldn't reach your Mac (\(why))."
        case .wrongMac: "That Mac's identity changed. Pair again."
        case .refused(_, let message): message
        case .protocolError(let m): "Sync stopped: \(m)"
        }
    }

    /// The Mac doesn't know this iPhone any more: pair again
    var needsPairing: Bool {
        if case .refused(let code, _) = self { return code == "unknown_device" || code == "bad_proof" }
        return self == .wrongMac
    }
}

/// Finds a paired Mac by its id in the Bonjour TXT record.
enum SyncDiscovery {
    static func find(macID: String, timeout: TimeInterval = 4) async -> NWEndpoint? {
        await withCheckedContinuation { cont in
            let browser = NWBrowser(for: .bonjourWithTXTRecord(type: SyncWire.serviceType, domain: nil), using: .tcp)
            let once = Once()
            let queue = DispatchQueue(label: "com.nofriction.sync.browse")
            browser.browseResultsChangedHandler = { results, _ in
                for r in results {
                    if case .bonjour(let txt) = r.metadata, txt["id"].flatMap(SyncIDs.wire) == macID {
                        once.run {
                            browser.cancel()
                            cont.resume(returning: r.endpoint)
                        }
                    }
                }
            }
            browser.stateUpdateHandler = { state in
                if case .failed = state { once.run { browser.cancel(); cont.resume(returning: nil) } }
            }
            browser.start(queue: queue)
            queue.asyncAfter(deadline: .now() + timeout) {
                once.run { browser.cancel(); cont.resume(returning: nil) }
            }
        }
    }

    /// Bonjour first, then the addresses in the pairing link.
    static func endpoints(for state: SyncMacState) async -> [NWEndpoint] {
        var out: [NWEndpoint] = []
        if let e = await find(macID: state.macID) { out.append(e) }
        if let port = NWEndpoint.Port(rawValue: state.port) {
            out += state.hosts.map { .hostPort(host: NWEndpoint.Host($0), port: port) }
        }
        return out
    }

    static func connect(_ state: SyncMacState) async throws -> SyncConnection {
        let endpoints = await endpoints(for: state)
        guard !endpoints.isEmpty else { throw SyncError.notFound(state.name) }
        var last: Error = SyncError.notFound(state.name)
        for e in endpoints {
            let c = SyncConnection(endpoint: e, fingerprint: state.fingerprint)
            do {
                try await c.start()
                return c
            } catch {
                c.cancel()
                last = error
                if (error as? SyncError) == .wrongMac { throw error }
            }
        }
        throw (last as? SyncError) == .unreachable("timed out") ? SyncError.notFound(state.name) : last
    }
}

/// The iPhone's side of pairing and of a session (docs/SYNC.md "Session").
@MainActor
enum SyncSession {
    /// Pair with the Mac in a pairing link; returns its saved state.
    static func pair(_ link: PairingLink, deviceName: String, connect: ((SyncMacState) async throws -> SyncConnection)? = nil) async throws -> SyncMacState {
        var state = SyncMacState(macID: link.macID, name: link.name, fingerprint: link.fingerprint, hosts: link.hosts, port: link.port)
        let c = try await (connect ?? SyncDiscovery.connect)(state)
        defer { c.cancel() }
        try await c.send(.pair(code: link.code, deviceID: SyncStore.deviceID, name: deviceName))
        switch try await c.receive() {
        case .paired(let macID, let name, let secret):
            guard SyncIDs.wire(macID) == link.macID, let s = Data(base64Encoded: secret), s.count == 32 else {
                throw SyncError.protocolError("unexpected pairing answer")
            }
            try SyncStore.setSecret(s, for: link.macID)
            state.name = name
            SyncStore.save(state)
            return state
        case .error(let code, let message): throw SyncError.refused(code: code, message: message)
        default: throw SyncError.protocolError("unexpected pairing answer")
        }
    }

    struct Summary: Equatable {
        var sent = 0
        var received = 0
        var errors: [String] = []
    }

    /// One full session with a paired Mac.
    static func run(_ state: inout SyncMacState, engine makeEngine: (Data) -> SyncEngine,
                    connect: ((SyncMacState) async throws -> SyncConnection)? = nil) async throws -> Summary {
        guard let secret = SyncStore.secret(for: state.macID) else {
            throw SyncError.refused(code: "unknown_device", message: "This iPhone isn't paired with \(state.name) any more. Pair again.")
        }
        let engine = makeEngine(SyncCrypto.tokenKey(secret: secret))
        engine.assignLineIDs()
        let c = try await (connect ?? SyncDiscovery.connect)(state)
        defer { c.cancel() }
        var summary = Summary()

        // Hello / challenge / auth
        let nonceP = SyncCrypto.random(32)
        try await c.send(.hello(deviceID: SyncStore.deviceID, nonce: nonceP.base64EncodedString()))
        let (nonceM, proofM): (Data, Data)
        switch try await c.receive() {
        case .challenge(let n, let p):
            guard let n = Data(base64Encoded: n), n.count == 32, let p = Data(base64Encoded: p) else { throw SyncError.protocolError("bad challenge") }
            (nonceM, proofM) = (n, p)
        case .error(let code, let message): throw SyncError.refused(code: code, message: message)
        default: throw SyncError.protocolError("expected a challenge")
        }
        guard SyncCrypto.equal(proofM, SyncCrypto.macProof(secret: secret, noncePhone: nonceP, nonceMac: nonceM)) else {
            throw SyncError.refused(code: "bad_proof", message: "\(state.name) couldn't prove it's the Mac you paired with. Pair again.")
        }
        try await c.send(.auth(proof: SyncCrypto.phoneProof(secret: secret, nonceMac: nonceM, noncePhone: nonceP).base64EncodedString()))
        switch try await c.receive() {
        case .welcome(_, let name): state.name = name
        case .error(let code, let message): throw SyncError.refused(code: code, message: message)
        default: throw SyncError.protocolError("expected welcome")
        }

        // 1. Our removals first
        let removals = engine.removals(for: state)
        let retry1 = try await push(removals, phase: .removals, over: c)
        engine.confirmSent(removals, retry: retry1, state: &state)
        summary.sent += removals.count - retry1.count
        SyncStore.save(state)

        // 2. The Mac's removals and changes
        try await c.send(.pull(since: state.since))
        var incoming: [SyncItem] = []
        var upto: Int64?
        while upto == nil {
            switch try await c.receive() {
            case .batch(_, let items, let last, let u):
                incoming += items
                if last { upto = u ?? state.since }
            case .error(let code, let message): throw SyncError.refused(code: code, message: message)
            default: throw SyncError.protocolError("expected a batch")
            }
        }
        let report = await engine.apply(incoming, state: &state)
        summary.received = report.applied
        summary.errors += report.errors
        // The files of the Mac's photos and screens we don't have
        if !report.want.isEmpty {
            let (added, failed) = try await fetchBlobs(report.want, over: c, engine: engine, state: &state)
            summary.received += added
            if failed > 0 { summary.errors.append("\(failed) photo\(failed == 1 ? "" : "s") or screen\(failed == 1 ? "" : "s") didn't arrive; trying again next time.") }
        }
        if summary.errors.isEmpty, let upto { state.since = max(state.since, upto) }
        SyncStore.save(state)

        // 3. Our changes
        let changes = engine.changes(for: state)
        let retry2 = try await push(changes, phase: .changes, over: c, files: { engine.screenFile($0) })
        engine.confirmSent(changes, retry: retry2, state: &state)
        summary.sent += changes.count - retry2.count
        try? await c.send(.done)
        state.lastSyncedAt = .now
        state.lastError = summary.errors.first
        SyncStore.save(state)
        return summary
    }

    /// Send items; if the Mac asks for photo/screen files, send them too.
    /// Returns the ids to send again next time.
    private static func push(_ items: [SyncItem], phase: SyncPhase, over c: SyncConnection,
                             files: (String) -> Data? = { _ in nil }) async throws -> Set<String> {
        let chunks = SyncMessage.batches(items)
        for (i, chunk) in chunks.enumerated() {
            try await c.send(.batch(phase: phase, items: chunk, last: i == chunks.count - 1, upto: nil))
        }
        var retry: Set<String>
        var want: [String]
        switch try await c.receive() {
        case .applied(let r, let w): (retry, want) = (Set(r), w)
        case .error(let code, let message): throw SyncError.refused(code: code, message: message)
        default: throw SyncError.protocolError("expected applied")
        }
        guard !want.isEmpty else { return retry }
        var missing: [String] = []
        for id in want {
            guard let data = files(id) else { missing.append(id); continue }
            for m in SyncBlob.chunks(id: id, data) { try await c.send(m) }
        }
        try await c.send(.blobsEnd(missing: missing))
        switch try await c.receive() {
        case .applied(let r, _): retry.formUnion(r)
        case .error(let code, let message): throw SyncError.refused(code: code, message: message)
        default: throw SyncError.protocolError("expected applied")
        }
        retry.formUnion(missing)
        want = []
        return retry
    }

    /// Ask the Mac for these files and save each one that checks out.
    /// Returns (saved, failed).
    private static func fetchBlobs(_ wanted: [ScreenItem], over c: SyncConnection, engine: SyncEngine,
                                   state: inout SyncMacState) async throws -> (Int, Int) {
        try await c.send(.want(ids: wanted.map(\.id)))
        var assembler = SyncBlob.Assembler()
        var saved = Set<String>()
        loop: while true {
            switch try await c.receive() {
            case .blob(let id, let off, let data, let last):
                let item = wanted.first { $0.id == id }
                if let bytes = try? assembler.feed(id: id, off: off, data: data, last: last, expected: item), let item {
                    if (try? engine.addScreen(item, data: bytes, state: &state)) == true { saved.insert(id) }
                }
            case .blobsEnd: break loop
            case .error(let code, let message): throw SyncError.refused(code: code, message: message)
            default: throw SyncError.protocolError("expected blobs")
            }
        }
        return (saved.count, wanted.count - saved.count)
    }
}
