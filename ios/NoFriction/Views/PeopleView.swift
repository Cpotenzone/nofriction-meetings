import SwiftData
import SwiftUI

/// Everyone from your calendar invites, most recent first. Reached from the
/// People row under Recordings (not a tab of its own).
struct PeopleListView: View {
    @Query(filter: #Predicate<Person> { !$0.isSelf }) private var people: [Person]
    @State private var query = ""

    private var shown: [Person] {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        return people
            .filter { q.isEmpty || $0.displayName.lowercased().contains(q) || $0.email.contains(q) || ($0.company?.lowercased().contains(q) ?? false) }
            .sorted { ($0.meetings.first?.startedAt ?? .distantPast) > ($1.meetings.first?.startedAt ?? .distantPast) }
    }

    var body: some View {
        List {
            ForEach(shown) { person in
                NavigationLink {
                    PersonDetailView(person: person)
                } label: {
                    PersonRow(person: person, role: nil, showsMeetingCount: true)
                }
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background(Theme.background)
        .navigationTitle("People")
        .searchable(text: $query, prompt: "Name, company or email")
        .overlay {
            if people.isEmpty {
                ContentUnavailableView("No people yet", systemImage: "person.2",
                                       description: Text("Connect your calendar and everyone on your invites appears here."))
            } else if shown.isEmpty {
                ContentUnavailableView.search(text: query)
            }
        }
    }
}

/// One person: their recordings, and the LinkedIn link (the one place to add or change it).
struct PersonDetailView: View {
    @Bindable var person: Person
    @Environment(\.openURL) private var openURL
    @Environment(\.modelContext) private var context
    @State private var editing = false

    var body: some View {
        List {
            Section {
                PersonRow(person: person, role: nil)
            }
            Section("Links") {
                if let link = person.linkedinURL, let url = URL(string: link) {
                    Button { openURL(url) } label: {
                        Label("Open LinkedIn", systemImage: "link")
                    }
                    .accessibilityHint("Opens the profile in your browser")
                    Button("Change LinkedIn link", systemImage: "pencil") { editing = true }
                    Button("Remove LinkedIn link", systemImage: "trash", role: .destructive) {
                        person.linkedinURL = nil
                        try? context.save()
                    }
                } else {
                    Button("Add LinkedIn link", systemImage: "plus") { editing = true }
                        .accessibilityIdentifier("person-add-linkedin")
                }
            }
            Section("Recordings") {
                ForEach(person.meetings) { meeting in
                    NavigationLink {
                        MeetingDetailView(meeting: meeting)
                    } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(meeting.title)
                            Text(meeting.startedAt.formatted(date: .abbreviated, time: .shortened))
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.background)
        .navigationTitle(person.displayName)
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $editing) {
            LinkedInSheet(person: person)
                .presentationDetents([.medium])
        }
    }
}

/// Avatar, name, company; a LinkedIn badge when a link is saved (one tap to open).
struct PersonRow: View {
    @Bindable var person: Person
    let role: String?
    var showsMeetingCount = false
    @Environment(\.openURL) private var openURL

    var body: some View {
        HStack(spacing: 12) {
            Text(person.initials)
                .font(.caption.weight(.semibold))
                .frame(width: 36, height: 36)
                .background(Theme.card, in: Circle())
                .overlay(Circle().stroke(Theme.hairline))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(person.displayName).font(.body.weight(.medium)).lineLimit(1)
                    if role == "organizer" {
                        Text("Organizer").font(.caption2.weight(.semibold)).foregroundStyle(Theme.accent)
                    }
                }
                Text(subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 8)
            if let link = person.linkedinURL, let url = URL(string: link) {
                Button { openURL(url) } label: {
                    Text("in")
                        .font(.system(size: 13, weight: .bold))
                        .foregroundStyle(.white)
                        .frame(width: 30, height: 30)
                        .background(Theme.linkedIn, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
                }
                .buttonStyle(.borderless)
                .accessibilityLabel("Open \(person.displayName)'s LinkedIn")
            }
        }
        .padding(.vertical, 6)
    }

    private var subtitle: String {
        var parts = [person.company, person.email].compactMap { $0 }
        if showsMeetingCount {
            let n = person.attendances.count
            parts.append("\(n) recording\(n == 1 ? "" : "s")")
        }
        return parts.joined(separator: " · ")
    }
}

/// Find on LinkedIn → copy the profile link → paste here.
private struct LinkedInSheet: View {
    @Bindable var person: Person
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    @Environment(\.modelContext) private var context
    @State private var text = ""
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Button {
                        openURL(LinkedIn.searchURL(name: person.name, email: person.email, company: person.company))
                    } label: {
                        Label("Search LinkedIn for \(person.displayName)", systemImage: "magnifyingglass")
                    }
                } footer: {
                    Text("Find their profile, tap Share → Copy Link, then come back and paste it below.")
                }
                Section {
                    TextField("linkedin.com/in/…", text: $text)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                        .onSubmit(save)
                    PasteButton(payloadType: String.self) { strings in
                        if let s = strings.first { text = s; save() }
                    }
                } footer: {
                    if let error { Text(error).foregroundStyle(Theme.accent) }
                }
            }
            .navigationTitle("LinkedIn")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("Save", action: save).disabled(text.isEmpty) }
            }
            .onAppear { text = person.linkedinURL ?? "" }
        }
    }

    private func save() {
        do {
            person.linkedinURL = try LinkedIn.normalize(text)
            try? context.save()
            dismiss()
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// "Connect your calendar": once, on Recordings only, until access is granted
/// or the card is dismissed. The result line stays until the view goes away.
struct CalendarConnectCard: View {
    static let dismissedKey = "calendarCardDismissed"
    @Environment(\.modelContext) private var context
    @Environment(\.openURL) private var openURL
    @AppStorage(dismissedKey) private var dismissed = false
    @State private var authorized = CalendarService.shared.isAuthorized
    @State private var result: String?

    var body: some View {
        if (!authorized && !dismissed) || result != nil {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "calendar")
                    .font(.title3)
                    .foregroundStyle(Theme.accent)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 4) {
                    Text(authorized ? "Calendar connected" : "Connect your calendar").font(.subheadline.weight(.semibold))
                    Text(result ?? "Recordings take their event's name and who attended. Read only.")
                        .font(.caption).foregroundStyle(.secondary)
                    if !authorized {
                        Button(CalendarService.shared.isDenied ? "Open Settings" : "Connect") { Task { await connect() } }
                            .buttonStyle(.borderedProminent)
                            .controlSize(.small)
                            .foregroundStyle(.black)
                            .padding(.top, 4)
                    }
                }
                Spacer(minLength: 0)
                if !authorized {
                    Button { dismissed = true } label: {
                        Image(systemName: "xmark").font(.footnote.weight(.semibold))
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("Dismiss")
                    .accessibilityHint("Hides this card. Calendar access can be allowed later in the Settings app.")
                    .accessibilityIdentifier("calendar-card-dismiss")
                }
            }
            .padding(14)
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .tint(Theme.accent)   // lists may use a neutral selection tint
            .accessibilityIdentifier("calendar-card")
        }
    }

    private func connect() async {
        if CalendarService.shared.isDenied {
            if let url = URL(string: UIApplication.openSettingsURLString) { openURL(url) }
            return
        }
        authorized = await CalendarService.shared.requestAccess()
        if authorized {
            let n = MeetingLinker.backfill(in: context)
            result = n > 0 ? "Linked \(n) recording\(n == 1 ? "" : "s") to your calendar." : "New recordings will pick up their calendar event."
        }
    }
}
