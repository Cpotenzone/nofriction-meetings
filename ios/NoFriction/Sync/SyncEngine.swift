import Foundation
import SwiftData

/// The iPhone side of sync's data (docs/SYNC.md): what to send a Mac, and
/// applying what it sends. Removals go through `RedactionEngine` (audio,
/// AI notes, review guide, AI topics, chat answers, store scrub) exactly like
/// a local Delete or Strike.
@MainActor
struct SyncEngine {
    let context: ModelContext
    let tokenKey: Data
    var redactions: RedactionCenter?
    var screenCapture: ScreenCaptureCenter?
    var now: () -> Date = { .now }

    init(context: ModelContext, tokenKey: Data, redactions: RedactionCenter? = nil, screenCapture: ScreenCaptureCenter? = nil) {
        self.context = context
        self.tokenKey = tokenKey
        self.redactions = redactions
        self.screenCapture = screenCapture
    }

    // MARK: Snapshot of the store

    private func meetings() -> [Meeting] { (try? context.fetch(FetchDescriptor<Meeting>())) ?? [] }

    /// Every line gets its cross-device id the first time sync sees it.
    func assignLineIDs() {
        var changed = false
        for m in meetings() {
            for s in m.segments where s.syncID == nil {
                s.syncID = UUID()
                changed = true
            }
        }
        if changed { try? context.save() }
    }

    static func hash(_ item: SyncItem) -> String {
        var j = item.json
        // The time stamp isn't part of what changed
        if case .object(var map) = j { map["mod"] = nil; j = .object(map) }
        return SyncCrypto.sha256Hex(Data(j.canonical.utf8))
    }

    // MARK: Items from local records

    func recordingItem(_ m: Meeting, modified: Int64) -> SyncItem {
        let cal = SyncCalendar(event: m.calendarEventID, start: m.scheduledStart.map(SyncIDs.ms), end: m.scheduledEnd.map(SyncIDs.ms),
                               location: m.location, url: m.meetingURL, notes: m.inviteNotes)
        let people = m.attendances.compactMap { a -> SyncPerson? in
            guard let p = a.person else { return nil }
            return SyncPerson(email: p.email.lowercased(), name: p.name, role: a.role == "organizer" ? "organizer" : "attendee")
        }.sorted { ($0.role == "organizer" ? 0 : 1, $0.email) < ($1.role == "organizer" ? 0 : 1, $1.email) }
        return .recording(RecordingItem(
            id: SyncIDs.wire(m.id), title: m.title, started: SyncIDs.ms(m.startedAt), ended: m.endedAt.map(SyncIDs.ms),
            kind: m.kind.rawValue, notebook: m.courseName.flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 },
            planned: m.plannedMinutes.map(Int64.init), cal: cal.isEmpty ? nil : cal, people: people, modified: modified))
    }

    func lineItem(_ s: Segment, in m: Meeting) -> SyncItem? {
        guard let id = s.syncID else { return nil }
        return .line(LineItem(id: SyncIDs.wire(id), rec: SyncIDs.wire(m.id), text: SyncText.toWire(s.text), at: SyncIDs.ms(s.start),
                              dur: s.duration > 0 ? Int64((s.duration * 1000).rounded()) : nil, speaker: nil,
                              src: s.source == Snapshot.Source.screen ? Snapshot.Source.screen : nil))
    }

    func notesItem(_ m: Meeting, modified: Int64) -> SyncItem? { Self.notesItem(m, modified: modified) }

    static func notesItem(_ m: Meeting, modified: Int64) -> SyncItem? {
        guard let md = m.aiNotes else { return nil }
        return .notes(NotesItem(rec: SyncIDs.wire(m.id), md: md, made: SyncIDs.ms(m.aiNotesAt ?? m.startedAt), stale: m.aiNotesStale, modified: modified))
    }

    /// Hash of the recording's notes as they are now (nil = none)
    static func notesHash(_ m: Meeting) -> String? { notesItem(m, modified: 0).map(hash) }

    func markItem(_ k: MomentMarker, in m: Meeting, modified: Int64) -> SyncItem {
        .mark(MarkItem(id: SyncIDs.wire(k.id), rec: SyncIDs.wire(m.id), at: SyncIDs.ms(k.at), kind: k.kind,
                       note: k.note, created: SyncIDs.ms(k.createdAt), modified: modified))
    }

    func refItem(_ r: MeetingReference, in m: Meeting, modified: Int64) -> SyncItem {
        .ref(RefItem(id: SyncIDs.wire(r.id), rec: SyncIDs.wire(m.id), url: r.url, title: r.title, note: r.note,
                     created: SyncIDs.ms(r.createdAt), modified: modified))
    }

    func topicItem(_ t: MeetingTopic, in m: Meeting) -> SyncItem {
        .topic(TopicItem(id: SyncIDs.wire(t.id), rec: SyncIDs.wire(m.id), label: t.label, key: t.key,
                         conf: Int64((min(1, max(0, t.confidence)) * 1000).rounded()), source: t.source, created: SyncIDs.ms(t.createdAt)))
    }

    func strikeItem(_ r: Redaction, in m: Meeting) -> SyncItem? {
        guard r.isStrike, r.kind == Redaction.Kind.words.rawValue || r.kind == Redaction.Kind.line.rawValue else { return nil }
        let token = RedactionText.markerToken(r.id)
        let line = m.orderedSegments.first { $0.text.contains(token) }?.syncID.map(SyncIDs.wire)
        return .strike(StrikeItem(id: SyncIDs.wire(r.id), rec: SyncIDs.wire(m.id), target: r.kind, from: r.coveredFrom.map(SyncIDs.ms),
                                  to: r.coveredTo.map(SyncIDs.ms), created: SyncIDs.ms(r.createdAt), reason: r.reason, line: line))
    }

    // MARK: Outgoing

    /// Deletions, strikes and line edits since the last sync with this Mac.
    func removals(for state: SyncMacState) -> [SyncItem] {
        let ledger = SyncLedger.load()
        var out: [SyncItem] = []
        let all = meetings()
        let present = Set(all.map { SyncIDs.wire($0.id) })
        for m in all {
            let rec = SyncIDs.wire(m.id)
            for r in m.redactions where state.hash("strike", SyncIDs.wire(r.id)) == nil {
                if let item = strikeItem(r, in: m) { out.append(item) }
            }
            for s in m.segments {
                guard let id = s.syncID.map(SyncIDs.wire) else { continue }
                let edits = ledger.lineEdits[id] ?? 0
                if state.lines.contains(id), edits != (state.sentEdits[id] ?? 0) {
                    out.append(.edit(EditItem(id: id, rec: rec, keep: SyncCrypto.keepList(tokenKey: tokenKey, wireText: SyncText.toWire(s.text)))))
                }
            }
            let marks = Set(m.markers.map { SyncIDs.wire($0.id) })
            let refs = Set(m.references.map { SyncIDs.wire($0.id) })
            let topics = Set(m.topics.map { SyncIDs.wire($0.id) })
            for (entity, ids) in [("mark", marks), ("ref", refs), ("topic", topics)] {
                for (id, _) in state.known[entity] ?? [:] where !ids.contains(id) && recOf(entity, id, state) == rec {
                    out.append(.gone(GoneItem(entity: entity, id: id, rec: rec)))
                }
            }
            if m.aiNotes == nil, state.hash("notes", rec) != nil {
                out.append(.gone(GoneItem(entity: "notes", id: rec, rec: rec)))
            }
        }
        // Lines no longer here, in recordings still here
        let lineToRec = lineRecordings(all)
        for id in state.lines where lineToRec[id] == nil {
            if let rec = state.known["lineRec"]?[id], present.contains(rec) {
                out.append(.gone(GoneItem(entity: "line", id: id, rec: rec)))
            }
        }
        // Recordings deleted here
        for (id, _) in state.known["recording"] ?? [:] where !present.contains(id) {
            out.append(.gone(GoneItem(entity: "recording", id: id, rec: nil)))
        }
        return out.sorted { $0.order < $1.order }
    }

    private func recOf(_ entity: String, _ id: String, _ state: SyncMacState) -> String? {
        state.known["\(entity)Rec"]?[id]
    }

    private func lineRecordings(_ all: [Meeting]) -> [String: String] {
        var map: [String: String] = [:]
        for m in all {
            let rec = SyncIDs.wire(m.id)
            for s in m.segments { if let id = s.syncID { map[SyncIDs.wire(id)] = rec } }
        }
        return map
    }

    /// New and changed records since the last sync with this Mac.
    func changes(for state: SyncMacState) -> [SyncItem] {
        let mod = SyncIDs.ms(now())
        var out: [SyncItem] = []
        for m in meetings().sorted(by: { $0.startedAt < $1.startedAt }) {
            let rec = SyncIDs.wire(m.id)
            let r = recordingItem(m, modified: mod)
            if state.hash("recording", rec) != Self.hash(r) { out.append(r) }
            for s in m.orderedSegments {
                guard let id = s.syncID.map(SyncIDs.wire), !state.lines.contains(id), let item = lineItem(s, in: m) else { continue }
                if RedactionText.plain(s.text).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && RedactionText.onlyMarker(s.text) == nil { continue }
                out.append(item)
            }
            if let n = notesItem(m, modified: mod), state.hash("notes", rec) != Self.hash(n) { out.append(n) }
            for k in m.markers {
                let item = markItem(k, in: m, modified: mod)
                if state.hash("mark", SyncIDs.wire(k.id)) != Self.hash(item) { out.append(item) }
            }
            for x in m.references {
                let item = refItem(x, in: m, modified: mod)
                if state.hash("ref", SyncIDs.wire(x.id)) != Self.hash(item) { out.append(item) }
            }
            for t in m.topics {
                let item = topicItem(t, in: m)
                if state.hash("topic", SyncIDs.wire(t.id)) == nil { out.append(item) }
            }
        }
        return out.sorted { $0.order < $1.order }
    }

    /// The Mac confirmed `items` (except `retry`): remember them.
    func confirmSent(_ items: [SyncItem], retry: Set<String>, state: inout SyncMacState) {
        let ledger = SyncLedger.load()
        var newlyGone: [(String, String)] = []
        for item in items where !retry.contains(item.retryID) {
            remember(item, state: &state, ledger: ledger)
            if case .gone(let g) = item { newlyGone.append((g.entity, g.id)) }
        }
        if !newlyGone.isEmpty {
            var l = SyncLedger.load()
            for (e, id) in newlyGone { l.gone[e, default: []].insert(id) }
            SyncLedger.save(l)
        }
    }

    /// Record that the Mac has this item as it is now.
    func remember(_ item: SyncItem, state: inout SyncMacState, ledger: SyncLedgerData) {
        switch item {
        case .recording(let r): state.setHash("recording", r.id, Self.hash(item))
        case .line(let l):
            state.lines.insert(l.id)
            state.setHash("lineRec", l.id, l.rec)
            state.sentEdits[l.id] = ledger.lineEdits[l.id] ?? 0
        case .edit(let e): state.sentEdits[e.id] = ledger.lineEdits[e.id] ?? 0
        case .strike(let s): state.setHash("strike", s.id, "1")
        case .notes(let n): state.setHash("notes", n.rec, Self.hash(item))
        case .mark(let k): state.setHash("mark", k.id, Self.hash(item)); state.setHash("markRec", k.id, k.rec)
        case .ref(let r): state.setHash("ref", r.id, Self.hash(item)); state.setHash("refRec", r.id, r.rec)
        case .topic(let t): state.setHash("topic", t.id, Self.hash(item)); state.setHash("topicRec", t.id, t.rec)
        case .gone(let g):
            switch g.entity {
            case "line":
                state.lines.remove(g.id); state.sentEdits[g.id] = nil; state.setHash("lineRec", g.id, nil)
            case "recording":
                state.setHash("recording", g.id, nil)
                state.setHash("notes", g.id, nil)
                for entity in ["mark", "ref", "topic"] {
                    for (id, rec) in state.known["\(entity)Rec"] ?? [:] where rec == g.id {
                        state.setHash(entity, id, nil); state.setHash("\(entity)Rec", id, nil)
                    }
                }
                for (id, rec) in state.known["lineRec"] ?? [:] where rec == g.id {
                    state.lines.remove(id); state.sentEdits[id] = nil; state.setHash("lineRec", id, nil)
                }
            case "notes": state.setHash("notes", g.id, nil)
            default: state.setHash(g.entity, g.id, nil); state.setHash("\(g.entity)Rec", g.id, nil)
            }
        }
    }

    // MARK: Incoming

    struct ApplyReport: Equatable {
        var applied = 0
        var skipped = 0
        var errors: [String] = []
    }

    private func meeting(_ wireID: String) -> Meeting? {
        guard let id = SyncIDs.uuid(wireID) else { return nil }
        return (try? context.fetch(FetchDescriptor<Meeting>(predicate: #Predicate { $0.id == id })))?.first
    }

    private func segment(_ wireID: String, in m: Meeting) -> Segment? {
        guard let id = SyncIDs.uuid(wireID) else { return nil }
        return m.segments.first { $0.syncID == id }
    }

    /// Apply a Mac's items (removals first), remembering each so it isn't sent back.
    func apply(_ items: [SyncItem], state: inout SyncMacState) async -> ApplyReport {
        var report = ApplyReport()
        var ledger = SyncLedger.load()
        for item in items.sorted(by: { $0.order < $1.order }) {
            do {
                if try await applyOne(item, state: &state, ledger: &ledger) { report.applied += 1 } else { report.skipped += 1 }
            } catch {
                report.errors.append(error.localizedDescription)
            }
        }
        SyncLedger.save(ledger)
        try? context.save()
        return report
    }

    private func isGone(_ entity: String, _ id: String, _ ledger: SyncLedgerData) -> Bool {
        ledger.gone[entity]?.contains(id) ?? false
    }

    // swiftlint:disable:next cyclomatic_complexity function_body_length
    private func applyOne(_ item: SyncItem, state: inout SyncMacState, ledger: inout SyncLedgerData) async throws -> Bool {
        switch item {
        case .strike(let s):
            guard let m = meeting(s.rec), let id = SyncIDs.uuid(s.id), m.redaction(id: id) == nil,
                  let kind = Redaction.Kind(rawValue: s.target), kind != .screen else { return false }
            let from = s.from.map(SyncIDs.date), to = s.to.map(SyncIDs.date)
            let start = max(0, (from ?? m.startedAt).timeIntervalSince(m.startedAt))
            let end = max(start, (to ?? from ?? m.startedAt).timeIntervalSince(m.startedAt))
            let r = Redaction(id: id, kind: kind, action: .strike, mediaStart: start, mediaEnd: end, coveredFrom: from, coveredTo: to,
                              reason: s.reason, createdAt: SyncIDs.date(s.created))
            context.insert(r)
            r.meeting = m
            state.setHash("strike", s.id, "1")
            return true

        case .edit(let e):
            guard let m = meeting(e.rec), let seg = segment(e.id, in: m), let keep = SyncMerge.parseKeep(e.keep) else { return false }
            let plan = SyncMerge.plan(local: SyncText.tokens(SyncText.toWire(seg.text)), keep: keep, tokenKey: tokenKey)
            guard !plan.isEmpty else { return false }
            let notesBefore = Self.notesHash(m)
            let changed = try await RedactionEngine.applySynced(plan, to: seg, meeting: m, context: context)
            keepNotesInStep(m, before: notesBefore, state: &state)
            return changed

        case .gone(let g):
            ledger.gone[g.entity, default: []].insert(g.id)
            remember(item, state: &state, ledger: ledger)
            switch g.entity {
            case "recording":
                guard let m = meeting(g.id) else { return false }
                RecordingDeletion.delete(m, context: context, redactions: redactions, screenCapture: screenCapture)
                return true
            case "line":
                guard let rec = g.rec, let m = meeting(rec), let seg = segment(g.id, in: m) else { return false }
                let notesBefore = Self.notesHash(m)
                let changed = try await RedactionEngine.applySyncedLineDelete(seg, meeting: m, context: context)
                keepNotesInStep(m, before: notesBefore, state: &state)
                return changed
            case "mark":
                guard let id = SyncIDs.uuid(g.id), let k = try? context.fetch(FetchDescriptor<MomentMarker>(predicate: #Predicate { $0.id == id })).first else { return false }
                context.delete(k); return true
            case "ref":
                guard let id = SyncIDs.uuid(g.id), let r = try? context.fetch(FetchDescriptor<MeetingReference>(predicate: #Predicate { $0.id == id })).first else { return false }
                context.delete(r); return true
            case "topic":
                guard let id = SyncIDs.uuid(g.id), let t = try? context.fetch(FetchDescriptor<MeetingTopic>(predicate: #Predicate { $0.id == id })).first else { return false }
                context.delete(t); return true
            case "notes":
                guard let m = meeting(g.id), m.aiNotes != nil else { return false }
                m.aiNotes = nil; m.aiNotesAt = nil; m.aiNotesStale = false
                return true
            default: return false
            }

        case .recording(let r):
            guard !isGone("recording", r.id, ledger), let id = SyncIDs.uuid(r.id) else { return false }
            let existing = meeting(r.id)
            if let m = existing {
                // Changed here since the last sync too: the iPhone's version wins (docs/SYNC.md)
                let mine = recordingItem(m, modified: 0)
                if let known = state.hash("recording", r.id), known != Self.hash(mine) { return false }
            }
            let m = existing ?? {
                let m = Meeting(title: r.title, startedAt: SyncIDs.date(r.started))
                m.id = id
                context.insert(m)
                return m
            }()
            m.title = r.title
            m.startedAt = SyncIDs.date(r.started)
            m.endedAt = r.ended.map(SyncIDs.date)
            m.recordingKind = RecordingKind(stored: r.kind).rawValue
            m.courseName = r.notebook
            m.plannedMinutes = r.planned.map(Int.init)
            m.calendarEventID = r.cal?.event
            m.scheduledStart = r.cal?.start.map(SyncIDs.date)
            m.scheduledEnd = r.cal?.end.map(SyncIDs.date)
            m.location = r.cal?.location
            m.meetingURL = r.cal?.url
            m.inviteNotes = r.cal?.notes
            applyPeople(r.people, to: m)
            state.setHash("recording", r.id, Self.hash(recordingItem(m, modified: 0)))
            return true

        case .line(let l):
            guard !isGone("line", l.id, ledger), let m = meeting(l.rec), let id = SyncIDs.uuid(l.id), segment(l.id, in: m) == nil else { return false }
            let text = SyncText.fromWire(l.text)
            guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return false }
            let words = Double(RedactionText.plain(text).split(whereSeparator: \.isWhitespace).count)
            let dur = l.dur.map { Double($0) / 1000 } ?? min(30, max(1, words * 0.4))
            let s = Segment(text: text, start: SyncIDs.date(l.at), duration: dur)
            s.syncID = id
            s.source = l.src == Snapshot.Source.screen ? Snapshot.Source.screen : nil
            context.insert(s)
            s.meeting = m
            state.lines.insert(l.id)
            state.setHash("lineRec", l.id, l.rec)
            state.sentEdits[l.id] = ledger.lineEdits[l.id] ?? 0
            return true

        case .notes(let n):
            guard let m = meeting(n.rec) else { return false }
            if let mine = notesItem(m, modified: 0), let known = state.hash("notes", n.rec), known != Self.hash(mine) { return false }
            m.aiNotes = n.md
            m.aiNotesAt = SyncIDs.date(n.made)
            m.aiNotesStale = n.stale
            if let mine = notesItem(m, modified: 0) { state.setHash("notes", n.rec, Self.hash(mine)) }
            return true

        case .mark(let k):
            guard !isGone("mark", k.id, ledger), let m = meeting(k.rec), let id = SyncIDs.uuid(k.id),
                  let kind = MarkerKind(rawValue: k.kind) else { return false }
            let existing = m.markers.first { $0.id == id }
            if let e = existing, let known = state.hash("mark", k.id), known != Self.hash(markItem(e, in: m, modified: 0)) { return false }
            let marker = existing ?? {
                let x = MomentMarker(id: id, at: SyncIDs.date(k.at), kind: kind, note: k.note)
                context.insert(x)
                x.meeting = m
                return x
            }()
            marker.at = SyncIDs.date(k.at)
            marker.setKind(kind)
            marker.setNote(k.note)
            marker.createdAt = SyncIDs.date(k.created)
            state.setHash("mark", k.id, Self.hash(markItem(marker, in: m, modified: 0)))
            state.setHash("markRec", k.id, k.rec)
            return true

        case .ref(let r):
            guard !isGone("ref", r.id, ledger), let m = meeting(r.rec), let id = SyncIDs.uuid(r.id) else { return false }
            // Web links only, like a link added by hand
            guard let scheme = URL(string: r.url)?.scheme?.lowercased(), ["http", "https"].contains(scheme) else { return false }
            let existing = m.references.first { $0.id == id }
            if let e = existing, let known = state.hash("ref", r.id), known != Self.hash(refItem(e, in: m, modified: 0)) { return false }
            let ref = existing ?? {
                let x = MeetingReference(url: r.url, title: r.title, note: r.note, createdAt: SyncIDs.date(r.created))
                x.id = id
                context.insert(x)
                x.meeting = m
                return x
            }()
            ref.url = r.url
            ref.title = r.title
            ref.note = r.note
            state.setHash("ref", r.id, Self.hash(refItem(ref, in: m, modified: 0)))
            state.setHash("refRec", r.id, r.rec)
            return true

        case .topic(let t):
            guard !isGone("topic", t.id, ledger), let m = meeting(t.rec), let id = SyncIDs.uuid(t.id),
                  let source = MeetingTopic.Source(rawValue: t.source) else { return false }
            guard !m.topics.contains(where: { $0.id == id || $0.key == t.key }) else { return false }
            let topic = MeetingTopic(id: id, label: t.label, confidence: Double(t.conf) / 1000, source: source, createdAt: SyncIDs.date(t.created))
            context.insert(topic)
            topic.meeting = m
            state.setHash("topic", t.id, Self.hash(topicItem(topic, in: m)))
            state.setHash("topicRec", t.id, t.rec)
            return true
        }
    }

    /// The Mac's edit rewrote these notes here the way it rewrote its own
    /// copy: if the Mac had the notes as they were, it now has them as they are.
    private func keepNotesInStep(_ m: Meeting, before: String?, state: inout SyncMacState) {
        let rec = SyncIDs.wire(m.id)
        if let known = state.hash("notes", rec), known == before { state.setHash("notes", rec, Self.notesHash(m)) }
    }

    private func applyPeople(_ people: [SyncPerson], to m: Meeting) {
        let wanted = Dictionary(people.map { ($0.email.lowercased(), $0) }, uniquingKeysWith: { a, _ in a })
        for a in m.attendances where a.person.map({ wanted[$0.email.lowercased()] == nil }) ?? true {
            context.delete(a)
        }
        for (email, p) in wanted {
            if let a = m.attendances.first(where: { $0.person?.email.lowercased() == email }) {
                a.role = p.role
                continue
            }
            let person = (try? context.fetch(FetchDescriptor<Person>(predicate: #Predicate { $0.email == email })))?.first ?? {
                let x = Person(email: email, name: p.name)
                context.insert(x)
                return x
            }()
            if person.name == nil { person.name = p.name }
            let a = Attendance(role: p.role)
            context.insert(a)
            a.meeting = m
            a.person = person
        }
    }
}
