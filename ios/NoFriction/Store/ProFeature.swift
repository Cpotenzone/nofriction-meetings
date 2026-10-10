import Foundation

/// noFriction Pro: what's Pro, and the paywall's words.
/// Source of truth: docs/PRO.md (owner decision 2026-10-10). The keys match
/// the Mac (`src/lib/pro.ts`, `entitlement::ProFeature`).
///
/// Open the paywall for a feature with `PaywallView(feature: .sync)`; its
/// title then says "Sync is part of noFriction Pro" and the feature's group
/// is highlighted. Gate with `store.isPro`.
enum ProFeature: String, CaseIterable, Identifiable, Sendable {
    case ai
    case notes
    case followUp = "follow_up"
    case reviewGuide = "review_guide"
    case chat
    case topics
    case sync
    case transcribePlaying = "transcribe_playing"
    case obsidian

    var id: String { rawValue }

    /// Paywall title when this feature opened it.
    var headline: String {
        switch self {
        case .ai: "AI features are part of noFriction Pro"
        case .notes: "Notes are part of noFriction Pro"
        case .followUp: "Follow-up email is part of noFriction Pro"
        case .reviewGuide: "Review guides are part of noFriction Pro"
        case .chat: "Chat is part of noFriction Pro"
        case .topics: "Topics are part of noFriction Pro"
        case .sync: "Sync is part of noFriction Pro"
        case .transcribePlaying: "Transcribe what's playing is part of noFriction Pro"
        case .obsidian: "Export to Obsidian is part of noFriction Pro"
        }
    }

    var group: ProGroup {
        switch self {
        case .ai, .notes, .followUp, .reviewGuide, .topics: .study
        case .chat: .chat
        case .sync: .sync
        case .transcribePlaying: .playing
        case .obsidian: .obsidian
        }
    }
}

/// What Pro adds, grouped as the paywall lists it (iPhone wording).
enum ProGroup: String, CaseIterable, Identifiable, Sendable {
    case study, chat, sync, playing, obsidian

    var id: String { rawValue }

    var title: String {
        switch self {
        case .study: "Notes and review"
        case .chat: "Chat"
        case .sync: "Sync with your Mac"
        case .playing: "Transcribe what's playing"
        case .obsidian: "Export to Obsidian"
        }
    }

    var detail: String {
        switch self {
        case .study:
            "Notes in each recording's style, follow-up emails, topics, and review guides with flashcards and a practice quiz."
        case .chat:
            "Ask all your recordings, a notebook or one recording. Answers cite the moment."
        case .sync:
            "Recordings, transcripts, notes, marks and screens move between iPhone and Mac directly on your Wi-Fi. No server; pair once with a QR code."
        case .playing:
            "While you capture the screen, noFriction also transcribes the audio of the video or call you're watching."
        case .obsidian:
            "On the Mac, each recording is saved as Markdown in your vault when it stops."
        }
    }

    var systemImage: String {
        switch self {
        case .study: "list.bullet.rectangle"
        case .chat: "bubble.left.and.text.bubble.right"
        case .sync: "arrow.triangle.2.circlepath"
        case .playing: "play.rectangle"
        case .obsidian: "square.and.arrow.down.on.square"
        }
    }
}

enum ProCopy {
    /// Default paywall title (opened from Settings).
    static let title = "noFriction Pro"
    /// The one line under the title.
    static let value = "Turn every recording into notes, a review guide and answers, and keep your iPhone and Mac in sync."
    /// What stays free, in one line.
    static let free = "Always free: recording on iPhone, iPad, Mac and Apple Watch, on-device microphone transcription, screens, marks, notebooks, Rewind, search, Links, calendar and people, Delete and Strike, and sharing."
    /// Pro is the app's features, not an AI service.
    static let aiNote = "AI runs on Apple's on-device model or the endpoint you set up. Pro doesn't include an AI service."

    static func headline(_ feature: ProFeature?) -> String { feature?.headline ?? title }
}
