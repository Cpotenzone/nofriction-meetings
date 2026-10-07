import Foundation

/// Link detection and normalization (pure; no I/O). docs/LINKS.md.
///
/// A port of the Mac's `src-tauri/src/meeting_links/detect.rs`. Both run the
/// shared cases in `src-tauri/src/meeting_links/detection_cases.json`
/// (`MeetingLinksTests`), so a rule changed here must change there too.
///
/// - Written: `http(s)://…`, `www.…`, and bare domains whose last label is in
///   `bareTLDs`. A bare domain needs a lowercase TLD unless the whole host is
///   uppercase, so `project.It` isn't a link. Emails, file names (`main.rs`),
///   numbers, `Mr. Smith.` and paths (`/x/site.com`) are not.
/// - Spoken (transcripts): "example dot com", "w w w dot …", "dot a i",
///   "slash word". A label can't be a common word ("the dot com bubble"), and
///   "<name> at school dot edu" is an email.
/// - Strike markers are a hard boundary: no link is built across one.
enum LinkDetector {
    struct Normalized: Equatable, Hashable {
        /// Dedupe key: host without `www.`, port (unless default), path without
        /// trailing `/`, filtered query, a `#/route` fragment. No scheme.
        let key: String
        /// What Open uses: scheme (https unless written http), `www.` if it was there, the key
        let url: String
        /// Host without `www.` (plus `:port`)
        let host: String
        /// Everything after the host in `key`
        let path: String
        let scheme: String
        let www: Bool
    }

    struct Found: Equatable {
        let link: Normalized
        /// UTF-16 offset where it starts (for ordering)
        let pos: Int
    }

    static let bareTLDs: Set<String> = [
        "com", "org", "net", "edu", "gov", "mil", "int", "io", "ai", "co", "info", "biz", "dev", "me", "tv", "fm",
        "ly", "gg", "xyz", "tech", "site", "online", "blog", "news", "wiki", "page", "academy", "school", "education",
        "university", "college", "science", "museum", "health", "ac", "us", "uk", "ca", "au", "nz", "de", "fr", "es",
        "it", "nl", "se", "no", "dk", "fi", "ie", "ch", "at", "be", "jp", "kr", "cn", "in", "br", "mx", "eu", "il",
        "sg", "hk", "za", "ru",
    ]

    static let spokenTLDs: Set<String> = [
        "com", "org", "net", "edu", "gov", "io", "ai", "co", "dev", "app", "info", "me", "tv", "us", "uk", "ca", "au",
        "de", "fr", "xyz",
    ]

    static let spokenStopwords: Set<String> = [
        "a", "an", "the", "and", "or", "but", "so", "to", "of", "in", "on", "at", "for", "with", "from", "by", "is",
        "was", "are", "were", "be", "been", "am", "it", "its", "this", "that", "these", "those", "my", "your", "our",
        "their", "his", "her", "we", "you", "they", "i", "he", "she", "me", "us", "them", "as", "if", "then", "than",
        "there", "here", "just", "like", "about", "into", "not", "no", "yes", "do", "does", "did", "go", "going", "use",
        "using", "called", "named", "polka", "dot", "slash", "period", "point",
    ]

    static let spokenPlaceWords: Set<String> = [
        "look", "looking", "available", "posted", "online", "find", "found", "located", "hosted", "live", "site",
        "website", "page", "link", "up", "out", "more", "info", "details", "everything",
    ]

    private static let trackingParams: Set<String> = [
        "fbclid", "gclid", "dclid", "gbraid", "wbraid", "msclkid", "yclid", "igshid", "mc_cid", "mc_eid", "_hsenc",
        "_hsmi", "mkt_tok",
    ]

    private static let secretParams: Set<String> = [
        "access_token", "id_token", "refresh_token", "token", "auth", "auth_token", "authorization", "api_key",
        "apikey", "key", "password", "passwd", "pwd", "secret", "client_secret", "signature", "sig",
        "x-amz-signature", "x-amz-credential", "x-amz-security-token", "session", "sessionid", "session_id", "sid",
        "jwt",
    ]

    /// Strike markers on both platforms (Mac `⟦strickenid<hex>⟧`, iOS `⟦stricken:<uuid>⟧`)
    private static let markerRegex = try! NSRegularExpression(
        pattern: "⟦?strickenid[0-9a-f]{32}⟧?|⟦stricken:[0-9A-Fa-f-]{36}⟧")
    static let markerBoundary = " ⟦⟧ "

    private static let candidateRegex = try! NSRegularExpression(pattern:
        #"(?i)(https?://[^\s<>"'`“”‘’⟦⟧]+)"#
        + #"|((?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,24}(?::[0-9]{1,5})?(?:[/?#][^\s<>"'`“”‘’⟦⟧]*)?)"#)

    private static let tokenPunct: Set<Character> = [
        ".", ",", ";", ":", "!", "?", "\"", "'", "(", ")", "[", "]", "{", "}", "“", "”", "‘", "’", "…",
    ]

    private static let maxCandidates = 2000

    // MARK: Normalize

    /// Validate and normalize a link. nil for anything that isn't an http(s)
    /// URL or a bare host: other schemes (`javascript:`, `file:`, `mailto:`),
    /// user info, whitespace, non-ASCII hosts, bad ports, hosts without a dot
    /// (except `localhost`).
    static func normalize(_ input: String) -> Normalized? {
        let s = input.trimmingCharacters(in: .whitespacesAndNewlines)
        // Same test as Rust's `char::is_whitespace` / `is_control`
        if s.isEmpty || s.unicodeScalars.contains(where: { $0.properties.isWhitespace || $0.properties.generalCategory == .control }) {
            return nil
        }
        let lower = s.lowercased()
        let scheme: String
        let rest: Substring
        if lower.hasPrefix("https://") {
            scheme = "https"; rest = s.dropFirst(8)
        } else if lower.hasPrefix("http://") {
            scheme = "http"; rest = s.dropFirst(7)
        } else {
            // Another scheme is rejected; `host:port` is not a scheme
            if let colon = s.firstIndex(of: ":") {
                let head = s[..<colon]
                let looksScheme = (head.first.map { $0.isASCII && $0.isLetter } ?? false)
                    && head.allSatisfy { $0.isASCII && ($0.isLetter || $0.isNumber || "+.-".contains($0)) }
                let after = s[s.index(after: colon)...]
                let portFollows = after.first.map { $0.isASCII && $0.isNumber } ?? false
                if looksScheme && !portFollows { return nil }
            }
            scheme = "https"; rest = s[...]
        }
        let authEnd = rest.firstIndex(where: { $0 == "/" || $0 == "?" || $0 == "#" }) ?? rest.endIndex
        let authority = rest[..<authEnd]
        let remainder = rest[authEnd...]
        if authority.isEmpty || authority.contains("@") { return nil }
        var hostRaw = authority
        var port: Int?
        if let i = authority.lastIndex(of: ":") {
            let p = authority[authority.index(after: i)...]
            guard !p.isEmpty, p.count <= 5, p.allSatisfy({ $0.isASCII && $0.isNumber }), let n = Int(p), n > 0, n <= 65535 else {
                return nil
            }
            port = n
            hostRaw = authority[..<i]
        }
        var host = hostRaw.lowercased()
        if host.hasSuffix(".") { host.removeLast() }
        guard validHost(host) else { return nil }
        let afterWWW = host.dropFirst(4)
        let www = host.hasPrefix("www.") && afterWWW.contains(".")
        let bare = www ? String(afterWWW) : host

        var pathquery = remainder
        var fragment: Substring?
        if let h = remainder.firstIndex(of: "#") {
            pathquery = remainder[..<h]
            fragment = remainder[remainder.index(after: h)...]
        }
        var path = pathquery
        var query: Substring = ""
        if let q = pathquery.firstIndex(of: "?") {
            path = pathquery[..<q]
            query = pathquery[pathquery.index(after: q)...]
        }
        while path.hasSuffix("/") { path = path.dropLast() }
        let kept = query.split(separator: "&").filter { p in
            let name = (p.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false).first.map(String.init) ?? "").lowercased()
            return !(name.hasPrefix("utm_") || trackingParams.contains(name) || secretParams.contains(name))
        }
        let route = fragment.flatMap { ($0.hasPrefix("/") || $0.hasPrefix("!/")) ? $0 : nil }

        let defaultPort = scheme == "https" ? 443 : 80
        var hostDisplay = bare
        if let port, port != defaultPort { hostDisplay += ":\(port)" }
        var tail = String(path)
        if !kept.isEmpty { tail += "?" + kept.joined(separator: "&") }
        if let route { tail += "#" + route }
        let key = hostDisplay + tail
        let url = "\(scheme)://\(www ? "www." : "")\(key)"
        return Normalized(key: key, url: url, host: hostDisplay, path: tail, scheme: scheme, www: www)
    }

    private static func validHost(_ host: String) -> Bool {
        if host.isEmpty || host.utf8.count > 253 { return false }
        if host != "localhost" && !host.contains(".") { return false }
        return host.split(separator: ".", omittingEmptySubsequences: false).allSatisfy { label in
            !label.isEmpty && label.utf8.count <= 63
                && label.allSatisfy { $0.isASCII && ($0.isLowercase || $0.isNumber || $0 == "-") }
                && !label.hasPrefix("-") && !label.hasSuffix("-")
        }
    }

    /// Only http and https links open, and only well-formed ones.
    static func isOpenable(_ url: String) -> Bool {
        let lower = url.lowercased()
        return (lower.hasPrefix("https://") || lower.hasPrefix("http://"))
            && url == url.trimmingCharacters(in: .whitespacesAndNewlines)
            && normalize(url) != nil
    }

    // MARK: Written

    /// Drop trailing sentence punctuation, keeping a closing bracket that
    /// belongs to the link (`/wiki/Foo_(bar)`).
    static func trimTrailing(_ s: Substring) -> Substring {
        var out = s
        while let last = out.last {
            let drop: Bool
            switch last {
            case ".", ",", ";", ":", "!", "?", "'", "\"", "…": drop = true
            case ")": drop = out.filter { $0 == "(" }.count < out.filter { $0 == ")" }.count
            case "]": drop = out.filter { $0 == "[" }.count < out.filter { $0 == "]" }.count
            case "}": drop = out.filter { $0 == "{" }.count < out.filter { $0 == "}" }.count
            default: drop = false
            }
            if !drop { break }
            out = out.dropLast()
        }
        return out
    }

    private static func isASCIIAlnum(_ u: UInt16) -> Bool {
        (u >= 0x30 && u <= 0x39) || (u >= 0x41 && u <= 0x5A) || (u >= 0x61 && u <= 0x7A)
    }

    private static func blocksBefore(_ u: UInt16) -> Bool {
        isASCIIAlnum(u) || "_-.@/\\$%+=~#&:".utf16.contains(u)
    }

    private static func bareHostOK(_ host: Substring) -> Bool {
        guard let tld = host.split(separator: ".", omittingEmptySubsequences: false).last else { return false }
        let lowerHost = host.lowercased()
        let www = lowerHost.hasPrefix("www.") && lowerHost.dropFirst(4).contains(".")
        if !www && !bareTLDs.contains(tld.lowercased()) { return false }
        if !tld.allSatisfy({ $0.isASCII && $0.isLetter }) { return false }
        let allUpper = !host.contains(where: { $0.isASCII && $0.isLowercase })
        return tld.allSatisfy({ $0.isLowercase }) || allUpper
    }

    private static func detectWritten(_ text: String, into out: inout [Found]) {
        let ns = text as NSString
        let matches = candidateRegex.matches(in: text, range: NSRange(location: 0, length: ns.length))
        for (n, m) in matches.enumerated() {
            if n >= maxCandidates { break }
            let start = m.range.location
            let end = m.range.location + m.range.length
            let before: UInt16? = start > 0 ? ns.character(at: start - 1) : nil
            let after: UInt16? = end < ns.length ? ns.character(at: end) : nil
            let raw = ns.substring(with: m.range)
            let lower = raw.lowercased()
            if lower.hasPrefix("http://") || lower.hasPrefix("https://") {
                if let b = before, isASCIIAlnum(b) { continue }
                if let link = normalize(String(trimTrailing(raw[...]))) { out.append(Found(link: link, pos: start)) }
                continue
            }
            if let b = before, blocksBefore(b) { continue }
            let hostEnd = raw.firstIndex(where: { $0 == "/" || $0 == "?" || $0 == "#" }) ?? raw.endIndex
            let hasPath = hostEnd < raw.endIndex
            let hostport = raw[..<hostEnd]
            let path = raw[hostEnd...]
            let host = hostport.split(separator: ":", maxSplits: 1, omittingEmptySubsequences: false).first ?? hostport
            let hasPort = host.count < hostport.count
            var candidate: String?
            if bareHostOK(host) {
                let blocked = after.map { $0 == UInt16(UInt8(ascii: "(")) || $0 == UInt16(UInt8(ascii: "@")) || isASCIIAlnum($0)
                    || $0 == UInt16(UInt8(ascii: "_")) || $0 == UInt16(UInt8(ascii: "-")) } ?? false
                if !(!hasPath && blocked) { candidate = String(hostport) + String(trimTrailing(path)) }
            } else if !hasPath && !hasPort {
                // `example.com.Next`: back off to the longest prefix that is a host
                var labels = host.split(separator: ".", omittingEmptySubsequences: false)
                while labels.count > 2 {
                    labels.removeLast()
                    let h = labels.joined(separator: ".")
                    if bareHostOK(h[...]) { candidate = h; break }
                }
            }
            if let c = candidate, let link = normalize(c) { out.append(Found(link: link, pos: start)) }
        }
    }

    // MARK: Spoken

    struct Tok {
        /// UTF-16 offset in the text
        var pos: Int
        /// Lowercased, edge punctuation removed
        var core: String
        var lead: Bool
        var trail: Bool
    }

    static func tokens(_ text: String) -> [Tok] {
        var out: [Tok] = []
        var piece = ""
        var pieceStart = 0
        var offset = 0
        func flush() {
            guard !piece.isEmpty else { return }
            let trimmedStart = piece.drop(while: { tokenPunct.contains($0) })
            var core = trimmedStart
            while let l = core.last, tokenPunct.contains(l) { core = core.dropLast() }
            out.append(Tok(pos: pieceStart, core: core.lowercased(),
                           lead: trimmedStart.count < piece.count, trail: core.count < trimmedStart.count))
            piece = ""
        }
        for scalar in text.unicodeScalars {
            // Rust's `split_whitespace`: the Unicode White_Space property
            if scalar.properties.isWhitespace {
                flush()
            } else {
                if piece.isEmpty { pieceStart = offset }
                piece.unicodeScalars.append(scalar)
            }
            offset += scalar.utf16.count
        }
        flush()
        return out
    }

    private static func isLabel(_ core: String) -> Bool {
        !core.isEmpty && core.utf8.count <= 63
            && core.allSatisfy { $0.isASCII && ($0.isLowercase || $0.isNumber || $0 == "-") }
            && !core.hasPrefix("-") && !core.hasSuffix("-")
            && !spokenStopwords.contains(core)
    }

    private static func isEmailName(_ t: Tok) -> Bool {
        !t.trail && !t.core.isEmpty
            && t.core.allSatisfy { $0.isASCII && ($0.isLowercase || $0.isNumber || $0 == "." || $0 == "_" || $0 == "-") }
            && !spokenStopwords.contains(t.core) && !spokenPlaceWords.contains(t.core)
    }

    private static func isPathWord(_ core: String) -> Bool {
        guard let first = core.first, first.isASCII, first.isLetter || first.isNumber else { return false }
        return core.allSatisfy { $0.isASCII && ($0.isLowercase || $0.isNumber || $0 == "-" || $0 == "_") }
            && !spokenStopwords.contains(core)
    }

    /// A spoken TLD at `i`: one word ("com", "a.i.") or two single letters ("a i").
    private static func spokenTLD(_ toks: [Tok], _ i: Int) -> (String, Int)? {
        guard i < toks.count else { return nil }
        let t = toks[i]
        var word = t.core
        let parts = word.split(separator: ".", omittingEmptySubsequences: false)
        if word.utf8.count >= 3 && parts.allSatisfy({ $0.count == 1 && $0.allSatisfy { $0.isASCII && $0.isLowercase } }) {
            word = word.replacingOccurrences(of: ".", with: "")
        }
        if word.utf8.count == 1 && !t.trail, i + 1 < toks.count {
            let n = toks[i + 1]
            if n.core.utf8.count == 1 && !n.lead {
                let two = word + n.core
                if spokenTLDs.contains(two) { return (two, 2) }
            }
        }
        if spokenTLDs.contains(word) { return (word, 1) }
        return nil
    }

    private static func mergeWWW(_ toks: [Tok]) -> [Tok] {
        var out: [Tok] = []
        var i = 0
        func c(_ k: Int) -> String { k < toks.count ? toks[k].core : "" }
        func clean(_ k: Int) -> Bool { k < toks.count && !toks[k].trail }
        while i < toks.count {
            let triple = (c(i) == "w" && c(i + 1) == "w" && c(i + 2) == "w") || (c(i) == "dub" && c(i + 1) == "dub" && c(i + 2) == "dub")
            let n: Int
            if triple && clean(i) && clean(i + 1) { n = 3 }
            else if c(i) == "triple" && c(i + 1) == "w" && clean(i) { n = 2 }
            else if c(i) == "w.w.w" { n = 1 }
            else { n = 0 }
            if n > 0 {
                out.append(Tok(pos: toks[i].pos, core: "www", lead: toks[i].lead, trail: toks[i + n - 1].trail))
                i += n
            } else {
                out.append(toks[i])
                i += 1
            }
        }
        return out
    }

    /// The longest "label dot … dot tld" chain starting at `i`: (host, index after it).
    private static func spokenHost(_ toks: [Tok], at i: Int) -> (String, Int)? {
        guard i < toks.count else { return nil }
        let first = toks[i]
        if !isLabel(first.core) || first.trail || first.core == "dot" { return nil }
        var labels = [first.core]
        var k = i + 1
        var best: (String, Int)?
        while labels.count <= 6 {
            guard k < toks.count else { break }
            let dot = toks[k]
            if dot.core != "dot" || dot.lead || dot.trail { break }
            if let (tld, used) = spokenTLD(toks, k + 1) {
                let last = toks[k + used]
                if !toks[k + 1].lead {
                    best = (labels.joined(separator: ".") + "." + tld, k + 1 + used)
                    if last.trail || used == 2 { break }
                }
            }
            guard k + 1 < toks.count else { break }
            let next = toks[k + 1]
            if next.lead || next.trail || !isLabel(next.core) { break }
            labels.append(next.core)
            k += 2
        }
        return best
    }

    private static func detectSpoken(_ text: String, into out: inout [Found]) {
        guard text.lowercased().contains("dot") else { return }
        let toks = mergeWWW(tokens(text))
        var i = 0
        while i < toks.count {
            guard let (host, end) = spokenHost(toks, at: i) else { i += 1; continue }
            let email = i >= 2 && toks[i - 1].core == "at" && !toks[i - 1].lead && !toks[i - 1].trail && isEmailName(toks[i - 2])
            var j = end
            var path = ""
            if !toks[j - 1].trail {
                var segs = 0
                while segs < 6, j + 1 < toks.count,
                      toks[j].core == "slash", !toks[j].lead, !toks[j].trail,
                      !toks[j + 1].lead, isPathWord(toks[j + 1].core) {
                    path += "/" + toks[j + 1].core
                    let ends = toks[j + 1].trail
                    j += 2
                    segs += 1
                    if ends { break }
                }
            }
            if !email, let link = normalize(host + path) { out.append(Found(link: link, pos: toks[i].pos)) }
            i = j
        }
    }

    // MARK: Detect

    static func stripMarkers(_ text: String) -> String {
        guard text.contains("stricken") else { return text }
        return markerRegex.stringByReplacingMatches(in: text, range: NSRange(location: 0, length: (text as NSString).length),
                                                    withTemplate: NSRegularExpression.escapedTemplate(for: markerBoundary))
    }

    /// Every link in `text`, in text order (each mention). `spoken` adds the
    /// spoken forms (transcripts).
    static func detect(_ input: String, spoken: Bool) -> [Found] {
        let text = stripMarkers(input)
        var out: [Found] = []
        detectWritten(text, into: &out)
        if spoken {
            var said: [Found] = []
            detectSpoken(text, into: &said)
            let ns = text as NSString
            for f in said {
                let overlaps = out.contains { w in
                    let ws = ns.rangeOfCharacter(from: .whitespacesAndNewlines, options: [],
                                                 range: NSRange(location: w.pos, length: ns.length - w.pos))
                    let end = ws.location == NSNotFound ? ns.length : ws.location
                    return f.pos >= w.pos && f.pos < end
                }
                if !overlaps { out.append(f) }
            }
            out.sort { $0.pos < $1.pos }
        }
        return out
    }
}
