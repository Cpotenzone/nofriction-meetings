import Foundation

/// Meeting prompts. Every instruction is spelled out (small local models have
/// no persona), and the model is told never to invent owners or dates.
enum MeetingAI {
    static let notesSystem = """
        You write meeting notes from a raw speech-to-text transcript. The transcript has no \
        speaker labels and may contain recognition errors; don't quote errors back.
        Answer in Markdown with exactly these sections:
        ## Summary
        (one short paragraph)
        ## Decisions
        (bullets; "None recorded." if there were none)
        ## Action items
        (bullets as "- [owner] task (due date)". Only name an owner or date that is \
        stated in the transcript; otherwise write "- [owner not stated] task" and leave the date out.)
        Never invent names, numbers, owners or dates.
        """

    /// A recording with a class gets lecture notes, not meeting minutes
    /// (same rules as the Mac's classes.rs prompt). A prompt variant only.
    static let lectureNotesSystem = """
        You write lecture notes for a student from a raw speech-to-text transcript of a class. \
        The transcript has no speaker labels and may contain recognition errors; don't quote errors back.
        Answer in Markdown with exactly these sections:
        ## Summary
        (one short paragraph: what the lecture covered)
        ## Key concepts
        (bullets, in the order they came up)
        ## Definitions
        (bullets as "- **term**: definition as the instructor explained it"; "None." if there were none)
        ## Examples
        (bullets: examples the instructor worked through; "None." if there were none)
        ## Announcements and deadlines
        (bullets: assignments, readings, exams and dates the instructor stated; "None mentioned." if none)
        Never write action items for attendees and never invent tasks for students.
        Never invent names, numbers, definitions or dates.
        """

    /// The notes prompt for a meeting: lecture notes when it has a class.
    static func notesSystem(isLecture: Bool) -> String { isLecture ? lectureNotesSystem : notesSystem }

    static let emailSystem = """
        Draft a short, friendly follow-up email to the people in this meeting: thank them, \
        recap what was decided, and list next steps. Plain text, no Markdown. Start with a \
        "Subject:" line. Only state facts that are in the transcript; never invent owners or dates.
        """

    static func notes(context: String, endpoint: AIEndpoint, isLecture: Bool = false) async throws -> String {
        try await AIClient.shared.complete([
            .init(role: "system", content: notesSystem(isLecture: isLecture)),
            .init(role: "user", content: context),
        ], maxTokens: 1500, temperature: 0.2, endpoint: endpoint)
    }

    static func followUpEmail(context: String, endpoint: AIEndpoint) async throws -> String {
        try await AIClient.shared.complete([
            .init(role: "system", content: emailSystem),
            .init(role: "user", content: context),
        ], maxTokens: 700, temperature: 0.5, endpoint: endpoint)
    }

    /// Prompt text for a meeting. Build it on the main actor (SwiftData
    /// models aren't Sendable) and pass the string in.
    @MainActor static func context(_ m: Meeting) -> String {
        var s = "Meeting: \(m.title)\nWhen: \(m.startedAt.formatted(date: .complete, time: .shortened))\n"
        if let className = m.courseName { s += "Class: \(className)\n" }
        if !m.people.isEmpty {
            s += "Attendees: " + m.people.map { p in
                p.person.displayName + (p.person.company.map { " (\($0))" } ?? "")
            }.joined(separator: ", ") + "\n"
        }
        if let notes = m.inviteNotes, !notes.isEmpty { s += "Invite notes: \(notes)\n" }
        s += "\nTranscript:\n\(m.transcriptText)"
        return s
    }
}
