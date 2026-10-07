import SwiftData
import SwiftUI

/// The meeting's Links: sites said in the transcript and references the user
/// added (syllabus, a reading, slides). docs/LINKS.md.
///
/// Its own view so playback progress in the meeting screen doesn't re-run
/// detection. Nothing is fetched: no page titles, no icons. Open hands an
/// http(s) link to the browser, only when tapped.
struct MeetingLinksSection: View {
    let meeting: Meeting
    @Environment(\.modelContext) private var context
    @Environment(\.openURL) private var openURL
    @State private var editing: ReferenceDraft?
    @State private var error: String?

    var body: some View {
        let items = MeetingLinks.items(for: meeting)
        SectionBlock(title: "Links") {
            VStack(alignment: .leading, spacing: 0) {
                if items.isEmpty {
                    Text("Sites mentioned in the transcript (\u{201C}example dot com\u{201D}) show up here. Add the syllabus, a reading or the slides with Add.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .padding(14)
                        .frame(maxWidth: .infinity, alignment: .leading)
                } else {
                    ForEach(items) { item in
                        LinkRow(item: item, open: { open(item) }, edit: { edit(item) }, delete: { remove(item) })
                        if item.id != items.last?.id { Divider().overlay(Theme.hairline) }
                    }
                }
                if let error {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .font(.footnote).foregroundStyle(.orange)
                        .padding([.horizontal, .bottom], 14)
                }
            }
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        } accessory: {
            Button("Add", systemImage: "plus") { editing = ReferenceDraft(reference: nil) }
                .font(.subheadline)
                .accessibilityLabel("Add a reference")
                .accessibilityHint("Adds a web address to this recording, like the syllabus, a reading or a document")
                .accessibilityIdentifier("links-add")
        }
        .sheet(item: $editing) { draft in
            ReferenceEditor(draft: draft, meeting: meeting)
        }
    }

    private func open(_ item: MeetingLinkItem) {
        // http and https only: never javascript:, file: or any other scheme
        guard LinkDetector.isOpenable(item.url), let url = URL(string: item.url) else {
            error = "Only web links (http and https) can be opened."
            return
        }
        error = nil
        openURL(url)
    }

    private func reference(_ item: MeetingLinkItem) -> MeetingReference? {
        item.referenceID.flatMap { id in meeting.references.first { $0.id == id } }
    }

    private func edit(_ item: MeetingLinkItem) {
        guard let r = reference(item) else { return }
        editing = ReferenceDraft(reference: r)
    }

    private func remove(_ item: MeetingLinkItem) {
        guard let r = reference(item) else { return }
        do {
            try MeetingLinks.delete(r, context: context)
            error = nil
        } catch {
            self.error = "Couldn't remove the reference."
        }
    }
}

private struct LinkRow: View {
    let item: MeetingLinkItem
    let open: () -> Void
    let edit: () -> Void
    let delete: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                if let title = item.title {
                    Text(title).font(.subheadline.weight(.semibold))
                }
                Text(MeetingLinks.display(item))
                    .font(.footnote.monospaced())
                    .foregroundStyle(item.title == nil ? .primary : .secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let note = item.note {
                    Text(note).font(.footnote).foregroundStyle(.secondary)
                }
                HStack(spacing: 6) {
                    ForEach(item.sources, id: \.self) { s in
                        Text(s == .added ? "ADDED" : "SAID")
                            .font(.caption2.weight(.semibold))
                            .padding(.horizontal, 6).padding(.vertical, 1)
                            .overlay(Capsule().stroke(lineWidth: 1))
                            .foregroundStyle(s == .added ? Theme.accent : Theme.ai)
                    }
                    if item.saidCount > 0 {
                        Text("\(item.saidCount)×").font(.caption2).foregroundStyle(.secondary)
                    }
                    if let first = item.firstSaid {
                        Text("first at \(first.formatted(date: .omitted, time: .shortened))")
                            .font(.caption2).foregroundStyle(.secondary)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Button(action: open) {
                Image(systemName: "arrow.up.right.square").font(.title3)
            }
            .buttonStyle(.borderless)
            .disabled(!LinkDetector.isOpenable(item.url))
            .accessibilityLabel("Open \(MeetingLinks.display(item))")
        }
        .padding(14)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityAction(named: "Open") { open() }
        .contextMenu {
            Button("Open", systemImage: "safari", action: open)
            Button("Copy Link", systemImage: "doc.on.doc") { UIPasteboard.general.string = item.url }
            if item.referenceID != nil {
                Button("Edit…", systemImage: "pencil", action: edit)
                Button("Delete Reference", systemImage: "trash", role: .destructive, action: delete)
            }
        }
        .accessibilityIdentifier("link-row")
    }
}

/// Add or edit sheet input
struct ReferenceDraft: Identifiable {
    let id = UUID()
    let reference: MeetingReference?
}

private struct ReferenceEditor: View {
    let draft: ReferenceDraft
    let meeting: Meeting
    @Environment(\.modelContext) private var context
    @Environment(\.dismiss) private var dismiss
    @State private var url = ""
    @State private var title = ""
    @State private var note = ""
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("https://example.edu/syllabus", text: $url)
                        .keyboardType(.URL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .accessibilityLabel("Web address")
                        .accessibilityIdentifier("reference-url")
                } header: { Text("Web address") }
                Section {
                    TextField("Syllabus, Chapter 3 reading, Lecture slides…", text: $title)
                        .accessibilityLabel("Title")
                } header: { Text("Title (optional)") }
                Section {
                    TextField("Pages, due date…", text: $note, axis: .vertical)
                        .lineLimit(2...5)
                        .accessibilityLabel("Note")
                } header: { Text("Note (optional)") }
                if let error {
                    Section { Label(error, systemImage: "exclamationmark.triangle").foregroundStyle(.orange) }
                }
                if let r = draft.reference {
                    Section {
                        Button("Delete Reference", role: .destructive) {
                            try? MeetingLinks.delete(r, context: context)
                            dismiss()
                        }
                    }
                }
            }
            .navigationTitle(draft.reference == nil ? "Add Reference" : "Edit Reference")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save", action: save)
                        .disabled(url.trimmingCharacters(in: .whitespaces).isEmpty)
                        .accessibilityIdentifier("reference-save")
                }
            }
            .onAppear {
                if let r = draft.reference {
                    url = r.url
                    title = r.title ?? ""
                    note = r.note ?? ""
                }
            }
        }
        .presentationDetents([.medium, .large])
    }

    private func save() {
        do {
            if let r = draft.reference {
                try MeetingLinks.update(r, url: url, title: title, note: note, context: context)
            } else {
                try MeetingLinks.add(to: meeting, url: url, title: title, note: note, context: context)
            }
            dismiss()
        } catch {
            self.error = error.localizedDescription
        }
    }
}
