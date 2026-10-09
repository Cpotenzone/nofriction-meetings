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

**Students and classrooms (2026-10-06, first pass; the second pass below
replaces its student-first positioning):** the descriptions, promotional
text, keywords, secondary category, review notes, What to Test and screenshot
plan now cover the student features: timed recording and classes
([TIMED_RECORDING_AND_NOTEBOOKS.md](TIMED_RECORDING_AND_NOTEBOOKS.md)), links
([LINKS.md](LINKS.md)), and moment markers and study guides
([STUDY_TOOLS.md](STUDY_TOOLS.md)). The copy was checked against the code on
that date. **App Store Connect still holds the 2026-10-03 text** and the
Business secondary category, so re-enter every field marked changed in §10.
The Mac copy doesn't mention links "seen on screen": the Mac App Store build
has no Accessibility text capture (`build_info::ACCESSIBILITY_CAPTURE` is off
in `mas`) and doesn't record browser addresses, so it has no screen text to
find links in. Only links said aloud and references the user adds apply
there, as on iOS.

**Meetings, classes and everyday life (2026-10-06, second pass):** the owner
asked for the listing to cover everyone, not lead with students: note-taking
and Rewind for meetings, classes and the rest of life. The promotional text,
descriptions, keywords, What's New, review notes, TestFlight text and
screenshot plan now describe the update that both apps get together:
- Record asks **What is it?** (**Meeting · Class · Personal**, remembered,
  Meeting by default) above **How long?** "Personal covers everything else:
  conversations, appointments, talks, ideas."
- The old optional class tag is now a **Notebook** for any type ("Acme
  project", "BIO 101", "Health"), with **Notebooks** filter chips.
- Notes follow the type (meeting notes, lecture notes, or summary, key points
  and to-dos). The third mark follows it too: **Follow up** (Meeting), **On
  the test** (Class), **Remember** (Personal), beside ★ Important and
  ? Question.
- STUDY is now **REVIEW**: a Review guide for every type, called Study guide
  for a Class.
- The school-policy notice shows on the first Class recording.

This copy was written from that spec, before the update reached this branch.
Check the labels against the build being submitted before entering the text
in App Store Connect. The category stays Productivity + Education (§1).

---

## 1. App Information (shared by iOS and macOS)

### Name (limit 30)

Chosen by the owner on 2026-10-07 (entered in App Store Connect):

**28 / 30 characters**

```text
noFriction: Record your Life
```

Names are unique across the store. The home-screen name stays "noFriction" on
iOS (`CFBundleDisplayName`) and "noFriction" on the Mac
(`productName`), whatever the store name is.

### Subtitle (limit 30)

Doesn't repeat "Record" from the name, and names Rewind:

**29 / 30 characters**

```text
Transcribe, summarize, rewind
```

### Category
- Primary: **Productivity**
- Secondary: **Education** (was Business until 2026-10-06)

Rationale (rechecked for the universal positioning, 2026-10-06):
Productivity already covers note-taking for meetings and everyday life, so the
secondary category should add an audience Productivity doesn't reach.
Education does that for the Class type and its study guides; Business would
only repeat the meeting audience, so it doesn't fit the universal listing
better.

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
the browser; nothing is displayed in-app. Recordings of every type,
references and review guides are the user's own material, and links found in
a recording are never fetched or displayed as pages.

### Age rating
Expected result: **4+**. Answers for Apple's questionnaire (updated 2025):

| Question group | Answer | Reasoning |
|---|---|---|
| Parental controls / in-app controls | No | None in the app. |
| Age assurance | No | No accounts, no age gate. |
| Unrestricted web access | No | No in-app browser. Links (LinkedIn search, provider key pages, policies, and a meeting's Links list, http/https only) open in Safari / the default browser. The Mac app's web view loads only bundled UI. |
| User-generated content | No | Recordings, notes, photos, Notebooks, references, moment markers and review guides are private to the user's device. Nothing is published or shared with other users of the app. |
| Messaging and chat | No | No person-to-person communication. (Mac "Chat" is a question box over the user's own transcripts, answered by the user's AI provider; it isn't messaging between users.) |
| Advertising | No | No ads. |
| Violence, sexual content/nudity, profanity or crude humor, horror/fear, mature/suggestive themes, alcohol/tobacco/drugs, simulated gambling, contests, loot boxes / chance-based items | None | The app ships no such content. |
| Medical or wellness topics | No | |
| Gambling | No | |

**AI questions.** If the form asks whether the app includes AI-generated
content or an AI assistant/chatbot, answer **Yes** and say: "AI features
summarize the user's own recorded transcripts (meetings, classes and personal
recordings) and make review material from them (notes, key terms, flashcards,
practice quizzes), using Apple's on-device model or an OpenAI-compatible
endpoint the user enters, with the user's own key if it needs one. Output is
shown only to that user; prompts are fixed to note-taking and review tasks."
Judgment call: the
Mac Chat accepts free-form questions, and the user's own model decides the
answer. 4+ is defensible because output isn't shared and the user chooses the
model; if Apple's form treats free-form generative chat as needing a higher
rating, accept the rating it computes rather than arguing it down.

Recording types, Notebooks, timed recording, links, moment markers and review
guides add no content the questionnaire asks about, so the answers above
still apply. Rechecked 2026-10-06.

**Kids Category: not opted in.** noFriction is for adults at work and at
home, and for high-school and college students. It is not designed for
children under 13. Leave *Made for Kids* off and don't pick a Kids age
band. The Education secondary category doesn't change this, and the 4+
rating describes the content, not the audience. The Kids Category would also require a parental
gate before the app's external links and purchases, which the app doesn't
have.

---

## 2. App Privacy (nutrition label, shared by both platforms)

**Published 2026-10-07: "Data Not Collected"** (owner decision: "we don't
collect any data from the users").

**Tracking:** No. No tracking domains (`NSPrivacyTracking = false`).

**Why "Data Not Collected" is accurate:** Apple's "collect" means sending
data off the device so that *the developer or its third-party partners*
(analytics, ad networks, SDKs or vendors whose code is in the app) can keep
it longer than needed to answer the request. noFriction has no servers, no
analytics, no crash-reporting SDK and no partner code. Recording,
transcription and storage stay on the device. When a user connects an AI
endpoint they chose, the app sends that user's content there at their
request; that endpoint is the user's own provider, not our partner, and
nothing reaches us. Purchases are handled by Apple.

The privacy manifests match: `ios/NoFriction/PrivacyInfo.xcprivacy` and
`ios/NoFrictionWatch/PrivacyInfo.xcprivacy` declare no collected data types.

If you ever add an SDK, server, crash reporter or a destination the app
chooses for the user, update this section, the published label, the
privacy manifests and the privacy policy in the same change.

**Privacy Policy URL:** `https://nofriction.io/privacy` (set in App Store
Connect; matches `ios/NoFriction/App/AppLinks.swift` and `src/lib/build.ts`).
The live page is the company policy; it needs a noFriction app section (what
stays on the device, the optional user-chosen AI endpoint, no collection),
based on `site/privacy.html`, before review.

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

**159 / 170 characters**

```text
Meetings, classes and everyday life: record and transcribe on your device, mark the moments that matter, then turn any recording into notes and a review guide.
```

### Description (limit 4000)

App Store Connect rejects some symbols in descriptions with "This field contains one or more invalid characters" (seen 2026-10-07 for ★; key glyphs like ⌘ are avoided too). Write marks and shortcuts in words here; bullets (•) are fine.

**3987 / 4000 characters**

```text
noFriction records and transcribes on your iPhone or iPad so you can stay in the moment: a meeting, a class, or anything else worth keeping. Then create notes, follow-up drafts and review guides with the AI you choose.

WORK OFFLINE WITH LOCAL AI
Recording and transcription run on your device. On supported devices, noFriction Pro can use Apple's on-device model for notes, follow-up drafts and review guides without a cloud AI account or API key. Complete setup and any required model downloads before working offline.

Apple on-device AI requires iOS 26 or later, compatible Apple Intelligence hardware, Apple Intelligence enabled and its model ready. You can also connect a model you run yourself through an OpenAI-compatible endpoint. A model running on another computer needs a reachable local network.

noFriction does not host AI models or include cloud AI usage. Connecting a third-party cloud provider is optional, uses your own account and may incur charges from that provider. The app asks permission before sending recording content to it.

RECORD ANYTHING, FIND WHAT MATTERS
• Say what it is when you record: Meeting, Class or Personal. Notes, marks and the review guide follow the type
• Choose how long to record (15, 30, 60 or 90 minutes, or no limit). It stops by itself, after a warning with +15 min
• Live transcription with Apple's on-device speech recognition, even with the screen locked
• Tap Mark this moment while you record: Important, Question, or the third mark for the type, with an optional note
• Group recordings into Notebooks, such as "Acme project", "BIO 101" or "Health"
• Play back the audio and search everything you've recorded
• Add photos of slides, whiteboards and handouts
• See the web addresses said in a recording, and add your own references. Nothing is fetched

FOR MEETINGS
• Link recordings to calendar events and their attendees
• Meeting notes (summary, key topics, decisions, action items) and follow-up email drafts
• Mark Follow up as you go

FOR CLASSES
• Lecture notes: key concepts, definitions and examples, and announcements and deadlines
• The review guide becomes a study guide, with questions to ask your instructor. Moments you marked On the test or Important get extra weight

FOR EVERYDAY LIFE
• Personal covers everything else: conversations, appointments, talks, ideas
• Notes with a summary, key points, and to-dos and reminders
• Mark Remember on details you don't want to lose

REVIEW ANY RECORDING
noFriction Pro makes a review guide from any recording: summary, key terms, flashcards, a practice quiz and questions to ask. Each quiz answer has an explanation and can jump to its moment. Export flashcards as a CSV file that popular flashcard apps can import, or share the whole guide as Markdown. Read and check generated notes and guides before sharing them.

KEEP CONTROL OF THE RECORD
Edit transcript text, remove unwanted material, or use Strike from the record to leave a visible marker. Deleting or striking transcript text also deletes that recording's review guide. Share a recording as text when you choose. Recordings stay on your device, and no noFriction account is needed.

FREE AND PRO
Recording, on-device transcription, recording types, Notebooks, timed recording, marks, links, calendar and people, photos, search, editing and text sharing are free. AI features, including notes and review guides, require noFriction Pro. One subscription unlocks Pro on iPhone, iPad and Mac.

noFriction Pro is an auto-renewing monthly or yearly subscription charged to your Apple Account; it renews unless canceled at least 24 hours before the period ends. Manage it in your Apple Account settings.
Privacy Policy: https://nofriction.io/privacy
Terms of Use (Apple Standard EULA): https://www.apple.com/legal/internet-services/itunes/dev/stdeula/

Tell participants before recording and obtain any required consent. Many schools require the instructor's permission to record a class; check your school's policy.
```

### Keywords (limit 100)

Comma-separated, no spaces after commas. Doesn't repeat words already in the
name or the subtitle ("noFriction", "record", "your", "life", "transcribe",
"summarize", "rewind"); no competitor or third-party trademarks (no "Anki" or
"Quizlet" anywhere in the metadata, guideline 2.3.7; the description says the
CSV works with "popular flashcard apps"). Rebalanced 2026-10-07 for the name
"noFriction: Record your Life": "meeting" and "notes" are no longer in the name, so they lead. Apple
combines words across the name, subtitle and keywords ("meeting" + "notes",
"lecture" + "notes", "meeting" + "minutes", "voice" + "recorder").
"quiz" and "offline" were dropped for room; "flashcards" still reaches students.

**96 / 100 characters**

```text
meeting,notes,transcription,recorder,voice,memo,lecture,class,minutes,action items,flashcards,AI
```

### What's New in this version (limit 4000)

App Store Connect doesn't show this field for an app's first version. Use the
text in the TestFlight "What to Test" field now and keep it for 1.0.1.

**685 / 4000 characters**

```text
Welcome to noFriction for iPhone and iPad.
• Record and transcribe meetings, classes and everyday life on your device
• What is it? Meeting, Class or Personal: notes and marks follow the type
• Notebooks group your recordings, with filter chips
• Mark this moment: Important, Question, and Follow up, On the test or Remember
• Review guides with flashcards and a practice quiz (noFriction Pro)
• Calendar matching, attendees and a People list
• Photos of slides and whiteboards in the timeline
• AI notes and follow-up emails with Apple's on-device model or your own endpoint (noFriction Pro)
• Delete, or Strike from the record
• Stops at the time you choose, or when the meeting ends
```

---

## 6. macOS version page

The Mac app's first App Store version is **3.6.0** (`tauri.conf.json`),
not 1.0; versions are per platform.

### Promotional text (limit 170)

**158 / 170 characters**

```text
Notes for meetings, classes and everyday life, transcribed on your Mac. Rewind shows the screen next to what was said; a hotkey marks the moments that matter.
```

### Description (limit 4000)

**3989 / 4000 characters**

```text
noFriction keeps your Mac's recordings, transcripts, screenshots and notes in one place: meetings, classes and anything else worth keeping. Go back to any moment in Rewind, and turn the transcript into notes with the AI you choose.

WORK OFFLINE WITH LOCAL MODELS
Download a speech model once; after that, transcription runs on your Mac without an internet connection. noFriction Pro can also use a local model for notes, review guides and other text AI features.

Apple on-device AI is available on compatible Apple Intelligence Macs running macOS 26 or later, with Apple Intelligence enabled and its model ready. You can also connect a model you run yourself through an OpenAI-compatible endpoint. A model on another computer needs a reachable local network. Complete model downloads and setup before working offline.

noFriction does not host AI models or include cloud AI usage. An optional custom AI endpoint uses your own account and may incur charges from its operator. When used, recording content goes straight to the endpoint you entered.

RECORD AND REWIND
• Say what it is when you record: Meeting, Class or Personal. Notes, marks and the review guide follow the type
• Choose how long to record (15, 30, 60 or 90 minutes, or no limit). It stops by itself, after a warning with +15 min
• Capture your microphone and your Mac's audio, with a live local transcript
• Capture the displays or windows you choose, and see screenshots and transcript side by side in Rewind
• Mark a moment with Control-Option-Command-M, even from another app: Important, Question, or the third mark for the type, with a note, shown on the Rewind timeline
• Group recordings into Notebooks, such as "Acme project", "BIO 101" or "Health"
• Search every transcript
• See the web addresses said in a recording, and add your own references. Nothing is fetched

FOR MEETINGS
• Link recordings to calendar events and their attendees
• Meeting notes (summary, key topics, decisions, action items) and follow-up email drafts
• Live insights while you record
• Mark Follow up as you go

FOR CLASSES
• Lecture notes: key concepts, definitions and examples, and announcements and deadlines
• Mark On the test, then show only those marks before an exam
• The review guide becomes a study guide, with questions to ask your instructor

FOR EVERYDAY LIFE
• Personal covers everything else: conversations, appointments, talks, ideas
• Notes with a summary, key points, and to-dos and reminders
• Mark Remember on details you don't want to lose

REVIEW AND ASK
noFriction Pro makes a review guide from any recording: summary, key terms, flashcards, a practice quiz with explanations, and questions to ask. Export flashcards as a CSV file that popular flashcard apps can import, or the whole guide as Markdown. Ask your transcripts questions in Chat. Check generated text before relying on it.

YOUR RECORDS
Keep recordings on your Mac, edit transcript text, remove unwanted material or use Strike from the record to leave a visible marker. Deleting or striking transcript text also deletes that recording's review guide. Export recordings as Markdown files to a folder you choose. No noFriction account is required.

FREE AND PRO
Recording, transcription, recording types, Notebooks, timed recording, marks, links, screenshots, calendar and people, search, editing and export are free. AI features, including notes and review guides, require noFriction Pro. One subscription unlocks Pro on Mac, iPhone and iPad.

noFriction Pro is an auto-renewing monthly or yearly subscription charged to your Apple Account; it renews unless canceled 24 hours before the period ends. Manage it in Apple Account settings.
Privacy Policy: https://nofriction.io/privacy
Terms of Use (Apple Standard EULA): https://www.apple.com/legal/internet-services/itunes/dev/stdeula/

Tell participants before recording and obtain any required consent. Many schools require the instructor's permission to record a class; check your school's policy.
```

### Keywords (limit 100)

Comma-separated, no spaces after commas. Doesn't repeat words already in the
name or the subtitle ("noFriction", "record", "your", "life", "transcribe",
"summarize", "rewind"); no competitor or third-party trademarks (no "Anki" or
"Quizlet" anywhere in the metadata, guideline 2.3.7; the description says the
CSV works with "popular flashcard apps"). Rebalanced 2026-10-07 for the name
"noFriction: Record your Life": "meeting" and "notes" are no longer in the name, so they lead. Apple
combines words across the name, subtitle and keywords ("meeting" + "notes",
"lecture" + "notes", "meeting" + "minutes", "voice" + "recorder").
"rewind" is in the subtitle now; "slides" covers screen capture of presentations; "quiz" was dropped for room.

**100 / 100 characters**

```text
meeting,notes,transcription,recorder,slides,lecture,class,minutes,action items,flashcards,offline,AI
```

### What's New in this version (limit 4000)

Not shown for the first macOS version either; use it for TestFlight.

**804 / 4000 characters**

```text
noFriction is now on the Mac App Store.
• Notes and Rewind for meetings, classes and everyday life
• What is it? Meeting, Class or Personal: notes and marks follow the type
• Notebooks group your recordings, with filter chips in Rewind
• Mark a moment with ⌃⌥⌘M: Important, Question, and Follow up, On the test or Remember
• REVIEW: review guides with flashcards and a practice quiz (noFriction Pro)
• Choose how long to record: 15, 30, 60 or 90 minutes, or no limit
• Recordings stop when the meeting ends, with a 30-second banner to keep going
• Choose where AI runs: Apple's on-device model, or your own compatible AI endpoint
• Delete, or Strike from the record: remove words, lines or screenshots everywhere
• Transcription runs on your Mac
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

**40 / 45 characters**

```text
AI notes, summaries and reviews, monthly
```

Yearly display name:

**10 / 30 characters**

```text
Pro Yearly
```

Yearly description:

**39 / 45 characters**

```text
AI notes, summaries and reviews, yearly
```

Limits used: display name 30, description 45 (App Store Connect's
in-app purchase localization limits). The local test file
`ios/NoFriction.storekit` uses longer descriptions ("AI notes, summaries and
review guides, billed yearly."); that file is only for local testing and
doesn't need to match.

Review information for each product: a screenshot of the paywall (iOS: the
paywall from "Summarize"; Mac: Settings → Subscription) and the note "Unlocks
the AI features. Reach it from a recording's Summarize button (iOS) or Settings →
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

**3968 / 4000 characters**

```text
No noFriction account or app login is required. The app does not provide a hosted AI model or a cloud-provider account. An Apple Account is needed for App Store sandbox subscription testing.

RECORD AND TRANSCRIBE
1. Open Record and tap Record. After the recording notice, keep Meeting (the default) in What is it?, pick a length in How long? (No limit is fine) and tap Start recording.
2. Allow Microphone and Speech Recognition. Calendar access enables calendar matching. On a physical device with on-device speech recognition available, speak for a minute and check the live transcript. The Simulator can't test speech recognition.
3. Optionally add a photo. Stop, then open the meeting in Meetings.

PRO AND RESTORE
4. Open the meeting and choose Summarize or Follow-up email. Without Pro, the paywall opens. Settings → Subscription also offers the subscription and Restore Purchases.
5. Test with the App Store sandbox. Product IDs: com.nofriction.meetings.pro.monthly and com.nofriction.meetings.pro.yearly. AI features, including local AI, require the Pro entitlement.

OFFLINE AI PATH
Use an Apple Intelligence-compatible device running iOS 26 or later. Enable Apple Intelligence and finish its model download before going offline. In the AI setup or Settings, choose Use Apple on-device (no key), then return to Summarize or Follow-up email. This text-generation path runs on the device without a provider account or API key.

Alternatively, enter your own local OpenAI-compatible model server and model in Settings. A model on another computer requires a reachable local network. Provider presets (OpenAI, Anthropic, xAI, Mistral) only fill in the endpoint and model; the user supplies their own key, nothing is sent until they consent, and no provider is active by default. No provider credential is supplied in these notes.

RECORDING TYPES, NOTEBOOKS, TIMED RECORDING, MARKS AND LINKS (free)
6. Tap Record. In What is it?, choose Class. How long? offers 15, 30, 60 or 90 min or No limit, and an optional Notebook (type BIO 101). The first Class recording shows a one-time school-policy notice. The recording stops by itself at the end; a warning with +15 min and No limit comes 5 minutes before (2 for 15 min).
7. While recording, tap Mark this moment, then ? Question, ✎ On the test or Note within six seconds. On a physical device, say a web address such as "example dot com".
8. Stop. Recordings shows Notebook chips that filter the list. In the meeting, Marked moments lists the marks, and Links lists the address as Said; tap Add to add a reference. Links are never fetched.
Per type: the third mark is Follow up (Meeting), On the test (Class) or Remember (Personal), and the notes and guide follow the type (step 9). A short Meeting and a short Personal recording show the difference.

REVIEW (Pro)
9. In the meeting's Review section, make the guide and open it: Summary, Key terms, Flashcards, Practice quiz and Questions to ask. For a Class it is called Study guide; otherwise, Review guide. Its share button exports flashcards as CSV and the guide as Markdown. Summarize follows the type: meeting notes (summary, key topics, decisions, action items), lecture notes for a Class, and summary, key points and to-dos for Personal. Review uses the same AI, Pro paywall and consent prompt as Summarize.

APPLE WATCH
The Apple Watch app records and sends the audio to the iPhone app, which transcribes it on the device. Install it from the Watch app on the paired iPhone; tap Record, then Stop; the recording appears in the iPhone app's Recordings tab. Delivery and transcription require a physical iPhone and Apple Watch.

EDITING
Open a meeting and select transcript text. Delete offers a brief undo period. Strike from the record is permanent and leaves a marker.

The local-model path requires its setup, compatible hardware and Pro entitlement; it is not a hosted fallback. Reviewer contact details will be supplied separately in App Store Connect.
```

### macOS

**3958 / 4000 characters**

```text
No noFriction account or app login is required. The app does not provide a hosted AI model or a cloud-provider account. An Apple Account is needed for App Store sandbox subscription testing.

SETUP AND RECORDING
1. In setup, choose the Recommended (or Smaller) speech model and complete the download while online. Model size depends on the selected model. Subsequent local transcription works offline.
2. Grant Microphone, Screen & System Audio Recording and Calendar permissions as needed for the features being tested.
3. Open LIVE and start recording. Keep Meeting (the default) in What is it?, pick a length in How long? (No limit is fine) and click Start recording. Speak or use a call to check the transcript. Choose displays/windows with Change and use Snap to save a screenshot.
4. Stop, then open REWIND → RECORDINGS and select the meeting to inspect the transcript and screenshots.

PRO AND RESTORE
5. Settings → Subscription offers plans and Restore Purchases. An AI action without Pro opens the paywall. Test with the App Store sandbox. Product IDs: com.nofriction.meetings.pro.monthly and com.nofriction.meetings.pro.yearly. AI features, including local AI, require Pro.

OFFLINE AI PATH
Use an Apple Intelligence-compatible Mac running macOS 26 or later, with Apple Intelligence enabled and its model download complete. With no text provider configured, the app uses the available Apple on-device model automatically for text AI. No cloud account or API key is required for that path.

Alternatively enter a local OpenAI-compatible model server and model in Settings → AI Engine. Complete model downloads before offline testing. A model on another computer requires a reachable local network.

Open CHAT and ask What did we decide? Or use REWIND → RECORDINGS → select a meeting → NOTES → Generate notes / Follow-up email. LIVE provides insights during a recording. Apple on-device is text-only; screenshot capture and review do not require image analysis.

RECORDING TYPES, NOTEBOOKS, TIMED RECORDING, MARKS AND LINKS (free)
6. Start another recording from LIVE. In What is it?, choose Class. How long? offers 15, 30, 60 or 90 min or No limit (keys 1-5, Enter starts) and an optional Notebook (type BIO 101). The first Class recording shows a one-time school-policy notice. The recording stops by itself at the end, after a banner with +15 min and No limit (5 minutes before; 2 for 15 min).
7. While recording, click Mark in the capture bar, or press ⌃⌥⌘M with another app in front (File → Mark Moment also works). Choose ? Question or ✎ On the test and add a note. Say a web address such as "example dot com".
8. Stop. REWIND → RECORDINGS shows Notebook chips that filter the list, and marks on the timeline, in the transcript and in the Markers list. Select the meeting → LINKS lists the address as Said; use Add reference to add one. Links are never fetched.
Per type: the third mark is Follow up (Meeting), On the test (Class) or Remember (Personal), and the notes and guide follow the type (step 9). A short Meeting and a short Personal recording show the difference.

REVIEW (Pro)
9. Select the meeting → REVIEW and make the guide: Summary, Key terms, Flashcards, Practice quiz and Questions to ask. For a Class it is called Study guide; otherwise, Review guide. The flashcards export (CSV) and the guide export (Markdown) save to a file you choose. NOTES follow the type: meeting notes (summary, key topics, decisions, action items), lecture notes for a Class, and summary, key points and to-dos for Personal. Review uses the same AI, Pro check and consent prompt as notes.

Provider presets (OpenAI, Anthropic, xAI, Mistral) only fill in the endpoint and model; the user supplies their own key, nothing is sent until they consent, and no provider is active by default. Test connection sends only the word "Hi". No provider credential is supplied in these notes. Reviewer contact details will be supplied separately in App Store Connect.
```

Optional attachment: a short screen recording of a recording → Summarize →
paywall → notes, if review asks how to reach a feature.

### TestFlight (external testing) Test Information

Beta app description (limit 4000):

**308 / 4000 characters**

```text
noFriction records and transcribes meetings, classes and everyday life on your device. Create notes and follow-ups with Apple on-device AI or your own local model after setup on supported devices. AI features require Pro. Optional cloud providers use your own account. No noFriction login or hosted AI model.
```

What to Test (limit 4000):

**1366 / 4000 characters**

```text
Please test:
• A real meeting from start to finish: recording, live transcript, calendar match and attendees
• Leaving the phone locked during a long meeting
• Auto-stop when the meeting ends (and "Keep recording")
• AI notes and follow-up email with Apple on-device or your own local model, after setup
• Delete and Strike from the record
• Subscribing, restoring and the free trial (purchases are free in TestFlight)
New in this build:
• What is it? Record one Meeting, one Class and one Personal recording. Check that the choice is remembered and that the notes follow the type: meeting notes, lecture notes, or summary, key points and to-dos
• Notebooks: add one when you record (such as "Acme project" or "BIO 101"), then filter by it
• Mark moments while recording: Important, Question, and Follow up, On the test or Remember depending on the type, with a note (Mac: ⌃⌥⌘M while another app is in front)
• How long? Record for 15 minutes: check the warning, +15 min, No limit and the automatic stop
• Links: say a web address aloud, then check the recording's Links; add a reference such as an agenda or a reading
• Review (Pro): make the guide for each type (Study guide for a Class): summary, key terms, flashcards, practice quiz and questions to ask, then export flashcards as CSV and the guide as Markdown
Send feedback with a screenshot through TestFlight.
```

Feedback email: unresolved; do not use the old `support@nofriction.ai` placeholder.
Beta review notes: the same as the
App Review notes above.

---

## 9. Screenshot plan

**Uploaded 2026-10-07 (via the App Store Connect API), replacing the pre-relabel set:**

| Platform | Set | Files (source in `marketing/film/stills/`) |
|---|---|---|
| iPhone 6.9" | 5 screenshots, 1260×2736 | `iphone-6.9/01-record-sheet`, `02-class-marks`, `03-review-guide`, `04-flashcards-quiz`, `05-notebooks` |
| iPhone 6.9" | app preview, 886×1920, 28.8 s | `marketing/out/nofriction-app-preview-iphone-6.9in-886x1920.mp4` (rendered, not committed) |
| Mac | 5 screenshots, 2880×1800 | `mac/01-rewind`, `02-record-sheet`, `03-review`, `04-links`, `05-notebooks` |
| Mac | app preview, 1920×1080, 28.8 s | `marketing/out/nofriction-app-preview-mac-1920x1080.mp4` (rendered, not committed) |

| Apple Watch | 5 screenshots, 416×496 (Series 10/11/12 size), required once the Watch app ships in the build | `ios/AppStore/screenshots/watch-46mm/01-05` |

The iPad 13" set still holds the older six screenshots (pre-relabel wording); replace it from the film harness before a later submission. Everything else below is the original plan.



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
| 2 | The Record sheet: **What is it?** (Meeting · Class · Personal) above **How long?** | Record → Record, before Start recording | Meetings, classes and everyday life |
| 3 | Meeting detail with AI notes | Meetings → "Kubernetes migration sync" (with notes generated) | Notes and action items, with your AI |
| 4 | Class recording: Notebook BIO 101, **Marked moments** (★, ?, ✎ On the test, with notes) and the **Review** section showing the study guide is ready | Meetings → Notebook chip "BIO 101" → "Cell structure and the membrane" | Mark what's on the test |
| 5 | Study guide: **Flashcards** with a card flipped to its back, or **Practice quiz** with an answered question, its explanation and "Jump to this moment" | That recording → Review → open the guide | Flashcards and a quiz from any recording |
| 6 | Personal recording: notes with summary, key points and to-dos, and a Remember mark | Meetings → Notebook chip "Home" → "Kitchen renovation walkthrough" | Remember what was said, and what to do |
| 7 | Recordings list with Notebook chips (All · Acme project · BIO 101 · Home) | Recordings tab | Group recordings into Notebooks |
| 8 | People | People tab | Know who was there |
| 9 | Strike from the record marker | Meeting → select words → Strike | Strike it from the record |
| 10 | Settings → Connect AI | Settings tab (no key entered) | Your key. Your provider. |

Rebalanced 2026-10-06 (second pass) so the first six show a meeting, a class
and an everyday recording, with the type picker second. Apple allows at most
10 screenshots per size, so the paywall ("Free to record. Pro for AI.") left
the set; it stays the subscription review screenshot (§7,
`ios/AppStore/review-screenshots/`). Use the same screens, at the iPad size,
for iPad. Files already in `ios/AppStore/screenshots/` keep their own
numbers; save the new ones as `07-lecture.png`, `08-study.png`,
`09-what-is-it.png` and `10-personal.png`, re-capture `06-meetings.png` with
the Notebook chips, and set the order above when uploading. Re-capture any
existing shot whose screen changed in the update (for example, the Record and
meeting screens now show the type and Notebook).

**Demo data for rows 4–7.** `-NFSeedDemo` (`ios/NoFriction/App/DemoData.swift`)
seeds meetings only: no type, Notebook, markers or guide. Either extend the
seed with a Class and a Personal recording, their markers and a stored guide
(a separate code change), or capture on a physical device with Pro (sandbox)
and Apple on-device AI. Speech recognition doesn't work in the Simulator.
1. Lecture: record with **What is it? Class**, **How long? 15 min** and
   Notebook **BIO 101 — Cell Biology**, while a Mac plays an invented lecture
   made with `say` (for example, "Today: the cell membrane. The phospholipid
   bilayer controls what enters the cell. This will be on the midterm. Read
   the chapter at example dot org before Friday."). Mark one moment of each
   kind with a short invented note ("bilayer definition", "ask about
   channels", "midterm"). Change the title to "Cell structure and the
   membrane" and make the study guide.
2. Everyday: record with **What is it? Personal** and Notebook **Home** while
   the Mac plays an invented talk (for example, "The contractor can start on
   the fourteenth. The tiles have to be ordered by Friday, and the old
   cabinets go out first."). Mark **Remember** with the note "order tiles".
   Change the title to "Kitchen renovation walkthrough" and tap Summarize.
3. Give "Kubernetes migration sync" the Notebook **Acme project** on the
   recording, so row 7 shows all three chips. Then capture.
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
   are free). Use Apple on-device on a compatible Mac or enter your own local
   OpenAI-compatible model server in Settings → AI Engine. Finish model
   downloads first.
   Remove the demo profile after capturing the screenshots.
7. Turn on Do Not Disturb; hide the Dock (System Settings → Desktop & Dock →
   Automatically hide); use the default wallpaper; set Appearance to Dark.
8. Display: set the built-in display to "Default" scaling (1440 × 900 points
   on 13" Retina gives 2880 × 1800 pixel captures) or use any 16:10 display.
9. Lecture (for shots 4 and 5): add a "Work" calendar event "Cell structure
   and the membrane" right after the first one, a 3-slide Keynote deck
   ("The cell membrane", "Phospholipid bilayer", "Midterm topics") and an
   invented lecture from built-in voices:
   ```bash
   say -v Daniel -o /tmp/l1.aiff "Today we cover the cell membrane. The phospholipid bilayer controls what enters the cell."
   say -v Daniel -o /tmp/l2.aiff "Channel proteins let ions through. This will be on the midterm. Read the chapter at example dot org before Friday."
   ```
10. Everyday (for the Home chip in shot 4): an invented talk from a built-in
    voice:
    ```bash
    say -v Karen -o /tmp/p1.aiff "The contractor can start on the fourteenth. The tiles have to be ordered by Friday."
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
| 3 | The record sheet: **What is it?** (Meeting · Class · Personal) above **How long?** | Meetings, classes and everyday life |
| 4 | REWIND → RECORDINGS with the Notebook chips (All · Acme project · BIO 101 · Home), the lecture selected, marker pins on the timeline and the Markers list filtered to ✎ On the test | Mark what's on the test |
| 5 | The lecture → REVIEW: Flashcards with a card flipped to its back, or Practice quiz with an answered question, its explanation and "Jump to this moment" | Flashcards and a quiz from any recording |
| 6 | CHAT: ask "What did we decide about the cutover?" with the answer | Ask your recordings anything |
| 7 | LIVE → "LIVE INTELLIGENCE" cards during the recording (needs Pro + a configured AI) | Action items as they happen |
| 8 | Settings → AI Engine (key field empty, or showing only `••••` + last 4 of the throwaway key) | Bring your own AI |
| 9 | A transcript line struck from the record (marker visible) | Strike it from the record |
| 10 (optional) | REWIND → PEOPLE with invented attendees | Know who was there |

4. Stop the recording and let the auto-stop banner appear for an optional
   shot ("This seems to have ended"). Give this recording the Notebook
   **Acme project** in REWIND → RECORDINGS.
5. Type picker (shot 3): click LIVE → Start recording and capture the sheet
   before starting, with Meeting selected and a length picked.
6. Home chip: record the everyday clip with **What is it? Personal** and
   Notebook **Home**, mark **Remember**, and stop.
7. Lecture shots (4 and 5): open the lecture deck, start a recording with
   **What is it? Class**, **How long? 15 min** and Notebook **BIO 101 — Cell
   Biology**, and play the two lecture clips. While they play, press
   **⌃⌥⌘M** three times with Keynote in front (each makes a ★ Important
   mark). Stop, then in REWIND → RECORDINGS select the lecture and, in the
   Markers list, change one mark to **✎ On the test** (note "midterm") and one
   to **? Question**. Then **REVIEW** → make the study guide. Don't open the
   export save dialog in a shot.
8. Check every image at 100% for real names, emails, notifications, menu-bar
   items, course codes and key characters before uploading.

---

## 10. Character count summary

Recounted 2026-10-06 (second pass: meetings, classes and everyday life) with
Python `len()` on the exact text of every `text` block above (lines joined
with `\n`, no trailing newline; each newline counts as one character). The
script also checked that every keyword string has no space after a comma, no
repeated term and no word from the recommended name or subtitle (the Name
alt 1 strings may use `notes`).

"Changed" marks a field whose text changed in this pass; "was" is its count
in the previous version of this doc (the student pass, PR #13). App Store
Connect still holds the 2026-10-03 text, so re-enter every field marked
changed. The secondary category in App Store Connect still needs changing
from Business to Education.

| Field | Characters | Limit | This pass |
|---|---|---|---|
| Name | 28 | 30 | changed (was 25, "noFriction: Meeting Notes") |
| Subtitle | 29 | 30 | changed (was "Record, transcribe, summarize") |
| iOS promotional text | 159 | 170 | changed (was 166) |
| iOS description | 3987 | 4000 | changed (was 3865) |
| iOS keywords | 96 | 100 | changed |
| iOS What's New 1.0 | 685 | 4000 | changed (was 365) |
| Mac promotional text | 158 | 170 | changed (was 166) |
| Mac description | 3989 | 4000 | changed (was 3900) |
| Mac keywords | 100 | 100 | changed |
| Mac What's New 3.6.0 | 804 | 4000 | changed (was 381) |
| Group display name | 14 | 30 | |
| Monthly display name | 11 | 30 | |
| Monthly description | 38 | 45 | |
| Yearly display name | 10 | 30 | |
| Yearly description | 37 | 45 | |
| iOS review notes | 3935 | 4000 | changed (was 3593) |
| Mac review notes | 3882 | 4000 | changed (was 3495) |
| Beta description | 308 | 4000 | changed (was 281) |
| What to Test | 1366 | 4000 | changed (was 1058) |
