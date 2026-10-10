import Foundation
import SwiftData

/// What the user selected.
enum EditTarget {
    /// A run of words inside one line (token indices from `RedactionText.tokens`; words only)
    case words(Segment, ClosedRange<Int>)
    /// One or more whole lines
    case lines([Segment])
    /// One or more photos / screenshots
    case screens([Snapshot])
}

/// Applies Delete and Strike to a meeting and runs the purge checklist
/// (docs/REDACTION.md). Order for a Strike: audio first (the step most likely
/// to fail), then files, then the store — so a failure leaves nothing half
/// done and the user is told.
@MainActor
enum RedactionEngine {
    /// Padding around word timings when silencing (spec: 150 ms each side)
    static let padding = 0.15

    enum Failure: LocalizedError {
        case nothingSelected
        case markerInSelection
        var errorDescription: String? {
            switch self {
            case .nothingSelected: return "Nothing is selected."
            case .markerInSelection: return "A selection can't include something already stricken."
            }
        }
    }

    // MARK: Planning

    /// One spliced segment
    struct SegmentChange {
        let segment: Segment
        /// Token ranges (words only) to remove, any order
        let ranges: [ClosedRange<Int>]
        /// Audio covered by the removed words, seconds in the meeting's file
        let audio: ClosedRange<Double>
        /// The removed words, for redacting AI outputs (memory only, never stored)
        let phrases: [String]
    }

    struct Plan {
        var kind: Redaction.Kind
        /// Groups of changes that share one marker (a contiguous run of lines, or one word range)
        var groups: [[SegmentChange]] = []
        var snapshots: [Snapshot] = []

        var changes: [SegmentChange] { groups.flatMap { $0 } }
        /// Audio to silence in the recording's file. Lines heard from what was
        /// playing (screen capture) have none there: that audio is deleted
        /// once transcribed (docs/SCREEN_CAPTURE_IOS.md).
        var audioRanges: [ClosedRange<Double>] {
            AudioSilencer.normalized(changes.filter { !$0.segment.isFromScreen }.map(\.audio))
        }
        var phrases: [String] { changes.flatMap(\.phrases) }
        var isEmpty: Bool { changes.isEmpty && snapshots.isEmpty }
    }

    /// Where a line starts in the audio file. Lines from before word timings
    /// were stored fall back to wall-clock time since the meeting started.
    static func audioStart(of s: Segment, in m: Meeting) -> Double {
        s.audioOffset ?? max(0, s.start.timeIntervalSince(m.startedAt))
    }

    static func lineSpan(_ s: Segment, in m: Meeting) -> ClosedRange<Double> {
        let a = audioStart(of: s, in: m)
        // Legacy lines: their audio offset is a wall-clock estimate, so widen it
        let slack = s.audioOffset == nil ? 1.0 : 0
        return max(0, a - padding - slack)...(a + max(s.duration, 0) + padding + slack)
    }

    static func plan(_ target: EditTarget, in meeting: Meeting) throws -> Plan {
        switch target {
        case .words(let segment, let range):
            let toks = RedactionText.tokens(segment.text)
            guard RedactionText.isWordsOnly(toks, range) else { throw Failure.markerInSelection }
            let span = RedactionText.span(toks, range)
            let phrase = toks[range].map(\.text).joined(separator: " ")
            let words = toks[range]
            let timings = segment.wordTimings
            // Every selected word must have a timing, or the whole line is silenced
            let covered = words.allSatisfy { w in timings.contains { NSIntersectionRange($0.range, w.range).length > 0 } }
            let hits = timings.filter { NSIntersectionRange($0.range, span).length > 0 }
            let audio: ClosedRange<Double>
            if covered, let a = hits.map(\.start).min(), let b = hits.map(\.end).max() {
                audio = max(0, a - padding)...(b + padding)
            } else {
                audio = lineSpan(segment, in: meeting)
            }
            let kind: Redaction.Kind = RedactionText.wordRuns(toks).count == 1 && RedactionText.wordRuns(toks)[0] == range
                && toks.allSatisfy(\.isWord) ? .line : .words
            return Plan(kind: kind, groups: [[SegmentChange(segment: segment, ranges: [range], audio: audio, phrases: [phrase])]])

        case .lines(let segments):
            let ordered = meeting.orderedSegments
            let selected = Set(segments.map(\.persistentModelID))
            var groups: [[SegmentChange]] = []
            var current: [SegmentChange] = []
            for s in ordered {
                guard selected.contains(s.persistentModelID) else {
                    if !current.isEmpty { groups.append(current); current = [] }
                    continue
                }
                let toks = RedactionText.tokens(s.text)
                let runs = RedactionText.wordRuns(toks)
                guard !runs.isEmpty else { continue }      // already only markers
                let phrases = runs.map { toks[$0].map(\.text).joined(separator: " ") }
                var audio = lineSpan(s, in: meeting)
                let timings = s.wordTimings
                if let a = timings.map(\.start).min(), let b = timings.map(\.end).max() {
                    audio = min(audio.lowerBound, a - padding)...max(audio.upperBound, b + padding)
                }
                current.append(SegmentChange(segment: s, ranges: runs, audio: audio, phrases: phrases))
            }
            if !current.isEmpty { groups.append(current) }
            return Plan(kind: .line, groups: groups)

        case .screens(let snapshots):
            return Plan(kind: .screen, snapshots: snapshots)
        }
    }

    // MARK: Strike (no undo)

    struct StrikeResult {
        var storage: StoreHygiene.Result
    }

    /// Strike from the record. Irreversible: there is no undo buffer, and the
    /// removed text exists only in this call's locals until it returns.
    @discardableResult
    static func strike(_ target: EditTarget, reason: String?, meeting: Meeting, context: ModelContext) async throws -> StrikeResult {
        let plan = try plan(target, in: meeting)
        guard !plan.isEmpty else { throw Failure.nothingSelected }
        let reason = reason?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty

        // 1. Audio (may fail: then nothing has changed yet)
        try await silenceAudio(plan.audioRanges, meeting: meeting)

        // 2. Photo files
        let files = plan.snapshots.map(\.fileURL)
        for url in files { try removeFile(url) }

        // 3. Store: markers, text, rows
        let now = Date()
        for group in plan.groups {
            guard let first = group.first, let last = group.last else { continue }
            let r = Redaction(kind: plan.kind, action: .strike,
                              mediaStart: group.map(\.audio.lowerBound).min() ?? 0,
                              mediaEnd: group.map(\.audio.upperBound).max() ?? 0,
                              coveredFrom: first.segment.start,
                              coveredTo: last.segment.start.addingTimeInterval(max(last.segment.duration, 0)),
                              reason: reason, createdAt: now)
            context.insert(r)
            r.meeting = meeting
            let token = RedactionText.markerToken(r.id)
            for change in group {
                let s = change.segment
                let out = RedactionText.splice(s.text, timings: s.wordTimings, removingTokenRanges: change.ranges, inserting: token)
                s.text = out.text
                s.wordTimings = out.timings
            }
        }
        for snap in plan.snapshots {
            let offset = max(0, snap.takenAt.timeIntervalSince(meeting.startedAt))
            let r = Redaction(kind: .screen, action: .strike, mediaStart: offset, mediaEnd: offset,
                              coveredFrom: snap.takenAt, coveredTo: snap.takenAt, reason: reason, createdAt: now)
            context.insert(r)
            r.meeting = meeting
            context.delete(snap)
        }

        // 4. AI outputs (the study guide is deleted: it paraphrases the transcript)
        redactAIOutputs(meeting, phrases: plan.phrases, replacement: RedactionText.placeholder)
        if !plan.changes.isEmpty { purgeDerived(meeting, context: context) }

        try context.save()

        // 5. Freed space
        return StrikeResult(storage: StoreHygiene.scrub(context))
    }

    // MARK: Delete (5-second undo)

    /// Everything needed to put a Delete back during its undo window. Lives
    /// only in memory (RedactionCenter) and is dropped when the delete commits.
    struct PendingDelete {
        struct SegmentState {
            let segment: Segment?
            let text: String
            let start: Date
            let duration: TimeInterval
            let audioOffset: Double?
            let wordTimingsJSON: String?
        }

        let recordID: UUID
        let meeting: Meeting
        let label: String
        let segments: [SegmentState]
        let snapshots: [(fileName: String, takenAt: Date)]
        let audioRanges: [ClosedRange<Double>]
        let phrases: [String]
    }

    /// Apply a Delete to the transcript/photo rows now (the UI shows the
    /// result), and persist a pending record so the purge — audio, files, AI
    /// outputs — finishes even if the app dies before `commit`.
    static func delete(_ target: EditTarget, meeting: Meeting, context: ModelContext) throws -> PendingDelete {
        let plan = try plan(target, in: meeting)
        guard !plan.isEmpty else { throw Failure.nothingSelected }

        var states: [PendingDelete.SegmentState] = []
        for change in plan.changes {
            let s = change.segment
            let before = PendingDelete.SegmentState(segment: s, text: s.text, start: s.start, duration: s.duration,
                                                    audioOffset: s.audioOffset, wordTimingsJSON: s.wordTimingsJSON)
            let out = RedactionText.splice(s.text, timings: s.wordTimings, removingTokenRanges: change.ranges, inserting: nil)
            if out.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                states.append(.init(segment: nil, text: before.text, start: before.start, duration: before.duration,
                                    audioOffset: before.audioOffset, wordTimingsJSON: before.wordTimingsJSON))
                context.delete(s)
            } else {
                states.append(before)
                s.text = out.text
                s.wordTimings = out.timings
            }
        }
        let snaps = plan.snapshots.map { (fileName: $0.fileName, takenAt: $0.takenAt) }
        for snap in plan.snapshots { context.delete(snap) }

        let ranges = plan.audioRanges
        let record = Redaction(kind: plan.kind, action: .delete,
                               mediaStart: ranges.first?.lowerBound ?? 0, mediaEnd: ranges.last?.upperBound ?? 0,
                               coveredFrom: nil, coveredTo: nil)
        record.pendingAudioJSON = encode(ranges.map { [$0.lowerBound, $0.upperBound] })
        record.pendingFilesJSON = encode(snaps.map(\.fileName))
        context.insert(record)
        record.meeting = meeting
        try context.save()

        return PendingDelete(recordID: record.id, meeting: meeting, label: label(for: plan),
                             segments: states, snapshots: snaps, audioRanges: ranges, phrases: plan.phrases)
    }

    /// Put a pending Delete back exactly as it was.
    static func undo(_ p: PendingDelete, context: ModelContext) throws {
        for st in p.segments {
            if let s = st.segment, s.modelContext != nil, !s.isDeleted {
                s.text = st.text
                s.duration = st.duration
                s.audioOffset = st.audioOffset
                s.wordTimingsJSON = st.wordTimingsJSON
            } else {
                let s = Segment(text: st.text, start: st.start, duration: st.duration, audioOffset: st.audioOffset)
                s.wordTimingsJSON = st.wordTimingsJSON
                context.insert(s)
                s.meeting = p.meeting
            }
        }
        for snap in p.snapshots {
            let s = Snapshot(fileName: snap.fileName, takenAt: snap.takenAt)
            context.insert(s)
            s.meeting = p.meeting
        }
        if let record = p.meeting.redaction(id: p.recordID) { context.delete(record) }
        try context.save()
    }

    /// Finish a Delete: silence audio, delete files, redact AI outputs, drop
    /// the record (Delete leaves no trace), scrub freed space. If silencing
    /// fails the record stays, and `recover` retries at next launch.
    static func commit(_ p: PendingDelete, context: ModelContext) async throws {
        try await silenceAudio(p.audioRanges, meeting: p.meeting)
        for snap in p.snapshots { try removeFile(Storage.snapshots.appending(path: snap.fileName)) }
        redactAIOutputs(p.meeting, phrases: p.phrases, replacement: "")
        if !p.segments.isEmpty { purgeDerived(p.meeting, context: context) }
        if let record = p.meeting.redaction(id: p.recordID) { context.delete(record) }
        try context.save()
        _ = StoreHygiene.scrub(context)
    }

    /// At launch: finish any Delete whose undo window was cut short by the
    /// app being killed. The removed words aren't known any more (by design),
    /// so AI notes are only flagged for regeneration.
    static func recover(context: ModelContext) async {
        let deleteAction = Redaction.Action.delete.rawValue
        let pending = (try? context.fetch(FetchDescriptor<Redaction>(predicate: #Predicate { $0.action == deleteAction }))) ?? []
        guard !pending.isEmpty else { return }
        for record in pending {
            guard let meeting = record.meeting else { context.delete(record); continue }
            let ranges = (decode([[Double]].self, record.pendingAudioJSON) ?? [])
                .compactMap { $0.count == 2 && $0[0] <= $0[1] ? $0[0]...$0[1] : nil }
            do {
                try await silenceAudio(ranges, meeting: meeting)
                for name in decode([String].self, record.pendingFilesJSON) ?? [] {
                    try removeFile(Storage.snapshots.appending(path: name))
                }
                if meeting.aiNotes != nil { meeting.aiNotesStale = true }
                // Text was removed (it had audio to silence) or we can't tell: no study guide survives it
                if !ranges.isEmpty || (decode([String].self, record.pendingFilesJSON) ?? []).isEmpty {
                    purgeDerived(meeting, context: context)
                }
                context.delete(record)
            } catch {
                continue   // try again next launch
            }
        }
        try? context.save()
        _ = StoreHygiene.scrub(context)
    }

    // MARK: Purge steps

    static func silenceAudio(_ ranges: [ClosedRange<Double>], meeting: Meeting) async throws {
        guard !ranges.isEmpty, let name = meeting.audioFileName else { return }
        let url = Storage.audio.appending(path: name)
        guard FileManager.default.fileExists(atPath: url.path(percentEncoded: false)) else { return }
        try await Task.detached(priority: .userInitiated) {
            try AudioSilencer.silence(url, ranges: ranges)
        }.value
    }

    static func removeFile(_ url: URL) throws {
        guard FileManager.default.fileExists(atPath: url.path(percentEncoded: false)) else { return }
        try FileManager.default.removeItem(at: url)
    }

    /// Transcript text was removed: AI outputs that paraphrase or quote it go
    /// whole. The study guide (StudyStore), the AI topics (user topics stay)
    /// and every chat answer citing this recording, flagging its thread
    /// (docs/REDACTION.md step 5; docs/TOPICS_AND_CHAT.md).
    static func purgeDerived(_ meeting: Meeting, context: ModelContext) {
        StudyStore.purge(meeting, context: context)
        TopicStore.purgeAI(meeting, context: context)
        ChatStore.purge(meetingID: meeting.id, title: meeting.title, deleted: false, context: context)
    }

    /// Saved AI notes: redact the removed text and flag them as made before an edit.
    static func redactAIOutputs(_ meeting: Meeting, phrases: [String], replacement: String) {
        guard let notes = meeting.aiNotes else { return }
        meeting.aiNotes = RedactionText.redact(notes, phrases: phrases, replacement: replacement)
        meeting.aiNotesStale = true
    }

    // MARK: Helpers

    static func label(for plan: Plan) -> String {
        if !plan.snapshots.isEmpty {
            let screens = plan.snapshots.allSatisfy(\.isScreen)
            if plan.snapshots.count == 1 { return screens ? "Screen deleted" : "Photo deleted" }
            return "\(plan.snapshots.count) \(screens ? "screens" : "photos") deleted"
        }
        let lines = plan.changes.count
        if plan.kind == .words, let c = plan.changes.first, let r = c.ranges.first {
            let n = r.count
            return n == 1 ? "Word deleted" : "\(n) words deleted"
        }
        return lines == 1 ? "Line deleted" : "\(lines) lines deleted"
    }

    private static func encode<T: Encodable>(_ value: T) -> String? {
        (try? JSONEncoder().encode(value)).flatMap { String(data: $0, encoding: .utf8) }
    }

    private static func decode<T: Decodable>(_ type: T.Type, _ json: String?) -> T? {
        guard let data = json?.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(type, from: data)
    }
}

extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
