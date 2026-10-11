import SwiftData
import SwiftUI

/// Past recordings (the Recordings tab). iPhone: a list that pushes detail. iPad: list + detail.
/// Notebook chips are the one filter; People is a section under the list
/// (topics stay a search facet and live on the recording).
struct MeetingsView: View {
    @Query(sort: \Meeting.startedAt, order: .reverse) private var meetings: [Meeting]
    @Query(filter: #Predicate<Person> { !$0.isSelf }) private var people: [Person]
    @Environment(RecordingSession.self) private var session
    @Environment(RedactionCenter.self) private var redactions
    @Environment(\.modelContext) private var context
    @State private var selection: RecordingRoute?
    @State private var query = ""
    /// Lines that matched the search, by recording (`RecordingSearch`)
    @State private var lineHits: [UUID: RecordingSearch.Group] = [:]
    /// Bumped when the store is saved, so results drop a struck or deleted line
    @State private var storeVersion = 0
    /// Notebook filter; nil = All
    @State private var notebookFilter: String?

    /// Notebooks of saved recordings, most recent first
    private var notebooks: [String] { Notebook.recent(meetings.map { ($0.courseName, $0.startedAt) }, limit: 50) }

    /// The filter, unless its last recording was deleted
    private var activeNotebook: String? {
        guard let notebookFilter, notebooks.contains(where: { $0.caseInsensitiveCompare(notebookFilter) == .orderedSame }) else { return nil }
        return notebookFilter
    }

    private var shown: [Meeting] {
        let past = meetings.filter {
            $0.id != session.meeting?.id && Notebook.matches($0.courseName, filter: activeNotebook)
        }
        guard let q = RecordingSearch.normalized(query) else { return past }
        return past.filter { lineHits[$0.id] != nil || RecordingSearch.meetingMatches($0, query: q) }
    }

    /// What the search depends on: a change runs it again
    private struct SearchKey: Equatable {
        var query: String
        var storeVersion: Int
        var pendingDelete: UUID?
        var excluding: UUID?
    }

    private var searchKey: SearchKey {
        SearchKey(query: RecordingSearch.normalized(query) ?? "", storeVersion: storeVersion,
                  pendingDelete: redactions.pending?.recordID, excluding: session.meeting?.id)
    }

    /// Find the lines that match (after a short pause while typing, so every keystroke doesn't hit the store)
    private func runSearch(_ key: SearchKey) async {
        guard !key.query.isEmpty else {
            lineHits = [:]
            return
        }
        try? await Task.sleep(for: .milliseconds(150))
        guard !Task.isCancelled else { return }
        lineHits = RecordingSearch.group(RecordingSearch.lines(matching: key.query, in: context, excluding: key.excluding))
    }

    /// Sections by day, newest first (`RecordingsGrouping`)
    private var sections: [RecordingsGrouping.Section] { RecordingsGrouping.byDate(shown) }

    var body: some View {
        NavigationSplitView {
            List(selection: $selection) {
                CalendarConnectCard()
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
                if !notebooks.isEmpty {
                    NotebookFilterBar(notebooks: notebooks, selection: $notebookFilter)
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
                }
                if !people.isEmpty && query.isEmpty {
                    Section {
                        NavigationLink {
                            PeopleListView()
                        } label: {
                            Label {
                                Text("People")
                            } icon: {
                                Image(systemName: "person.2").foregroundStyle(Theme.accent)
                            }
                            .badge(people.count)
                        }
                        .accessibilityLabel("People, \(people.count)")
                        .accessibilityHint("Everyone from your calendar invites, with their recordings")
                        .accessibilityIdentifier("recordings-people")
                    }
                }
                ForEach(sections) { section in
                    Section(section.title) {
                        ForEach(section.meetings) { meeting in
                            NavigationLink(value: RecordingRoute(meeting: meeting)) { MeetingRow(meeting: meeting) }
                            // Search: the lines that matched, each opening the recording at that moment
                            if !query.isEmpty, let hits = lineHits[meeting.id] {
                                ForEach(hits.lines) { line in
                                    NavigationLink(value: RecordingRoute(meeting: meeting, line: line.id)) {
                                        SearchLineRow(line: line)
                                    }
                                    .listRowSeparator(.hidden, edges: .top)
                                    .accessibilityElement(children: .ignore)
                                    .accessibilityLabel(line.accessibilityLabel)
                                    .accessibilityHint("Opens the recording at this moment")
                                    .accessibilityAddTraits(.isButton)
                                    .accessibilityIdentifier("search-line")
                                }
                                if hits.moreLines > 0 {
                                    Text("and \(hits.moreLines) more")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .padding(.leading, SearchLineRow.indent)
                                        .listRowSeparator(.hidden, edges: .top)
                                        .accessibilityLabel("And \(hits.moreLines) more matching \(hits.moreLines == 1 ? "line" : "lines")")
                                        .accessibilityIdentifier("search-more")
                                }
                            }
                        }
                    }
                }
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(Theme.background)
            // Neutral selection on iPad; yellow fill made secondary text unreadable
            .tint(Color.white.opacity(0.14))
            .navigationTitle("Recordings")
            .searchable(text: $query, prompt: "Titles, people, topics, or anything said")
            .task(id: searchKey) { await runSearch(searchKey) }
            .onReceive(NotificationCenter.default.publisher(for: ModelContext.didSave)) { _ in
                if !query.isEmpty { storeVersion &+= 1 }
            }
            .overlay {
                if meetings.isEmpty {
                    ContentUnavailableView("No recordings yet", systemImage: "waveform",
                                           description: Text("Tap Record to make your first one. Meetings, classes and everything else you record appear here with their transcript and photos."))
                } else if shown.isEmpty && !query.isEmpty {
                    ContentUnavailableView.search(text: query)
                } else if shown.isEmpty, let activeNotebook {
                    ContentUnavailableView("No recordings in \(activeNotebook)", systemImage: "book.closed",
                                           description: Text("Tap All to see every recording."))
                }
            }
        } detail: {
            if let selection {
                // A new route (another recording, or a line) opens a fresh view
                MeetingDetailView(meeting: selection.meeting, jumpToLine: selection.line)
                    .id(selection)
            } else {
                ContentUnavailableView("Select a recording", systemImage: "rectangle.stack")
                    .background(Theme.background)
            }
        }
    }
}

/// What the Recordings list opens: a recording at its top, or at a line
/// that matched the search (scrolled to it, highlighted briefly).
struct RecordingRoute: Hashable {
    let meeting: Meeting
    var line: PersistentIdentifier? = nil
}

/// A line that matched the search, under its recording: the time from the
/// recording's start, and the words around the match.
private struct SearchLineRow: View {
    static let indent: CGFloat = 12
    let line: RecordingSearch.Line
    @Environment(\.dynamicTypeSize) private var typeSize

    var body: some View {
        // Side by side; stacked at the largest text sizes so the line keeps its width
        let layout = typeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: 4))
            : AnyLayout(HStackLayout(alignment: .firstTextBaseline, spacing: 10))
        layout {
            Text(line.offset.clock)
                .font(.caption.monospacedDigit())
                .foregroundStyle(Theme.accent)
            VStack(alignment: .leading, spacing: 2) {
                if line.isFromScreen {
                    Label(ScreenSpeaker.label, systemImage: "rectangle.dashed.badge.record")
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(.secondary)
                }
                Text(line.text)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(typeSize.isAccessibilitySize ? 6 : 3)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.leading, Self.indent)
    }
}

private struct MeetingRow: View {
    let meeting: Meeting

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(meeting.title).font(.body.weight(.medium)).lineLimit(1)
                if meeting.isFromWatch { WatchBadge() }
            }
            HStack(spacing: 8) {
                Text([meeting.startedAt.formatted(date: .omitted, time: .shortened), meeting.duration?.minutesLabel]
                    .compactMap { $0 }.joined(separator: " · "))
                if !meeting.snapshots.isEmpty {
                    Label("\(meeting.snapshots.count)", systemImage: "photo").labelStyle(.titleAndIcon)
                }
                if let phase = meeting.importPhase {
                    Text(phase == .failed ? "Not transcribed" : "Transcribing…")
                        .foregroundStyle(phase == .failed ? .orange : Theme.accent)
                        .accessibilityIdentifier("meeting-row-import-state")
                }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            // Class / Personal (a meeting is the default and isn't tagged), and the notebook
            if meeting.kind != .meeting || meeting.courseName != nil {
                HStack(spacing: 10) {
                    if meeting.kind != .meeting {
                        Label(meeting.kind.label, systemImage: meeting.kind.systemImage)
                            .foregroundStyle(.secondary)
                    }
                    if let notebook = meeting.courseName {
                        Label(notebook, systemImage: "book.closed")
                            .foregroundStyle(Theme.accent)
                            .accessibilityLabel("\(Notebook.label): \(notebook)")
                    }
                }
                .font(.caption)
                .lineLimit(1)
            }
            let names = meeting.people.prefix(3).map(\.person.displayName)
            if !names.isEmpty {
                Text(names.joined(separator: ", ") + (meeting.people.count > 3 ? " +\(meeting.people.count - 3)" : ""))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 2)
    }
}

/// Small Apple Watch mark for meetings recorded on the watch.
struct WatchBadge: View {
    var body: some View {
        Image(systemName: "applewatch")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
            .accessibilityLabel("Recorded on Apple Watch")
            .accessibilityIdentifier("watch-badge")
    }
}

/// Sections of the Recordings list (pure; unit-tested through `byDate(_:)`).
enum RecordingsGrouping {
    struct Section: Identifiable {
        var title: String
        var meetings: [Meeting]
        var id: String { title }
    }

    /// By day, newest first; the order inside a day is the order given.
    @MainActor
    static func byDate(_ meetings: [Meeting]) -> [Section] {
        let grouped = Dictionary(grouping: meetings) { Calendar.current.startOfDay(for: $0.startedAt) }
        return grouped.keys.sorted(by: >).map {
            Section(title: $0.formatted(.dateTime.weekday(.wide).month(.wide).day()), meetings: grouped[$0]!)
        }
    }
}
