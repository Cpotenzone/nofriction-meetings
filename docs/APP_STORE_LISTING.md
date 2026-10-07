# App Store listing: noFriction (iOS + macOS)

Ready-to-paste metadata for App Store Connect. Every limited field shows its
exact character count (generated, not estimated). Product facts were checked
against the code on 2026-10-02; where the Mac and iOS apps differ, the copy
says so. Owner steps and order: [LAUNCH_CHECKLIST.md](LAUNCH_CHECKLIST.md).
Portal background: [APP_STORE_RELEASE.md](APP_STORE_RELEASE.md) §5.

**One app record, two platforms (Universal Purchase).** Bundle id
`com.nofriction.meetings` on both. In App Store Connect the **name and
subtitle** live in *App Information* and are shared by iOS and macOS. The
description, keywords, promotional text, What's New, screenshots and URLs are
set **per platform** on each version page, so this doc gives both.

**Launch decisions (owner, 2026-10-03):** existing team `C7GCEESE2V`,
**US $0.99/month or $5.99/year**, and a **1-week free introductory
trial** for eligible subscribers. Prices and offers must be configured in App
Store Connect; keep public copy localized through the store.

**Domain correction from the release lane, 2026-10-03:** `nofriction.ai` belongs
to a different product and its supposed policy paths are not valid noFriction
meeting-app policy pages. Do not use that domain or `support@nofriction.ai` in
store metadata. Support, marketing, privacy and contact destinations remain
unresolved; existing source constants need a separately approved correction.

**ASC metadata readback, 2026-10-03:** app `6818838861` (`NOFRICTION-001`)
has iOS `1.0.0` and macOS `3.6.0`, both `PREPARE_FOR_SUBMISSION` with `MANUAL`
release. The en-US descriptions, promotional text, keywords, shared subtitle,
Productivity/Business categories and platform review notes below were accepted
and read back through Apple's public App Store Connect API. No app login is
required. URL fields and reviewer contact remain unset. No build was uploaded,
attached, submitted or released by this metadata step; no legal/content-rights
declaration or privacy questionnaire was changed. Receipts are retained in the
release lane's `apple-setup-20261003/nofriction-metadata-verified.json`.

**AI positioning:** noFriction supports fully offline operation with local or
Apple on-device models after setup/model downloads. It offers no hosted AI
models. Optional third-party connections use the user's own provider account.
Sources: [AI_PROVIDERS.md](AI_PROVIDERS.md),
[AppleOnDevice.swift](../ios/NoFriction/AI/AppleOnDevice.swift),
[AIProvider.swift](../ios/NoFriction/AI/AIProvider.swift).

**Source recheck (2026-10-03):** Mac NOTES is mounted in
[InsightDeckView.tsx](../src/components/agency/views/InsightDeckView.tsx), and
[MeetingNotesPanel.tsx](../src/components/MeetingNotesPanel.tsx) exposes Generate
notes, Regenerate and Follow-up email. The previous missing-feature caveats are
obsolete. Source inspection does not replace testing the signed store build.

**Students and classrooms (2026-10-06):** the descriptions, promotional
text, keywords, secondary category, review notes, What to Test and screenshot
plan now cover the student features: timed recording and classes
([TIMED_RECORDING_AND_CLASSES.md](TIMED_RECORDING_AND_CLASSES.md)), links
([LINKS.md](LINKS.md)), and moment markers and study guides
([STUDY_TOOLS.md](STUDY_TOOLS.md)). The copy was checked against the code on
that date. **App Store Connect still holds the 2026-10-03 text** and the
Business secondary category, so re-enter every field marked changed in §10.
The Mac copy doesn't mention links "seen on screen": the Mac App Store build
has no Accessibility text capture (`build_info::ACCESSIBILITY_CAPTURE` is off
in `mas`) and doesn't record browser addresses, so it has no screen text to
find links in. Only links said aloud and references the user adds apply
there, as on iOS.
---

## 1. App Information (shared by iOS and macOS)

### Name (limit 30)

Recommended:

**25 / 30 characters**

```text
noFriction: Meeting Notes
```

Alternate 1 (matches the Mac app's bundle name):

**19 / 30 characters**

```text
noFriction Meetings
```

Alternate 2:

**28 / 30 characters**

```text
noFriction: AI Meeting Notes
```

Notes: names are unique across the store; "noFriction" alone may be taken
(APP_STORE_RELEASE.md D5). The home-screen name stays "noFriction" on iOS
(`CFBundleDisplayName`) and "noFriction Meetings" on the Mac (`productName`),
whatever the store name is.

### Subtitle (limit 30)

Recommended:

**29 / 30 characters**

```text
Record, transcribe, summarize
```

Alternate:

**30 / 30 characters**

```text
Private notes with your own AI
```

### Category
- Primary: **Productivity**
- Secondary: **Education** (was Business until 2026-10-06)

Rationale: students record lectures, tag them by class, mark what's on the
test and study from flashcards and a practice quiz, so Education reaches
people browsing for study tools while Productivity keeps the meeting audience.

App Store Connect still shows Business from the 2026-10-03 entry; change the
secondary category in *App Information*. Choosing Education doesn't put the
app in the Kids Category (see Age rating). The Mac `Info.plist` declares only
the primary category (`public.app-category.productivity`), so it doesn't
change.

### Content rights
**"No, it does not contain, show, or access third-party content."**
Reasoning: the app shows only what the user records and imports. AI text comes
from the user's own provider account at the user's request. The Mac downloads
an open speech-recognition model file (not shown content). LinkedIn opens in
the browser; nothing is displayed in-app. Lecture recordings, references and
study guides are the user's own material, and links found in a recording are
never fetched or displayed as pages.

### Age rating
Expected result: **4+**. Answers for Apple's questionnaire (updated 2025):

| Question group | Answer | Reasoning |
|---|---|---|
| Parental controls / in-app controls | No | None in the app. |
| Age assurance | No | No accounts, no age gate. |
| Unrestricted web access | No | No in-app browser. Links (LinkedIn search, provider key pages, policies, and a meeting's Links list, http/https only) open in Safari / the default browser. The Mac app's web view loads only bundled UI. |
| User-generated content | No | Recordings, notes, photos, classes, references, moment markers and study guides are private to the user's device. Nothing is published or shared with other users of the app. |
| Messaging and chat | No | No person-to-person communication. (Mac "Chat" is a question box over the user's own transcripts, answered by the user's AI provider; it isn't messaging between users.) |
| Advertising | No | No ads. |
| Violence, sexual content/nudity, profanity or crude humor, horror/fear, mature/suggestive themes, alcohol/tobacco/drugs, simulated gambling, contests, loot boxes / chance-based items | None | The app ships no such content. |
| Medical or wellness topics | No | |
| Gambling | No | |

**AI questions.** If the form asks whether the app includes AI-generated
content or an AI assistant/chatbot, answer **Yes** and say: "AI features
summarize the user's own meeting and lecture transcripts and make study
material from them (notes, key terms, flashcards, practice quizzes), using
Apple's on-device model or an OpenAI-compatible endpoint the user enters, with
the user's own key if it needs one. Output is shown only to that user; prompts
are fixed to meeting and study tasks." Judgment call: the
Mac Chat accepts free-form questions, and the user's own model decides the
answer. 4+ is defensible because output isn't shared and the user chooses the
model; if Apple's form treats free-form generative chat as needing a higher
rating, accept the rating it computes rather than arguing it down.

The student features (timed recording, classes, links, moment markers, study
guides) add no content the questionnaire asks about, so the answers above
still apply. Rechecked 2026-10-06.

**Kids Category: not opted in.** noFriction is for high-school, college and
adult learners and for professionals. It is not designed for children under
13. Leave *Made for Kids* off and don't pick a Kids age band. The Education
secondary category doesn't change this, and the 4+ rating describes the
content, not the audience. The Kids Category would also require a parental
gate before the app's external links and purchases, which the app doesn't
have.

---

## 2. App Privacy (nutrition label, shared by both platforms)

**Tracking:** No. No data is used to track users; no tracking domains
(`NSPrivacyTracking = false`).

**Data collected by the developer:** none reaches us. The app sends data only
to the AI or transcription provider the user picks, with the user's own key,
when the user uses that feature (and on the Mac, automatically for live
insights and the after-meeting report once a provider is connected and
approved). Apple's definition of "collect" covers data a third party can keep
longer than needed to answer a request, and some AI providers retain request
data. So declare the following conservatively, consistent with the iOS
privacy manifest (`ios/NoFriction/PrivacyInfo.xcprivacy` declares Other User
Content, not linked, not tracking, App Functionality):

| Data type | Declare? | Linked to identity | Tracking | Purpose | Why |
|---|---|---|---|---|---|
| **User Content → Other User Content** | Yes | No | No | App Functionality | Transcript text, meeting title, class name, moment-marker notes, attendee names/companies and invite notes sent to the user's chosen AI provider (both platforms). |
| **User Content → Audio Data** | No | — | — | — | Transcription is on-device on both platforms (cloud transcription was removed from the Mac source on 2026-10-03), so meeting audio never leaves the device. |
| **User Content → Photos or Videos** | Yes | No | No | App Functionality | Mac only: screenshot analysis sends screenshots to the user's vision model. iOS photos never leave the device. |
| Contact Info, Contacts, Identifiers, Usage Data, Diagnostics, Location, Financial, Health, Browsing/Search History, Purchases, Sensitive Info | No | | | | Not collected. Attendee names are part of the meeting content above, not the address book. Purchases are handled by Apple. Crash reports shared through Apple's opt-in come from Apple, not an SDK. |

If you add any SDK, server, crash reporter or new destination, update this
section, `site/privacy.html` and the privacy manifest in the same change.

**Privacy Policy URL:** unresolved; leave the store field unset. The current
`nofriction.ai` constants in `ios/NoFriction/App/AppLinks.swift` and
`src/lib/build.ts` are known incorrect and must be corrected separately before
release. Do not treat the presence of those constants as a verified policy URL.

---

## 3. Export compliance (both platforms)

- `ITSAppUsesNonExemptEncryption = false` is set in both apps
  (`ios/project.yml`, `src-tauri/Info.plist`), so App Store Connect doesn't ask
  per build.
- If asked manually: *Does your app use encryption?* **Yes**, only HTTPS/TLS
  through Apple's networking and the Keychain. *Does it qualify for any
  exemptions?* **Yes**: it uses only encryption within the operating system
  (standard HTTPS; no proprietary or non-standard algorithms). No export
  documentation (CCATS) is needed; the French encryption declaration isn't
  needed for OS-provided crypto.

---

## 4. URLs (blocked pending correct destinations)

The release lane identified `nofriction.ai` as a different product. Its old
privacy/support paths and `support@nofriction.ai` must not be entered into App
Store Connect for this app. No replacement destination has been authorized.

| Field | Current action |
|---|---|
| Privacy Policy URL | Leave unset until the actual policy destination is approved and verified. |
| Support URL | Leave unset until the actual support destination is approved and verified. |
| Marketing URL | Leave unset until the actual product destination is approved and verified. |
| Reviewer contact | Supply the owner's real name, phone and email separately; never use placeholders. |

The source app links and existing website artifacts require a separate domain
correction. This metadata update did not change their URLs, legal documents,
content-rights declaration or privacy questionnaire.

Copyright remains dependent on the confirmed legal seller name.

---

## 5. iOS / iPadOS version page

### Promotional text (limit 170)

**166 / 170 characters**

```text
For students: record a lecture for a set time, tag it by class, mark what's on the test, then turn it into flashcards and a practice quiz. Transcribed on your device.
```

### Description (limit 4000)

**3865 / 4000 characters**

```text
noFriction records and transcribes meetings and lectures on your iPhone or iPad so you can stay in the conversation. Keep the recording, transcript, people and photos together, then create notes, follow-up drafts and study guides with the AI you choose.

WORK OFFLINE WITH LOCAL AI
Recording and transcription run on your device. On supported devices, noFriction Pro can use Apple's on-device model for summaries, decisions, action items, follow-up drafts and study guides without a cloud AI account or API key. Complete setup and any required model downloads before working offline.

Apple on-device AI requires iOS 26 or later, compatible Apple Intelligence hardware, Apple Intelligence enabled and its model ready. You can also connect your own local model through Ollama, LM Studio or an OpenAI-compatible endpoint. A model running on another computer needs a reachable local network.

noFriction does not host AI models or include cloud AI usage. Connecting a third-party cloud provider is optional, uses your own provider account and may incur charges from that provider. The app asks permission before sending meeting content to a cloud AI provider.

RECORD AND FIND THE IMPORTANT PARTS
• Live transcription with Apple's on-device speech recognition
• Recording continues with the screen locked
• Choose how long to record: 15, 30, 60 or 90 minutes, or no limit. It stops by itself, with a warning and +15 min before the end
• Play back meeting audio and search saved meetings
• Link recordings to calendar events and keep attendee information together
• Add photos of slides and whiteboards to the meeting
• See the web addresses said in a recording, and add references like the syllabus or a reading. Nothing is fetched

FOR STUDENTS AND CLASSES
• Tag a lecture with its class and filter your recordings by class
• Tap Mark this moment while you record: ★ Important, ? Question or ✎ On the test, with an optional note
• Notes for a class recording are lecture notes: key concepts, definitions, examples, and announcements and deadlines
• Make a study guide from a lecture: summary, key terms, flashcards, a practice quiz and questions to ask your instructor. Moments you marked On the test or Important get extra weight
• Quiz answers come with a short explanation and can jump to the moment in the lecture
• Export flashcards as a CSV file that popular flashcard apps can import, or share the whole guide as Markdown

TURN THE TRANSCRIPT INTO USEFUL NOTES
noFriction Pro adds AI summaries, decisions, action items, follow-up email drafts, lecture notes and study guides. Read and check generated notes before sharing them. Notes stay connected to the meeting that supplied their context.

KEEP CONTROL OF THE RECORD
Edit transcript text, remove unwanted material, or use Strike from the record to leave a visible marker. Deleting or striking transcript text also deletes that recording's study guide. Share a meeting as text when you choose. Meetings are stored on your device; signing up for a noFriction account is not required.

FREE AND PRO
Recording, on-device transcription, timed recording, classes, moment markers, links, calendar and people, photos, search, transcript editing and text sharing are available without Pro. AI features, including lecture notes and study guides, require noFriction Pro. One subscription unlocks Pro on iPhone, iPad and Mac.

noFriction Pro is an auto-renewing monthly or yearly subscription. Payment is charged to your Apple Account. Subscriptions renew automatically unless canceled at least 24 hours before the current period ends. Manage or cancel in your Apple Account settings. Optional third-party AI usage is billed separately by your provider.

Tell participants before recording and obtain any required consent. Many schools require the instructor's permission to record a class; check your school's policy.
```

### Keywords (limit 100)

Comma-separated, no spaces after commas. Doesn't repeat words already in the
recommended name or the subtitle ("noFriction", "meeting", "notes", "record",
"transcribe", "summarize"); no competitor or third-party trademarks (no
"Anki" or "Quizlet" anywhere in the metadata, guideline 2.3.7; the
description says the CSV works with "popular flashcard apps").
Rebalanced 2026-10-06: student terms first (lecture, class, study,
flashcards, quiz, student), then the meeting terms that matter most. Apple
combines words across the name, subtitle and keywords, so "lecture" here plus
"Notes" in the name covers searches for "lecture notes", and "lecture" plus
"recorder" covers "lecture recorder".

**98 / 100 characters**

```text
lecture,class,study,flashcards,quiz,student,transcription,recorder,minutes,action items,AI,offline
```

If the store name is **Name alt 1** ("noFriction Meetings", no "Notes"),
use this instead, which swaps `offline` for `notes`:

**96 / 100 characters**

```text
lecture,class,study,flashcards,quiz,student,transcription,recorder,minutes,action items,AI,notes
```

With **Name alt 2** ("noFriction: AI Meeting Notes"), `AI` is already in the
name: use the main string without `AI,` (95 characters).

### What's New in this version (limit 4000)

App Store Connect doesn't show this field for an app's first version. Use the
text in the TestFlight "What to Test" field now and keep it for 1.0.1.

**365 / 4000 characters**

```text
Welcome to noFriction for iPhone and iPad.
• Record and transcribe meetings on your device
• Calendar matching, attendees and a People list
• Photos of slides and whiteboards in the timeline
• AI notes and follow-up emails with your own key, or Apple's on-device model (noFriction Pro)
• Delete, or Strike from the record
• Stops automatically when the meeting ends
```

---

## 6. macOS version page

The Mac app's first App Store version is **3.6.0** (`tauri.conf.json`),
not 1.0; versions are per platform.

### Promotional text (limit 170)

**166 / 170 characters**

```text
For students: timed lecture recording, class tags, a hotkey to mark what's on the test, and study guides with flashcards and a practice quiz. Transcribed on your Mac.
```

### Description (limit 4000)

**3900 / 4000 characters**

```text
noFriction brings your Mac's meeting and lecture recordings, transcripts, screenshots and notes into one place. Record your microphone and the other participants, revisit the discussion in Rewind, and use the AI you choose to turn the transcript into useful work.

WORK OFFLINE WITH LOCAL MODELS
Choose local transcription and download a speech model once. After setup, transcription runs on your Mac without an internet connection. noFriction Pro can also use a local model for notes, follow-up drafts, study guides, questions and other text AI features.

Apple on-device AI is available on compatible Apple Intelligence Macs running macOS 26 or later, with Apple Intelligence enabled and its model ready. You can also connect your own installed Ollama, LM Studio or OpenAI-compatible model. A model on another computer needs a reachable local network. Complete model downloads and setup before working offline.

noFriction does not host AI models or include cloud AI usage. An optional custom AI endpoint uses your own account and may incur charges from its operator. When used, the relevant meeting content goes directly to the endpoint you entered; transcription always stays on your Mac.

RECORD AND REVIEW
• Capture your microphone and your Mac's audio
• Choose how long to record: 15, 30, 60 or 90 minutes, or no limit. It stops by itself, with a warning and +15 min before the end
• See a live transcript using local speech recognition
• Search meeting transcripts and revisit saved recordings
• Choose displays or windows to capture, and use Snap for an on-demand screenshot
• Review screenshots and transcript together in Rewind
• Connect meetings with calendar events and attendee information
• See the web addresses said in a recording, and add references like the syllabus or a reading. Nothing is fetched

FOR STUDENTS AND CLASSES
• Tag a lecture with its class and filter your recordings by class
• Mark a moment with ⌃⌥⌘M, even from another app: ★ Important, ? Question or ✎ On the test, with a note
• Find marks on the Rewind timeline, and show only On the test before an exam
• Notes for a class recording are lecture notes: key concepts, definitions and examples, and announcements and deadlines
• Make a study guide from a lecture: summary, key terms, flashcards, a practice quiz with explanations, and questions to ask your instructor
• Export flashcards as a CSV file that popular flashcard apps can import, or the whole guide as Markdown

NOTES, FOLLOW-UPS AND QUESTIONS
noFriction Pro adds AI notes with summaries, decisions and action items; lecture notes and study guides; follow-up email drafts; questions over your transcripts in Chat; and live insights during a recording. Review generated answers against the meeting before relying on or sharing them.

YOUR MEETING RECORDS
Keep meetings on your Mac, edit transcript text, remove unwanted material or use Strike from the record to leave a visible marker. Deleting or striking transcript text also deletes that recording's study guide. Export meeting material to your Obsidian vault. No noFriction account is required.

FREE AND PRO
Recording, transcription, timed recording, classes, moment markers, links, screenshots, calendar and people, search, editing and export are available without Pro. AI features, including lecture notes and study guides, require noFriction Pro. One subscription unlocks Pro on Mac, iPhone and iPad.

noFriction Pro is an auto-renewing monthly or yearly subscription. Payment is charged to your Apple Account. Subscriptions renew automatically unless canceled at least 24 hours before the current period ends. Manage or cancel in your Apple Account settings. Optional third-party AI usage is billed separately by your provider.

Tell participants before recording and obtain any required consent. Many schools require the instructor's permission to record a class; check your school's policy.
```

### Keywords (limit 100)

Same rules as iOS. `slides` replaces `offline` here: the Mac captures the
slides and screens shown during a lecture or call.

**97 / 100 characters**

```text
lecture,class,study,flashcards,quiz,student,transcription,recorder,minutes,action items,AI,slides
```

With **Name alt 1**, swap `slides` for `notes`:

**96 / 100 characters**

```text
lecture,class,study,flashcards,quiz,student,transcription,recorder,minutes,action items,AI,notes
```

With **Name alt 2**, use the main string without `AI,` (94 characters).

### What's New in this version (limit 4000)

Not shown for the first macOS version either; use it for TestFlight.

**381 / 4000 characters**

```text
noFriction is now on the Mac App Store.
• Choose where AI runs: Apple's on-device model, or your own compatible AI endpoint
• Delete, or Strike from the record: remove words, lines or screenshots everywhere
• Recordings stop when the meeting ends, with a 30-second banner to keep going
• Local transcription is the default
• One noFriction Pro subscription for Mac, iPhone and iPad
```

---

## 7. Subscriptions (App Store Connect → Monetization → Subscriptions)

Group reference name: `noFriction Pro`. Products:
`com.nofriction.meetings.pro.monthly`, `com.nofriction.meetings.pro.yearly`.
The owner approved US $0.99/month, $5.99/year and a 1-week free introductory
trial for eligible subscribers on 2026-10-03 (see LAUNCH_CHECKLIST.md §3).
These display names and descriptions show on both
platforms, so they avoid platform-specific features.

Subscription group display name (localization):

**14 / 30 characters**

```text
noFriction Pro
```

Monthly display name:

**11 / 30 characters**

```text
Pro Monthly
```

Monthly description:

**38 / 45 characters**

```text
AI meeting notes and insights, monthly
```

Yearly display name:

**10 / 30 characters**

```text
Pro Yearly
```

Yearly description:

**37 / 45 characters**

```text
AI meeting notes and insights, yearly
```

Limits used: display name 30, description 45 (App Store Connect's
in-app purchase localization limits). The local test file
`ios/NoFriction.storekit` uses longer descriptions ("AI meeting notes and
follow-up emails, billed yearly."); that file is only for local testing and
doesn't need to match.

Review information for each product: a screenshot of the paywall (iOS: the
paywall from "Summarize"; Mac: Settings → Subscription) and the note "Unlocks
the AI features. Reach it from a meeting's Summarize button (iOS) or Settings →
Subscription (Mac)."

---

## 8. App Review notes

Sign-in required: **No**. Paste the platform's notes into *App Review
Information → Notes* (limit 4000). Reviewer contact fields remain unset until
the owner supplies the actual name, phone and email. Verify the
on-device/local-model path on the submitted build and state compatible hardware
and model setup. noFriction supplies no hosted model or provider account. If
review requires an optional cloud-provider test, arrange a separate, restricted
review credential through App Review; never place a secret in the app or repo.

### iOS

**3593 / 4000 characters**

```text
No noFriction account or app login is required. The app does not provide a hosted AI model or a cloud-provider account. An Apple Account is needed for App Store sandbox subscription testing.

RECORD AND TRANSCRIBE
1. Open Record and tap Record. Review the recording notice, then pick a length in How long? (No limit is fine) and tap Start recording.
2. Allow Microphone and Speech Recognition. Calendar access enables calendar matching. On a physical device with on-device speech recognition available, speak for a minute and check the live transcript. Speech recognition is not supported by this test path in the Simulator.
3. Optionally add a photo. Stop, then open the meeting in Meetings.

PRO AND RESTORE
4. Open the meeting and choose Summarize or Follow-up email. Without Pro, the paywall opens. Settings → Subscription also offers the subscription and Restore Purchases.
5. Test with the App Store sandbox. Product IDs: com.nofriction.meetings.pro.monthly and com.nofriction.meetings.pro.yearly. AI features, including local AI, require the Pro entitlement.

OFFLINE AI PATH
Use an Apple Intelligence-compatible device running iOS 26 or later. Enable Apple Intelligence and finish its model download before going offline. In the AI setup or Settings, choose Use Apple on-device (no key), then return to Summarize or Follow-up email. This text-generation path runs on the device without a provider account or API key.

Alternatively, configure your own Ollama/LM Studio or OpenAI-compatible local model in Settings. A model on another computer requires a reachable local network. A custom endpoint is optional and entered by the user with their own account/key; the app requests consent before sending meeting content to a public endpoint. No provider credential is supplied in these notes.

TIMED RECORDING, CLASSES, MARKS AND LINKS (free)
6. Tap Record. How long? offers 15, 30, 60 or 90 min or No limit, and an optional Class (type BIO 101). The first class recording shows a one-time school-policy notice. The recording stops by itself at the end; a warning with +15 min and No limit comes 5 minutes before (2 for 15 min).
7. While recording, tap Mark this moment, then ? Question, ✎ On the test or Note within six seconds. On a physical device, say a web address such as "example dot com".
8. Stop. Meetings shows class chips that filter the list. In the meeting, Marked moments lists the marks, and Links lists the address as Said; tap Add to add a reference. Links are never fetched.

STUDY TOOLS (Pro)
9. In the meeting's Study section, tap Make study guide, then Open study guide: Summary, Key terms, Flashcards, Practice quiz and Questions to ask. Its share button exports flashcards as CSV and the guide as Markdown. Summarize on a class recording writes lecture notes. Study tools use the same AI as Summarize (Apple on-device or the user's own OpenAI-compatible endpoint) with the same Pro paywall and consent prompt.

APPLE WATCH
The Apple Watch app records a meeting and sends the audio to the iPhone app, which transcribes it on the device. Install it from the Watch app on the paired iPhone; tap Record, then Stop; the meeting appears in the iPhone app's Meetings tab. Delivery and transcription require a physical iPhone and Apple Watch.

EDITING
Open a meeting and select transcript text. Delete offers a brief undo period. Strike from the record is permanent and leaves a marker.

The local-model path requires its setup, compatible hardware and Pro entitlement; it is not a hosted fallback. Reviewer contact details will be supplied separately in App Store Connect.
```

### macOS

**3495 / 4000 characters**

```text
No noFriction account or app login is required. The app does not provide a hosted AI model or a cloud-provider account. An Apple Account is needed for App Store sandbox subscription testing.

SETUP AND RECORDING
1. In setup, choose the Recommended (or Smaller) speech model and complete the download while online. Model size depends on the selected model. Subsequent local transcription works offline.
2. Grant Microphone, Screen & System Audio Recording and Calendar permissions as needed for the features being tested.
3. Open LIVE and start recording; in How long?, pick a length (No limit is fine) and click Start recording. Speak or use a call to check the transcript. Choose displays/windows with Change and use Snap to save a screenshot.
4. Stop, then open REWIND → RECORDINGS and select the meeting to inspect the transcript and screenshots.

PRO AND RESTORE
5. Settings → Subscription offers plans and Restore Purchases. An AI action without Pro opens the paywall. Test with the App Store sandbox. Product IDs: com.nofriction.meetings.pro.monthly and com.nofriction.meetings.pro.yearly. AI features, including local AI, require Pro.

OFFLINE AI PATH
Use an Apple Intelligence-compatible Mac running macOS 26 or later, with Apple Intelligence enabled and its model download complete. With no text provider configured, the app uses the available Apple on-device model automatically for text AI. No cloud account or API key is required for that path.

Alternatively configure an installed Ollama/LM Studio or OpenAI-compatible local model in Settings → AI Engine. Complete model downloads before offline testing. A model on another computer requires a reachable local network.

Open CHAT and ask What did we decide? Or use REWIND → RECORDINGS → select a meeting → NOTES → Generate notes / Follow-up email. LIVE provides insights during a recording. Apple on-device is text-only; screenshot capture and review do not require image analysis.

TIMED RECORDING, CLASSES, MARKS AND LINKS (free)
6. Start another recording from LIVE. How long? offers 15, 30, 60 or 90 min or No limit (keys 1-5, Enter starts) and an optional Class (type BIO 101). The first class recording shows a one-time school-policy notice. The recording stops by itself at the end, after a banner with +15 min and No limit (5 minutes before; 2 for 15 min).
7. While recording, click Mark in the capture bar, or press ⌃⌥⌘M with another app in front (File → Mark Moment also works). Choose ? Question or ✎ On the test and add a note. Say a web address such as "example dot com".
8. Stop. REWIND → RECORDINGS shows class chips that filter the list, and marks on the timeline, in the transcript and in the Markers list. Select the meeting → LINKS lists the address as Said; use Add reference to add one. Links are never fetched.

STUDY TOOLS (Pro)
9. Select the meeting → STUDY → Make study guide: Summary, Key terms, Flashcards, Practice quiz and Questions to ask. Export flashcards (CSV) and Export study guide (Markdown) save to a file you choose. NOTES for a class recording are lecture notes. Study tools use the same AI as notes (Apple on-device or the user's own OpenAI-compatible endpoint) with the same Pro check and consent prompt.

A custom AI endpoint is optional, is entered by the user with their own account/key, and requires consent before meeting content is sent to a public endpoint. No provider credential is supplied in these notes. Reviewer contact details will be supplied separately in App Store Connect.
```

Optional attachment: a short screen recording of a recording → Summarize →
paywall → notes, if review asks how to reach a feature.

### TestFlight (external testing) Test Information

Beta app description (limit 4000):

**281 / 4000 characters**

```text
noFriction records and transcribes meetings on your device. Create notes and follow-ups with Apple on-device AI or your own local model after setup on supported devices. AI features require Pro. Optional cloud providers use your own account. No noFriction login or hosted AI model.
```

What to Test (limit 4000):

**1058 / 4000 characters**

```text
Please test:
• A real meeting from start to finish: recording, live transcript, calendar match and attendees
• Leaving the phone locked during a long meeting
• Auto-stop when the meeting ends (and "Keep recording")
• AI notes and follow-up email with Apple on-device or your own local model, after setup
• Delete and Strike from the record
• Subscribing, restoring and the free trial (purchases are free in TestFlight)
New for students:
• How long? Record for 15 minutes: check the warning, +15 min, No limit and the automatic stop
• Classes: add a class when you record, filter by it, and check that notes for a class read as lecture notes
• Mark moments while recording: Important, Question, On the test, with a note (Mac: ⌃⌥⌘M while another app is in front)
• Links: say a web address aloud, then check the meeting's Links; add a reference such as a syllabus
• Study guide (Pro): summary, key terms, flashcards, practice quiz and questions to ask, then export flashcards as CSV and the guide as Markdown
Send feedback with a screenshot through TestFlight.
```

Feedback email: unresolved; do not use the old `support@nofriction.ai` placeholder.
Beta review notes: the same as the
App Review notes above.

---

## 9. Screenshot plan

Rule for every image: **no real people, emails, meeting content, calendar
data or API keys.** Use only the demo data below. Apple requires screenshots
to show the app in use; captions are optional and can be added as plain
text above the screen (keep them to the benefit lines below).

### iPhone 6.9" (1320 × 2868 or 1290 × 2796) and iPad 13" (2064 × 2752 or 2048 × 2732)

Being generated by another agent under `ios/AppStore/screenshots/`, using the
debug-only `-NFSeedDemo` launch argument (sample meetings such as
"Kubernetes migration sync" with fictional attendees like Priya Shah and
Marcus Lee). The screen walk is the same as `ios/NoFrictionUITests/ScreensTests.swift`.
Order and suggested captions:

| # | Screen | How to reach it | Caption |
|---|---|---|---|
| 1 | Record (live transcript, recording) | Record tab while recording demo audio | Transcribed on your device |
| 2 | Meeting detail with AI notes | Meetings → "Kubernetes migration sync" (with notes generated) | Notes and action items, with your AI |
| 3 | Lecture: meeting detail of a class recording with its class, **Marked moments** (★, ?, ✎ with notes) and the **Study** section showing the guide is ready | Meetings → class chip "BIO 101" → "Cell structure and the membrane" | Mark what's on the test |
| 4 | Study guide: **Flashcards** with a card flipped to its back, or **Practice quiz** with an answered question, its explanation and "Jump to this moment" | That meeting → Study → open the guide | Flashcards and a practice quiz from every lecture |
| 5 | Meetings list | Meetings tab | Every meeting, named from your calendar |
| 6 | People | People tab | Know who was there |
| 7 | Strike from the record marker | Meeting → select words → Strike | Strike it from the record |
| 8 | Settings → Connect AI | Settings tab (no key entered) | Your key. Your provider. |
| 9 (optional) | Paywall | Meeting → Summarize without Pro | Free to record. Pro for AI. |

Rows 3 and 4 (added 2026-10-06) sit in the first four so a student sees a
study screen among the first screenshots. Use the same two screens, at the
iPad size, for iPad. Files already in `ios/AppStore/screenshots/` keep their
own numbers; save the new ones as `07-lecture.png` and `08-study.png` and set
the order above when uploading.

**Lecture demo data.** `-NFSeedDemo` (`ios/NoFriction/App/DemoData.swift`)
seeds meetings only: no class, markers or study guide. Either extend the seed
with a class recording, its markers and a stored study guide (a separate code
change), or capture on a physical device with Pro (sandbox) and Apple
on-device AI:
1. Record with **How long? 15 min** and class **BIO 101 — Cell Biology**,
   while a Mac plays an invented lecture made with `say` (for example,
   "Today: the cell membrane. The phospholipid bilayer controls what enters
   the cell. This will be on the midterm. Read openstax dot org before
   Friday."). Speech recognition doesn't work in the Simulator.
2. Mark one moment of each kind with a short invented note ("bilayer
   definition", "ask about channels", "midterm").
3. Change the meeting's title to "Cell structure and the membrane", tap
   **Make study guide**, then capture.
Keep the share sheet closed in the shots, so third-party app names don't
appear in the images.

Check before upload: the status bar shows a clean time (Simulator status bar
override `xcrun simctl status_bar booted override --time 9:41 --batteryLevel 100`),
no real key digits in Settings (the field must be empty), no real calendar
names, course codes, instructor or classmate names.

### Mac (16:10: 2880 × 1800, 2560 × 1600, 1440 × 900 or 1280 × 800)

The Mac app has no demo-data mode, so capture from a clean, fake profile.
Use a separate macOS user so none of your real calendars, keys or meetings
can appear.

**Prepare (once)**
1. System Settings → Users & Groups → Add User: "Demo" (standard user).
   Log in as Demo. Don't sign in to iCloud or add any internet accounts.
2. Calendar app → File → New Calendar → "On My Mac" calendar "Work".
   Create today's event "Kubernetes migration sync", 10:00–10:30, with
   notes "Agenda: cutover plan, rollback, owners". "On My Mac" calendars
   can't hold invitees, so the Mac shots show the meeting title without
   attendees. If you want attendees on screen, sign the Demo user in to a
   throwaway calendar account with an invented name and invite invented
   addresses on the reserved `example.com` domain (e.g. `priya.shah@example.com`).
3. Make the demo "meeting" audio from a script with built-in voices so no real
   voice is recorded:
   ```bash
   say -v Samantha -o /tmp/a.aiff "Thanks everyone. Let's settle the cutover plan for the Kubernetes migration."
   say -v Daniel   -o /tmp/b.aiff "We'll move the payments service on Thursday. I'll own the rollback runbook."
   say -v Karen    -o /tmp/c.aiff "Then we decided: freeze deploys Wednesday night, and Marcus checks the dashboards by Friday."
   ```
   Play them in order in QuickTime during recording (system audio is captured).
4. Make a 3-slide Keynote deck ("Cutover plan", "Rollback", "Owners") with
   invented text, for the screen capture.
5. Install the TestFlight or App Store build of noFriction for Mac in the Demo
   user. In the setup wizard choose the Recommended speech model and let it
   download. Grant Microphone, Screen & System Audio Recording and Calendars.
6. Settings → Subscription: subscribe in Apple's sandbox (TestFlight purchases
   are free). Use Apple on-device on a compatible Mac or connect your own local
   Ollama/LM Studio model in Settings → AI Engine. Finish model downloads first.
   Remove the demo profile after capturing the screenshots.
7. Turn on Do Not Disturb; hide the Dock (System Settings → Desktop & Dock →
   Automatically hide); use the default wallpaper; set Appearance to Dark.
8. Display: set the built-in display to "Default" scaling (1440 × 900 points
   on 13" Retina gives 2880 × 1800 pixel captures) or use any 16:10 display.
9. Lecture (for shots 3 and 4): add a "Work" calendar event "Cell structure
   and the membrane" right after the first one, a 3-slide Keynote deck
   ("The cell membrane", "Phospholipid bilayer", "Midterm topics") and an
   invented lecture from built-in voices:
   ```bash
   say -v Daniel -o /tmp/l1.aiff "Today we cover the cell membrane. The phospholipid bilayer controls what enters the cell."
   say -v Daniel -o /tmp/l2.aiff "Channel proteins let ions through. This will be on the midterm. Read openstax dot org before Friday."
   ```

**Capture**
1. Open the Keynote deck in a window. In noFriction LIVE → "Change", pick
   the Keynote window.
2. Record. Play the three audio clips. Click "Snap" once on the title slide,
   then advance slides.
3. Shots (window capture: ⌘⇧4, then Space, then click the noFriction window;
   hold Option to omit the window shadow, then crop/pad to 16:10):

| # | Screen | Caption |
|---|---|---|
| 1 | LIVE while recording: live transcript + captured slide thumbnails | Everyone on the call, transcribed on your Mac |
| 2 | REWIND → RECORDINGS: the meeting with transcript and screenshot timeline | Every slide, next to what was said |
| 3 | REWIND → RECORDINGS with the class chips (All · BIO 101), the lecture selected, marker pins on the timeline and the Markers list filtered to ✎ On the test | Mark what's on the test |
| 4 | The lecture → STUDY: Flashcards with a card flipped to its back, or Practice quiz with an answered question, its explanation and "Jump to this moment" | Flashcards and a practice quiz from every lecture |
| 5 | CHAT: ask "What did we decide about the cutover?" with the answer | Ask your meetings anything |
| 6 | LIVE → "LIVE INTELLIGENCE" cards during the recording (needs Pro + provider) | Action items as they happen |
| 7 | Settings → AI Engine with the provider list (key field empty, or showing only `••••` + last 4 of the throwaway key) | Bring your own AI |
| 8 | A transcript line struck from the record (marker visible) | Strike it from the record |
| 9 (optional) | REWIND → PEOPLE with invented attendees | Know who was there |

4. Stop the recording and let the auto-stop banner appear for an optional
   shot ("Meeting seems to have ended").
5. Lecture shots (3 and 4, added 2026-10-06): open the lecture deck, start a
   recording with **How long? 15 min** and class **BIO 101 — Cell Biology**,
   and play the two lecture clips. While they play, press **⌃⌥⌘M** three
   times with Keynote in front (each makes a ★ Important mark). Stop, then in
   REWIND → RECORDINGS select the lecture and, in the Markers list, change
   one mark to **✎ On the test** (note "midterm") and one to **? Question**.
   Then **STUDY** → **Make study guide**. Don't open the export save dialog
   in a shot.
6. Check every image at 100% for real names, emails, notifications, menu-bar
   items, course codes and key characters before uploading.

---

## 10. Character count summary

Recounted 2026-10-06 with Python `len()` on the exact text of every
`text` block above (lines joined with `\n`, no trailing newline; each newline
counts as one character). The script also checked that every keyword string
has no space after a comma, no repeated term and no word from the
recommended name or subtitle. "Changed" marks a field whose text changed on
2026-10-06; re-enter those in App Store Connect (and change the secondary
category to Education). Rows marked "recount" kept their text, but the old
figure was wrong.

| Field | Characters | Limit | 2026-10-06 |
|---|---|---|---|
| Name | 25 | 30 | |
| Name alt 1 | 19 | 30 | |
| Name alt 2 | 28 | 30 | |
| Subtitle | 29 | 30 | |
| Subtitle alt | 30 | 30 | |
| iOS promotional text | 166 | 170 | changed (was 141) |
| iOS description | 3865 | 4000 | changed (was 2584) |
| iOS keywords | 98 | 100 | changed (was 95) |
| iOS keywords, Name alt 1 | 96 | 100 | new |
| iOS What's New 1.0 | 365 | 4000 | |
| Mac promotional text | 166 | 170 | changed (was 148) |
| Mac description | 3900 | 4000 | changed (was 2687; listed as 2713) |
| Mac keywords | 97 | 100 | changed (was 96) |
| Mac keywords, Name alt 1 | 96 | 100 | new |
| Mac What's New 3.6.0 | 381 | 4000 | recount (listed as 409) |
| Group display name | 14 | 30 | |
| Monthly display name | 11 | 30 | |
| Monthly description | 38 | 45 | |
| Yearly display name | 10 | 30 | |
| Yearly description | 37 | 45 | |
| iOS review notes | 3593 | 4000 | changed (was 2377; listed as 2006 here) |
| Mac review notes | 3495 | 4000 | changed (was 2157; listed as 2109) |
| Beta description | 281 | 4000 | |
| What to Test | 1058 | 4000 | changed (was 470) |
