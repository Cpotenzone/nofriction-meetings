import Foundation

/// Merging a line edit from the Mac (docs/SYNC.md "Line edits never carry
/// removed text"). Same algorithm as `src-tauri/src/sync/merge.rs`; the
/// shared cases in `SyncFixtures/merge_cases.json` hold both to it. Pure.
enum SyncMerge {
    enum Keep: Equatable {
        case word(String)
        case marker(String)
    }

    /// nil if any entry is malformed (the edit is then ignored)
    static func parseKeep(_ keep: [String]) -> [Keep]? {
        var out: [Keep] = []
        for k in keep {
            if k.hasPrefix("w:") {
                let h = String(k.dropFirst(2)).lowercased()
                guard h.count == 16, h.allSatisfy(\.isHexDigit) else { return nil }
                out.append(.word(h))
            } else if k.hasPrefix("m:"), let id = SyncIDs.wire(String(k.dropFirst(2))) {
                out.append(.marker(id))
            } else {
                return nil
            }
        }
        return out
    }

    enum Op: Equatable {
        /// Remove the words in `range` (UTF-16); put `marker` (wire id) there if given.
        /// `strike`: AI notes get the stricken placeholder, else the words close up.
        case remove(range: NSRange, marker: String?, strike: Bool)
        /// Insert markers (wire ids) whose words were already gone here
        case insert(at: Int, markers: [String])

        var position: Int {
            switch self {
            case .remove(let r, _, _): r.location
            case .insert(let at, _): at
            }
        }
    }

    struct Plan: Equatable {
        /// From the end of the line backwards
        var ops: [Op] = []
        var removedWords = 0
        var isEmpty: Bool { ops.isEmpty }
    }

    static func plan(local: [SyncText.Tok], keep: [Keep], tokenKey: Data) -> Plan {
        let hashes: [String?] = local.map {
            if case .word(let w, _) = $0 { return SyncCrypto.wordHash(tokenKey: tokenKey, w) }
            return nil
        }
        func same(_ i: Int, _ j: Int) -> Bool {
            switch (local[i], keep[j]) {
            case (.word, .word(let h)): return hashes[i] == h
            case (.marker(let a, _), .marker(let b)): return a == b
            default: return false
            }
        }
        let n = local.count, m = keep.count
        var dp = Array(repeating: Array(repeating: 0, count: m + 1), count: n + 1)
        if n > 0 && m > 0 {
            for i in stride(from: n - 1, through: 0, by: -1) {
                for j in stride(from: m - 1, through: 0, by: -1) {
                    dp[i][j] = same(i, j) ? dp[i + 1][j + 1] + 1 : max(dp[i + 1][j], dp[i][j + 1])
                }
            }
        }
        var pairs: [(Int, Int)] = []
        var i = 0, j = 0
        while i < n && j < m {
            if same(i, j) { pairs.append((i, j)); i += 1; j += 1 }
            else if dp[i + 1][j] >= dp[i][j + 1] { i += 1 }
            else { j += 1 }
        }
        let localMarkers = Set(local.compactMap { if case .marker(let id, _) = $0 { return id }; return nil })

        var out = Plan()
        var prev: (Int, Int)?
        for (nl, nk) in pairs + [(n, m)] {
            let loL = prev.map { $0.0 + 1 } ?? 0
            let loK = prev.map { $0.1 + 1 } ?? 0
            let newMarkers: [String] = (loK..<max(loK, nk)).compactMap {
                if case .marker(let id) = keep[$0], !localMarkers.contains(id) { return id }
                return nil
            }
            var runs: [(Int, Int)] = []
            var start: Int?
            for t in loL..<max(loL, nl) {
                switch local[t] {
                case .word: if start == nil { start = t }
                case .marker:
                    if let s = start { runs.append((s, t - 1)); start = nil }
                }
            }
            if let s = start { runs.append((s, nl - 1)) }
            let strike = !newMarkers.isEmpty
            var markers = newMarkers[...]
            for (k, run) in runs.enumerated() {
                out.removedWords += run.1 - run.0 + 1
                let a = local[run.0].range, b = local[run.1].range
                let range = NSRange(location: a.location, length: b.location + b.length - a.location)
                out.ops.append(.remove(range: range, marker: k == 0 ? markers.popFirst() : nil, strike: strike))
                if k == 0, !markers.isEmpty {
                    out.ops.append(.insert(at: b.location + b.length, markers: Array(markers)))
                    markers = []
                }
            }
            if runs.isEmpty, !markers.isEmpty {
                let at = prev.map { local[$0.0].range.location + local[$0.0].range.length } ?? 0
                out.ops.append(.insert(at: at, markers: Array(markers)))
            }
            prev = (nl, nk)
        }
        out.ops.sort { a, b in
            if a.position != b.position { return a.position > b.position }
            if case .insert = a, case .remove = b { return true }
            return false
        }
        return out
    }

    /// Insert tokens at a UTF-16 offset with single spaces around them.
    static func insert(_ text: String, at: Int, tokens: [String]) -> String {
        let ns = text as NSString
        let cut = min(max(0, at), ns.length)
        let left = (ns.substring(to: cut)).replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression)
        let right = (ns.substring(from: cut)).replacingOccurrences(of: "^\\s+", with: "", options: .regularExpression)
        return [left, tokens.joined(separator: " "), right].filter { !$0.isEmpty }.joined(separator: " ")
    }

    /// Apply a plan to a line in wire form (the reference the shared cases check).
    static func apply(_ plan: Plan, toWire text: String) -> String {
        var t = text
        for op in plan.ops {
            switch op {
            case .remove(let range, let marker, _):
                t = RedactionText.splice(t, removing: range, inserting: marker.map(SyncText.wireMarker)).text
            case .insert(let at, let markers):
                t = insert(t, at: at, tokens: markers.map(SyncText.wireMarker))
            }
        }
        return t
    }
}
