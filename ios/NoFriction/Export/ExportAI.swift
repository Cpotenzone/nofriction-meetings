import Foundation

/// The Export sheet's AI tools (docs/EXPORT.md), same prompts and checks as
/// the Mac (src-tauri/src/export_package/ai.rs):
/// - **Highlights**: the key moments, each with words copied from the
///   transcript. A quote the model didn't copy exactly is dropped.
/// - **Captions**: one sentence per picked photo or screen, from what was
///   being said around it (text only, so Apple's on-device model can do it).
/// - **Pick**: the same, choosing the few pictures worth keeping.
/// Prompts carry `RedactionText` text, so stricken words reach the model as
/// the marker; a quote or caption with the marker is dropped.
enum ExportAI {
    static let highlightBudget = 16_000
    static let maxParts = 120
    static let partTarget = 320
    static let maxHighlights = 8
    static let maxPictures = 60
    static let maxPicks = 12
    /// Both removal markers ("[stricken from the record]", "[screen stricken
    /// from the record]") contain this: never quoted, never a caption.
    static let stricken = "stricken from the record"

    /// A line that is only a removal marker (a struck line or screen).
    static func onlyMarker(_ text: String) -> Bool {
        let t = text.trimmingCharacters(in: .whitespaces)
        return t == "[\(stricken)]" || t == "[screen \(stricken)]"
    }

    /// Quote marks the model may wrap a quote in (the package adds its own).
    static func unquote(_ q: String) -> String {
        q.trimmingCharacters(in: CharacterSet(charactersIn: "\"“”‘’'«»")).trimmingCharacters(in: .whitespaces)
    }

    static let highlightsSystem = """
        You choose the key moments of a recording for someone who wasn't there. \
        You get the transcript as numbered parts, each with its time. \
        Answer with JSON only, in this shape: \
        {"highlights":[{"part":3,"label":"What happens, at most 12 words","quote":"Up to 25 words copied exactly from that part"}]}. \
        Pick 3 to 8 parts, in time order: decisions, key facts, commitments, open questions and turning points. \
        Use only the transcript. Never invent words, names or numbers. \
        Never quote text shown as [stricken from the record].
        """

    static let captionsSystem = """
        You write captions for pictures from a recording: screens that were shown and photos that were taken. \
        For each numbered picture you get its time, the app and window if known, and what was being said around then. \
        Answer with JSON only, in this shape: {"captions":[{"picture":1,"caption":"One sentence, at most 20 words"}]}. \
        Say what the picture is about from what was said. Don't describe details you weren't told. \
        Use only the given text. Never invent names or numbers.
        """

    static func picksSystem(_ k: Int) -> String {
        """
        You choose the pictures worth keeping from a recording: screens that were shown and photos that were taken. \
        For each numbered picture you get its time, the app and window if known, and what was being said around then. \
        Choose at most \(k) pictures that best show what was discussed. Skip near-duplicates and pictures nothing was said about. \
        Caption each one you choose. \
        Answer with JSON only, in this shape: {"picks":[{"picture":1,"caption":"One sentence, at most 20 words"}]}, in time order. \
        Use only the given text. Never invent names or numbers.
        """
    }

    enum Failure: LocalizedError {
        case noTranscript, noHighlights, noCaptions, noPictures
        var errorDescription: String? {
            switch self {
            case .noTranscript: "This recording has no transcript, so there's nothing to pick highlights from."
            case .noHighlights: "The AI's answer had no usable highlights. Try again."
            case .noCaptions: "The AI's answer had no usable captions. Try again."
            case .noPictures: "There are no photos or screens to caption."
            }
        }
    }

    // MARK: Highlights

    struct Part: Equatable {
        var time: TimeInterval
        var text: String
    }

    /// Transcript lines grouped into at most `maxParts` parts of about
    /// `partTarget` characters, in time order.
    static func parts(_ lines: [ExportPackage.Line]) -> [Part] {
        let lines = lines.filter { !$0.text.trimmingCharacters(in: .whitespaces).isEmpty && !onlyMarker($0.text) }
        guard !lines.isEmpty else { return [] }
        let total = lines.reduce(0) { $0 + $1.text.utf8.count + 1 }
        let target = max(partTarget, total / maxParts + 1)
        var out: [Part] = []
        var cur: Part?
        for l in lines {
            let said = l.text.trimmingCharacters(in: .whitespaces)
            if var p = cur, p.text.utf8.count < target {
                p.text += " " + said
                cur = p
            } else {
                if let p = cur { out.append(p) }
                cur = Part(time: l.time, text: said)
            }
        }
        if let p = cur { out.append(p) }
        return out
    }

    static func highlightsPrompt(title: String, parts: [Part]) -> String {
        let per = max(80, highlightBudget / max(parts.count, 1))
        var out = "Recording: \(title)\n\nTRANSCRIPT PARTS:\n"
        for (i, p) in parts.enumerated() { out += "[\(i + 1)] (\(p.time.clock)) \(String(p.text.prefix(per)))\n" }
        return out + "\nJSON:"
    }

    /// Letters and digits only, lowercased, single-spaced.
    static func normalize(_ s: String) -> String {
        String(s.lowercased().map { $0.isLetter || $0.isNumber ? $0 : " " }).split(separator: " ").joined(separator: " ")
    }

    private static func number(_ v: Any?) -> Int? {
        if let n = v as? Int { return n }
        if let n = v as? Double { return Int(n) }
        if let s = v as? String { return Int(s.trimmingCharacters(in: .whitespaces)) }
        return nil
    }

    static func parseHighlights(_ raw: String, parts: [Part]) throws -> [ExportPackage.Highlight] {
        let v = try StudyParse.extractJSON(raw)
        let items = ((v as? [String: Any])?["highlights"] as? [Any]) ?? (v as? [Any]) ?? []
        var seen = Set<Int>()
        var out: [(Int, ExportPackage.Highlight)] = []
        for case let it as [String: Any] in items {
            guard let n = number(it["part"]), n >= 1, n <= parts.count, seen.insert(n).inserted,
                  let label = StudyParse.clean(it["label"], max: 120) else { continue }
            let part = parts[n - 1]
            let quote = StudyParse.clean(it["quote"], max: 220).map(unquote).flatMap { q -> String? in
                let nq = normalize(q)
                return !q.contains(stricken) && nq.split(separator: " ").count >= 2 && normalize(part.text).contains(nq) ? q : nil
            }
            out.append((n, .init(time: part.time, label: label, quote: quote)))
        }
        guard !out.isEmpty else { throw Failure.noHighlights }
        return out.sorted { $0.0 < $1.0 }.prefix(maxHighlights).map(\.1)
    }

    // MARK: Captions

    struct Picture: Equatable {
        var time: TimeInterval
        var end: TimeInterval?
        var isPhoto: Bool
    }

    static func saidAround(_ p: Picture, lines: [ExportPackage.Line], max: Int) -> String {
        let from = p.time - 30
        let to = Swift.max(p.end ?? p.time, p.time + 60)
        let text = lines.filter { $0.time >= from && $0.time <= to }
            .map { $0.text.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }.joined(separator: " ")
        return text.isEmpty ? "(nothing was said)" : String(text.prefix(max))
    }

    static func sample(_ n: Int, _ k: Int) -> [Int] {
        if n <= k { return Array(0..<n) }
        if k <= 1 { return [0] }
        var out: [Int] = []
        for i in 0..<k {
            let v = i * (n - 1) / (k - 1)
            if out.last != v { out.append(v) }
        }
        return out
    }

    static func picturesPrompt(title: String, pictures: [Picture], lines: [ExportPackage.Line], perPicture: Int) -> String {
        var out = "Recording: \(title)\n\nPICTURES:\n"
        for (i, p) in pictures.enumerated() {
            out += "[\(i + 1)] \(p.isPhoto ? "photo" : "screen") at \(p.time.clock)\n"
            out += "Said around then: \(saidAround(p, lines: lines, max: perPicture))\n\n"
        }
        return out + "JSON:"
    }

    /// (picture index from 0, caption), valid numbers only, once each, in order.
    static func parseCaptions(_ raw: String, count: Int, key: String) throws -> [(Int, String)] {
        let v = try StudyParse.extractJSON(raw)
        let o = v as? [String: Any]
        let items = (o?[key] as? [Any]) ?? (o?["captions"] as? [Any]) ?? (v as? [Any]) ?? []
        var seen = Set<Int>()
        var out: [(Int, String)] = []
        for case let it as [String: Any] in items {
            guard let n = number(it["picture"]), n >= 1, n <= count, seen.insert(n).inserted,
                  let c = StudyParse.clean(it["caption"], max: 160), !c.contains(stricken) else { continue }
            out.append((n - 1, c))
        }
        guard !out.isEmpty else { throw Failure.noCaptions }
        return out.sorted { $0.0 < $1.0 }
    }

    // MARK: Calls

    static func highlights(title: String, lines: [ExportPackage.Line], complete: MeetingAI.Complete) async throws -> [ExportPackage.Highlight] {
        let ps = parts(lines)
        guard !ps.isEmpty else { throw Failure.noTranscript }
        let raw = try await complete([.init(role: "system", content: highlightsSystem),
                                      .init(role: "user", content: highlightsPrompt(title: title, parts: ps))], 900, 0.2)
        return try parseHighlights(raw, parts: ps)
    }

    /// Captions for `pictures` (indexes into it). With `pick`, the model
    /// chooses at most that many (from an even sample if there are many).
    static func captions(title: String, pictures: [Picture], lines: [ExportPackage.Line], pick: Int?,
                         complete: MeetingAI.Complete) async throws -> [(Int, String)] {
        guard !pictures.isEmpty else { throw Failure.noPictures }
        let k = pick.map { min(max($0, 1), maxPicks) }
        let chosen: [Int] = pictures.count > maxPictures
            ? (k != nil ? sample(pictures.count, maxPictures) : Array(0..<maxPictures))
            : Array(pictures.indices)
        let sent = chosen.map { pictures[$0] }
        let prompt = picturesPrompt(title: title, pictures: sent, lines: lines, perPicture: sent.count > 20 ? 220 : 420)
        let system = k.map(picksSystem) ?? captionsSystem
        let raw = try await complete([.init(role: "system", content: system), .init(role: "user", content: prompt)], 1200, 0.2)
        var parsed = try parseCaptions(raw, count: sent.count, key: k == nil ? "captions" : "picks")
        if let k { parsed = Array(parsed.prefix(k)) }
        return parsed.map { (chosen[$0.0], $0.1) }
    }
}
