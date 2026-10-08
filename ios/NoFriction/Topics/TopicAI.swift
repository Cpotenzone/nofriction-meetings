import Foundation

/// A topic the model proposed, after validation (`TopicParse`).
struct TopicCandidate: Equatable, Sendable {
    var label: String
    var key: String
    var confidence: Double

    init(label: String, confidence: Double) {
        self.label = label
        self.key = Topic.key(label)
        self.confidence = min(1, max(0, confidence))
    }
}

/// Model output → topics. The answer is untrusted: parsed as JSON, every
/// label cleaned and checked, confidence clamped, near-duplicates merged,
/// at most `Topic.maxAI`. Never crashes on bad input and never logs it.
enum TopicParse {
    static func validate(_ raw: String) throws -> [TopicCandidate] {
        let v = try StudyParse.extractJSON(raw)
        let items: [Any]
        if let a = v as? [Any] {
            items = a
        } else if let o = v as? [String: Any], let a = (o["topics"] ?? o["labels"] ?? o["items"]) as? [Any] {
            items = a
        } else {
            throw StudyParse.Failure.nothingUsable("topics")
        }
        var out: [TopicCandidate] = []
        for item in items.prefix(Topic.maxAI * 3) {
            var label: String?
            var confidence = 0.5
            if let o = item as? [String: Any] {
                label = StudyParse.clean(o["label"] ?? o["topic"] ?? o["name"] ?? o["title"], max: Topic.maxLabelLength + 20)
                confidence = Self.confidence(o["confidence"] ?? o["score"] ?? o["weight"])
            } else if item is String {
                label = StudyParse.clean(item, max: Topic.maxLabelLength + 20)
            }
            guard let label, let clean = Topic.normalizeLabel(label), !Topic.isGeneric(clean),
                  clean.split(separator: " ").count <= Topic.maxLabelWords, clean.count >= 2 else { continue }
            let c = TopicCandidate(label: clean, confidence: confidence)
            if let i = out.firstIndex(where: { Topic.similar($0.key, c.key) }) {
                out[i].confidence = max(out[i].confidence, c.confidence)
                continue
            }
            out.append(c)
            if out.count == Topic.maxAI { break }
        }
        if out.isEmpty { throw StudyParse.Failure.nothingUsable("topics") }
        return out
    }

    /// 0.8, "0.8", "80%" and 80 (of 100) → 0.8; anything else → 0.5
    static func confidence(_ v: Any?) -> Double {
        var d: Double?
        if let n = v as? NSNumber, CFGetTypeID(n) != CFBooleanGetTypeID() {
            d = n.doubleValue
        } else if let s = v as? String {
            let t = s.trimmingCharacters(in: .whitespaces)
            d = Double(t.hasSuffix("%") ? String(t.dropLast()) : t)
        }
        guard var d, d.isFinite else { return 0.5 }
        if d > 1 { d /= 100 }
        return min(1, max(0, d))
    }
}

/// Topic naming (same prompt and rules as the Mac). Requests go through
/// `AIClient` (Apple on-device or the user's endpoint); the caller has done
/// the Pro and consent checks (`MeetingDetailView.requestAI`).
extension MeetingAI {
    static let topicsSystem = """
        You name the topics of a recording from a raw speech-to-text transcript. The transcript has no \
        speaker labels and may contain recognition errors. Each line starts with its time as [m:ss]. \
        Reply with only one JSON object: no Markdown, no code fence, no text before or after it. Shape: \
        {"topics": [{"label": "short noun phrase", "confidence": 0.9}]}
        1 to 4 topics. A label is a short noun phrase of 1 to 4 words naming what was discussed \
        ("Q4 roadmap", "Mitosis", "Lease renewal"): never a sentence, never a person's name alone, never a \
        generic word like "meeting", "discussion" or "update". Use only what the transcript says; never \
        invent. Text shown as [stricken from the record] was removed by the user: never guess at it. \
        "confidence" (0 to 1) is how sure you are that the topic was a main subject of the recording.
        """

    static func topicsUserMessage(_ input: StudyInput, chunk: String, part: Int, parts: Int) -> String {
        var s = studyHeader(input)
        if parts > 1 { s += "Part \(part) of \(parts)\n" }
        return s + "\nTRANSCRIPT:\n\(chunk)"
    }

    static func topicsMaxTokens(contextTokens: Int) -> Int { min(300, max(120, contextTokens / 20)) }

    enum TopicFailure: LocalizedError, Equatable {
        case noTranscript, unusable
        var errorDescription: String? {
            switch self {
            case .noTranscript: "This recording has no transcript to find topics in."
            case .unusable: "The model's answer couldn't be used (asked twice). Try again, or pick another model in Settings."
            }
        }
    }

    /// Name the topics: the transcript in one request, or chunk by chunk
    /// for a long one (then merged). Each request is retried once when the
    /// answer can't be used; AI access errors throw.
    static func findTopics(_ input: StudyInput, contextTokens: Int, complete: Complete) async throws -> [TopicCandidate] {
        guard input.hasTranscript else { throw TopicFailure.noTranscript }
        let maxTokens = topicsMaxTokens(contextTokens: contextTokens)
        let budget = bodyBudget(contextTokens: contextTokens, maxTokens: maxTokens,
                                fixed: topicsSystem + topicsUserMessage(input, chunk: "", part: 99, parts: 99))
        let chunks = chunkLines(input.transcriptLines, budget: budget)
        var lists: [[TopicCandidate]] = []
        for (i, chunk) in chunks.enumerated() {
            let user = topicsUserMessage(input, chunk: chunk, part: i + 1, parts: chunks.count)
            for attempt in 0..<2 {
                let text = attempt == 0 ? user : user + "\n\nYour previous answer couldn't be used. Answer again with only the JSON object in the shape described, and nothing else."
                do {
                    let raw = try await complete([ChatMessage(role: "system", content: topicsSystem),
                                                  ChatMessage(role: "user", content: text)], maxTokens, attempt == 0 ? 0.1 : 0.3)
                    if let got = try? TopicParse.validate(raw) { lists.append(got); break }
                } catch {
                    if !isAnswerError(error) { throw error }
                }
            }
        }
        let merged = mergeTopicCandidates(lists, parts: chunks.count)
        guard !merged.isEmpty else { throw TopicFailure.unusable }
        return merged
    }

    /// Topics from several chunks: near-duplicates join (the commonest
    /// label leads), confidence is the mean over the chunks that named the
    /// topic, lifted for a topic that several chunks named; top `maxAI`.
    static func mergeTopicCandidates(_ lists: [[TopicCandidate]], parts: Int) -> [TopicCandidate] {
        struct Acc { var labels: [String]; var confidences: [Double] }
        var keys: [String] = []
        var acc: [String: Acc] = [:]
        for list in lists {
            for c in list {
                let k = Topic.canonicalKey(c.key, existing: keys)
                if acc[k] == nil { keys.append(k); acc[k] = Acc(labels: [], confidences: []) }
                acc[k]!.labels.append(c.label)
                acc[k]!.confidences.append(c.confidence)
            }
        }
        let out = keys.map { k -> TopicCandidate in
            let a = acc[k]!
            var counts: [String: Int] = [:]
            for l in a.labels { counts[l, default: 0] += 1 }
            let label = counts.max { ($0.value, $1.key) < ($1.value, $0.key) }!.key
            let mean = a.confidences.reduce(0, +) / Double(a.confidences.count)
            let spread = parts > 1 ? Double(a.confidences.count - 1) / Double(parts - 1) : 0
            return TopicCandidate(label: label, confidence: min(1, mean * 0.7 + 0.3 * max(spread, parts > 1 ? 0 : 1)))
        }
        return Array(out.sorted { ($0.confidence, $1.label) > ($1.confidence, $0.label) }.prefix(Topic.maxAI))
    }
}
