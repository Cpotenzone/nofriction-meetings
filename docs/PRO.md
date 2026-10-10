# noFriction Pro: what's free and what's Pro

Owner decision, 2026-10-10: "move sync to pro. move phone audio to pro. move
a few more features to pro and make pro worth it."

This is the one list. The paywalls, the store text, the site, the terms and
the user guide copy from it. Change this file first, then the copy (see
[Where it's said](#where-its-said)).

## The list

| | Free | noFriction Pro |
|---|---|---|
| Record | iPhone, iPad, Mac and Apple Watch, with types, notebooks, timed recording and automatic stop | |
| Transcribe | On-device microphone transcription (Mac: also the call's audio) | **Transcribe what's playing** (iPhone): while you capture the screen, also transcribe the video or call you're watching |
| Screens | Mac screenshots, iPhone screen snapshots, photos | |
| Review | Marks, Rewind, search, Links, calendar and people | **Notes** (Make notes / Make again), automatic notes, **Follow-up email**, **Topics**, **Review guides** (Study guide for a class) with flashcards, practice quiz and CSV/Markdown export |
| Ask | | **Chat** with your recordings, with citations |
| Devices | One subscription covers iPhone, iPad and Mac | **Sync with your Mac**: recordings, transcripts, notes, marks and screens move between iPhone and Mac directly on your Wi-Fi. No server; pair once with a QR code |
| Your records | Edit, Delete and Strike from the record; share a recording as text; export everything as JSON | **Export to Obsidian** (Mac): each recording saved as Markdown in your vault, by hand or automatically when it stops |

Never Pro: recording, microphone transcription, search, Delete and Strike,
sharing and JSON export (data portability). The Mac's system audio (the
other people on a call) is part of recording a meeting on the Mac and stays
free; **Transcribe what's playing** is the new iPhone capability.

Price and terms don't change: `com.nofriction.meetings.pro.monthly` ($0.99 a
month) and `.pro.yearly` ($5.99 a year), each with a 1-week free trial, USA
only, Family Sharing off.

## Extra picks (at most two, 2026-10-10)

None. Every existing feature that isn't on the owner's Free list was checked;
each one is either something a free user relies on every day or too small to
sell Pro:

| Considered | Why it stays free |
|---|---|
| Automatic stop ("Stop when it's over") | A safety net that runs on every recording; without it a forgotten recording runs for hours. |
| Mac system audio (the other side of a call) | Half of every Mac meeting recording. The iPhone's "what's playing" is new; this is the baseline. |
| Choosing which screens or windows the Mac captures; adding photos | Part of screen capture, which the owner kept free. |
| Editing transcript text | The same act as Delete and Strike; it's how people fix names every day. |
| Apple Watch recording, discreet mode | Recording stays free on every device. The watch has no StoreKit path, and the policy guard keeps network APIs out of the watch. |
| A person's LinkedIn link, references in Links | Part of calendar and people, and Links, which stay free. |
| The larger Mac speech model | Would make free microphone transcription worse. |

Pro grows from new features (Sync, Transcribe what's playing) and the ones
that turn recordings into something more (notes, review, chat, export to a
notes vault), not from taking away what free users have.

## How it's enforced

| | Mac | iPhone and iPad |
|---|---|---|
| AI (notes, follow-up email, topics, review guides, chat, automatic notes) | `entitlement::require_pro()` in `ai::client::complete`, the single AI gate | `store.isPro` before every AI action (`MeetingDetailView.requestAI`, `ChatView.ask`) |
| Sync | `entitlement::require_pro_feature(ProFeature::Sync)` | `store.isPro` |
| Transcribe what's playing | (iPhone only) | `store.isPro` |
| Export to Obsidian | `require_pro_feature(ProFeature::Obsidian)` in every vault-writing command, turning on auto-export, and the auto-export on Stop | (Mac only) |

Both Mac gates enforce Pro only in the Mac App Store build (`--features mas`).
The Developer ID (DMG) build is the owner's build and never gates.

A non-AI gate fails with `PRO_REQUIRED:<key>: <Label> is part of noFriction
Pro.` The frontend reads the key (`src/lib/pro.ts → proFeatureFromError`) and
opens the paywall titled for that feature: wrap the call in
`withPro(() => invoke(...), "sync")` (`src/lib/build.ts`), or open it directly
with `requestPaywall("sync")`. The AI gate's plain `PRO_REQUIRED:` means
"ai"; `withAiConsent(fn, "chat")` passes the feature for the title.

On iOS, `PaywallView(feature: .sync)` does the same (`ProFeature.swift`).

### Feature keys

| Key | Mac | iOS | Paywall title |
|---|---|---|---|
| `ai` | AI gate default | `.ai` | AI features are part of noFriction Pro |
| `notes` | `withAiConsent(…, "notes")` | `.notes` | Notes are part of noFriction Pro |
| `follow_up` | `"follow_up"` | `.followUp` | Follow-up email is part of noFriction Pro |
| `review_guide` | `"review_guide"` | `.reviewGuide` | Review guides are part of noFriction Pro |
| `chat` | `"chat"` | `.chat` | Chat is part of noFriction Pro |
| `topics` | `"topics"` | `.topics` | Topics are part of noFriction Pro |
| `sync` | `ProFeature::Sync` / `"sync"` | `.sync` | Sync is part of noFriction Pro |
| `transcribe_playing` | | `.transcribePlaying` | Transcribe what's playing is part of noFriction Pro |
| `obsidian` | `ProFeature::Obsidian` / `"obsidian"` | `.obsidian` | Export to Obsidian is part of noFriction Pro |

Tests keep these in step: `entitlement::tests`, `commands::vault::tests`,
`src/lib/pro.test.ts`, `ios/NoFrictionTests/ProFeatureTests.swift` (which
reads `src/lib/pro.ts`).

## Where it's said

Update these when the list changes:

- Paywalls: `src/lib/pro.ts` (Mac), `ios/NoFriction/Store/ProFeature.swift`
  and `ios/NoFriction/Views/PaywallView.swift` (iOS)
- `docs/USER_GUIDE.md` §Subscription
- `docs/APP_STORE_LISTING.md`: both descriptions' "Free and Pro" paragraphs,
  What's New, subscription names and descriptions, review notes
- `site/index.html` #pricing and the FAQ, `site/terms/` (what Pro unlocks),
  `site/guides/` (the Free and Pro callout)
- `CLAUDE.md` (the subscription bullet points here)
