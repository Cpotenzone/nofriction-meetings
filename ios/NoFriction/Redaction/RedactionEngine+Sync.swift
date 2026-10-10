import Foundation
import SwiftData

/// Sync with your Mac (docs/SYNC.md): what the Mac removed from a line runs
/// through the same purge as a Delete or Strike made here: audio silenced
/// (for a recording made on this iPhone), the line edited or removed, AI
/// notes rewritten (distinctive words only) and flagged, the review guide,
/// AI topics and chat answers that cited it deleted, freed space scrubbed.
/// The removed words exist only in these locals.
extension RedactionEngine {
    /// Apply a merge plan (from `SyncMerge.plan`, offsets into the wire form
    /// of `segment.text`). Returns whether anything changed.
    @discardableResult
    static func applySynced(_ plan: SyncMerge.Plan, to segment: Segment, meeting: Meeting, context: ModelContext) async throws -> Bool {
        let original = segment.text
        // Work in wire form (the plan's offsets), then convert back
        var text = SyncText.toWire(original)
        var timings = segment.wordTimings
        var struck: [String] = []
        var deleted: [String] = []
        var audio: [ClosedRange<Double>] = []
        for op in plan.ops {
            switch op {
            case .remove(let range, let marker, let strike):
                // Wire and local markers have the same length (a 36-character
                // UUID), so offsets are the same in both forms
                let words = RedactionText.tokens(SyncText.fromWire(text)).filter { NSIntersectionRange($0.range, range).length > 0 }
                guard !words.isEmpty, words.allSatisfy(\.isWord) else { continue }
                let phrase = words.map(\.text).joined(separator: " ")
                if strike { struck.append(phrase) } else { deleted.append(phrase) }
                audio.append(wordAudio(words.map(\.range), timings: timings, segment: segment, meeting: meeting))
                let out = RedactionText.splice(text, timings: timings, removing: range,
                                               inserting: marker.map(SyncText.wireMarker))
                text = out.text
                timings = out.timings
            case .insert(let at, let markers):
                let before = (text as NSString).length
                let tokens = markers.filter { !text.contains(SyncText.wireMarker($0)) }.map(SyncText.wireMarker)
                guard !tokens.isEmpty else { continue }
                text = SyncMerge.insert(text, at: at, tokens: tokens)
                let delta = (text as NSString).length - before
                timings = timings.map { t in
                    var t = t
                    if t.location >= at { t.location += delta }
                    return t
                }
            }
        }
        let local = SyncText.fromWire(text)
        guard local != original else { return false }

        // 1. Audio (may fail: then nothing has changed yet)
        try await silenceAudio(AudioSilencer.normalized(audio), meeting: meeting)
        // 2. The line
        if RedactionText.plain(local).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && RedactionText.onlyMarker(local) == nil {
            context.delete(segment)
        } else {
            segment.text = local
            segment.wordTimings = timings
        }
        // 3. AI outputs
        if !struck.isEmpty { redactAIOutputs(meeting, phrases: struck, replacement: RedactionText.placeholder) }
        if !deleted.isEmpty { redactAIOutputs(meeting, phrases: deleted, replacement: "") }
        if !struck.isEmpty || !deleted.isEmpty { purgeDerived(meeting, context: context) }
        try context.save()
        // 4. Freed space
        _ = StoreHygiene.scrub(context)
        return true
    }

    /// The Mac deleted or struck this photo or screen: the file and the row
    /// go, and freed space is scrubbed, as for a Delete made here (a strike's
    /// marker arrives as its own record). Screen-only edits keep notes and guides.
    @discardableResult
    static func applySyncedScreenDelete(_ snapshot: Snapshot, meeting: Meeting, context: ModelContext) throws -> Bool {
        try removeFile(snapshot.fileURL)
        context.delete(snapshot)
        try context.save()
        _ = StoreHygiene.scrub(context)
        return true
    }

    /// The Mac deleted the whole line: every word goes (strike markers in it stay).
    @discardableResult
    static func applySyncedLineDelete(_ segment: Segment, meeting: Meeting, context: ModelContext) async throws -> Bool {
        let toks = SyncText.tokens(SyncText.toWire(segment.text))
        var ops: [SyncMerge.Op] = []
        var run: NSRange?
        for t in toks {
            switch t {
            case .word(_, let r):
                run = run.map { NSRange(location: $0.location, length: r.location + r.length - $0.location) } ?? r
            case .marker:
                if let r = run { ops.append(.remove(range: r, marker: nil, strike: false)); run = nil }
            }
        }
        if let r = run { ops.append(.remove(range: r, marker: nil, strike: false)) }
        guard !ops.isEmpty else { return false }
        return try await applySynced(SyncMerge.Plan(ops: ops.reversed(), removedWords: 0), to: segment, meeting: meeting, context: context)
    }

    /// Audio covered by these words: their timings (with padding) when every
    /// word has one, else the whole line, as for a local edit.
    private static func wordAudio(_ ranges: [NSRange], timings: [WordTiming], segment: Segment, meeting: Meeting) -> ClosedRange<Double> {
        let covered = ranges.allSatisfy { r in timings.contains { NSIntersectionRange($0.range, r).length > 0 } }
        let hits = timings.filter { t in ranges.contains { NSIntersectionRange(t.range, $0).length > 0 } }
        if covered, let a = hits.map(\.start).min(), let b = hits.map(\.end).max() {
            return max(0, a - padding)...(b + padding)
        }
        return lineSpan(segment, in: meeting)
    }
}
