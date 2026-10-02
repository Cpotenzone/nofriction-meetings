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

> **Before you paste: copy that depends on code being fixed.** The facts
> below are true of the current code, with these exceptions to resolve first
> (all in `src/`, outside this doc's scope):
> 1. The Mac paywall (`src/components/PaywallModal.tsx`, `PRO_FEATURES`)
>    lists **"Follow-up email drafts"**, but the Mac app has no follow-up
>    email feature. Remove that line (or build the feature) before review;
>    a paywall that sells a missing feature risks rejection (2.3.1/3.1.2).
> 2. On the Mac, AI meeting notes (summary, decisions, action items) are
>    generated, but no mounted screen shows them
>    (`src/components/MeetingDetailView.tsx` isn't used). They reach the user
>    only through Obsidian export. The Mac copy therefore says "ready to export
>    to your Obsidian vault" and leads with Chat. If a notes view is wired up,
>    you can say "read the notes in Rewind".
> 3. Mac Help text says "Click Generate Report", a button that isn't mounted.
> 4. The trial: the copy says "free trial" only in the terms line ("when a
>    free trial ends"). If you launch without an introductory offer, it's
>    still accurate; don't add a trial length anywhere except App Store Connect.

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
- Secondary: **Business**

(The Mac `Info.plist` already declares `public.app-category.productivity`.)

### Content rights
**"No, it does not contain, show, or access third-party content."**
Reasoning: the app shows only what the user records and imports. AI text comes
from the user's own provider account at the user's request. The Mac downloads
an open speech-recognition model file (not shown content). LinkedIn opens in
the browser; nothing is displayed in-app.

### Age rating
Expected result: **4+**. Answers for Apple's questionnaire (updated 2025):

| Question group | Answer | Reasoning |
|---|---|---|
| Parental controls / in-app controls | No | None in the app. |
| Age assurance | No | No accounts, no age gate. |
| Unrestricted web access | No | No in-app browser. Links (LinkedIn search, provider key pages, policies) open in Safari / the default browser. The Mac app's web view loads only bundled UI. |
| User-generated content | No | Recordings, notes and photos are private to the user's device. Nothing is published or shared with other users of the app. |
| Messaging and chat | No | No person-to-person communication. (Mac "Chat" is a question box over the user's own transcripts, answered by the user's AI provider; it isn't messaging between users.) |
| Advertising | No | No ads. |
| Violence, sexual content/nudity, profanity or crude humor, horror/fear, mature/suggestive themes, alcohol/tobacco/drugs, simulated gambling, contests, loot boxes / chance-based items | None | The app ships no such content. |
| Medical or wellness topics | No | |
| Gambling | No | |

**AI questions.** If the form asks whether the app includes AI-generated
content or an AI assistant/chatbot, answer **Yes** and say: "AI features
summarize the user's own meeting transcripts using the AI provider the user
connects with their own API key, or Apple's on-device model. Output is shown
only to that user; prompts are fixed to meeting tasks." Judgment call: the
Mac Chat accepts free-form questions, and the user's own model decides the
answer. 4+ is defensible because output isn't shared and the user chooses the
model; if Apple's form treats free-form generative chat as needing a higher
rating, accept the rating it computes rather than arguing it down.

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
| **User Content → Other User Content** | Yes | No | No | App Functionality | Transcript text, meeting title, attendee names/companies and invite notes sent to the user's chosen AI provider (both platforms). |
| **User Content → Audio Data** | Yes | No | No | App Functionality | Mac only: if the user turns on optional cloud transcription (Deepgram, Gladia, Google, Gemini), meeting audio goes to that provider. iOS never sends audio. |
| **User Content → Photos or Videos** | Yes | No | No | App Functionality | Mac only: screenshot analysis sends screenshots to the user's vision model. iOS photos never leave the device. |
| Contact Info, Contacts, Identifiers, Usage Data, Diagnostics, Location, Financial, Health, Browsing/Search History, Purchases, Sensitive Info | No | | | | Not collected. Attendee names are part of the meeting content above, not the address book. Purchases are handled by Apple. Crash reports shared through Apple's opt-in come from Apple, not an SDK. |

If you add any SDK, server, crash reporter or new destination, update this
section, `site/privacy.html` and the privacy manifest in the same change.

**Privacy Policy URL:** `https://nofriction.ai/privacy`. This is the URL
constant in both apps (`ios/NoFriction/App/AppLinks.swift`,
`src/lib/build.ts`). It's a placeholder until the site is live; see
`site/README.md`.

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

## 4. URLs (per platform; same values)

| Field | Value | Source |
|---|---|---|
| Privacy Policy URL | `https://nofriction.ai/privacy` | `AppLinks.privacyPolicy`, `PRIVACY_URL` (placeholder) |
| Support URL | `https://nofriction.ai/support.html` | `site/support.html` |
| Marketing URL | `https://nofriction.ai/` | `site/index.html` |
| Terms of Use (in description) | `https://www.apple.com/legal/internet-services/itunes/dev/stdeula/` | `AppLinks.terms`, `TERMS_URL` |
| Support email | `support@nofriction.ai` | `AppLinks.supportEmail` |

All three nofriction.ai URLs are placeholders until the domain is set up.
If the domain changes, update the two code constants (owned outside `docs/`).

Copyright: `2026 <legal seller name>`.

---

## 5. iOS / iPadOS version page

### Promotional text (limit 170)

**153 / 170 characters**

```text
Record and transcribe meetings on your device. Bring your own AI key for notes and action items, or use Apple's on-device model. No accounts. No servers.
```

### Description (limit 4000)

**3345 / 4000 characters**

```text
noFriction writes your meetings down so you can pay attention to them. It records and transcribes on your iPhone or iPad, knows which calendar event you're in and who was there, and, when you want them, writes the notes and the follow-up email with the AI you choose.

No account. No noFriction servers. Your recordings stay on your device.

RECORD AND TRANSCRIBE ON YOUR DEVICE
• Live transcript as people talk, from Apple's on-device speech recognition
• Keeps recording with the screen locked
• Play back the audio of any meeting
• Search every meeting by title, person or anything said

KNOWS THE MEETING AND THE PEOPLE
• Matches each recording to your calendar event, so it gets a real title and attendee list
• A People list across all your meetings
• Link each person's LinkedIn profile in a couple of taps

SLIDES AND WHITEBOARDS
• Take photos during the meeting, or add screenshots from Photos
• They appear in the meeting with the transcript

AI NOTES WITH YOUR OWN KEY (noFriction Pro)
• Summary, decisions and action items. Owners and due dates only when someone actually said them
• A follow-up email draft for the attendees
• Paste a key and noFriction recognizes the provider and checks it: OpenAI (default), Anthropic, Google Gemini, xAI Grok, Groq, OpenRouter, Mistral, DeepSeek, Perplexity or Together
• Or use your own server (Ollama, LM Studio, any OpenAI-compatible endpoint), or Apple's on-device model with no key (iOS 26 or later with Apple Intelligence)
• Your provider bills you directly. noFriction never sees your key or your meetings

STOPS WHEN THE MEETING DOES
• When the calendar event is over and the room goes quiet, or nobody has spoken for a few minutes, noFriction counts down 30 seconds and stops, unless you tap Keep recording
• Everything said before the stop is kept

DELETE, OR STRIKE FROM THE RECORD
• Remove words, whole lines or photos
• Delete leaves no trace, with five seconds to undo
• Strike from the record removes it from the transcript, search and the saved audio, and leaves only a marker with the time and an optional reason. No undo, no hidden copy
• AI notes made before the edit are marked so you can regenerate them

PRIVATE BY DESIGN
• Audio, transcripts and photos are stored only on your device
• API keys are kept in the Keychain on this device
• Before anything goes to a cloud AI provider, noFriction asks you and says what will be sent
• No analytics, no ads, no tracking

FREE AND PRO
Free: recording, on-device transcription, calendar and people, photos, search, Delete and Strike, and sharing a meeting as text.
noFriction Pro adds the AI features. One subscription also unlocks noFriction for Mac.

Recording laws vary, and in many places everyone must agree to be recorded. noFriction reminds you to tell people.

noFriction Pro is an auto-renewing subscription (monthly or yearly). Payment is charged to your Apple Account when you confirm the purchase, or when a free trial ends. It renews automatically unless canceled at least 24 hours before the end of the current period. Manage or cancel in your Apple Account settings. Any unused part of a free trial ends when you subscribe. AI provider usage is billed by your provider, not by noFriction.
Terms of Use: https://www.apple.com/legal/internet-services/itunes/dev/stdeula/
Privacy Policy: https://nofriction.ai/privacy
```

### Keywords (limit 100)

Comma-separated, no spaces after commas. Doesn't repeat words already in the
name or subtitle ("noFriction", "meeting", "notes", "record", "transcribe",
"summarize"); no competitor or third-party trademarks.

**95 / 100 characters**

```text
transcription,recorder,minutes,action items,summary,AI,audio,calendar,private,offline,follow-up
```

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

**153 / 170 characters**

```text
Record, transcribe and capture screens on your Mac. Ask your meetings questions with your own AI key or Apple's on-device model. No accounts. No servers.
```

### Description (limit 4000)

**3432 / 4000 characters**

```text
noFriction is a meeting recorder for your Mac that writes things down while you pay attention. It records your microphone and the other participants, transcribes on your Mac, keeps the screens you choose, and works with the AI you pick to answer questions across all your meetings.

No account. No noFriction servers. Your recordings stay on your Mac.

RECORD EVERYONE, TRANSCRIBE ON YOUR MAC
• Captures your microphone and your Mac's audio, so the other people on the call are transcribed too
• Live transcript from a Whisper speech model that runs on your Mac. One-time model download, then it works offline
• Optional cloud transcription with your own key (Deepgram, Gladia or Google)
• Search every meeting's transcript

KEEP THE SCREENS THAT MATTER
• Pick the displays or windows to capture. noFriction saves a screenshot when they change
• Snap saves one on demand
• Review the transcript and screenshots together in Rewind

KNOWS THE MEETING AND THE PEOPLE
• Matches each recording to your calendar event and its attendees
• A People directory with search, invite notes and LinkedIn links

ASK YOUR MEETINGS (noFriction Pro)
• Chat: ask a question and get an answer drawn from your own transcripts
• Live insights while you talk: action items, decisions, risks and deadlines
• A report after each meeting: summary, decisions and action items, ready to export to your Obsidian vault
• Paste a key and noFriction recognizes the provider and checks it: OpenAI (default), Anthropic, Google Gemini, xAI Grok, Groq, OpenRouter, Mistral, DeepSeek, Perplexity or Together
• Or use a local model (Ollama, LM Studio, any OpenAI-compatible server), or Apple's on-device model with no key (macOS 26 or later with Apple Intelligence)

STOPS WHEN THE MEETING DOES
• Notices when the call app lets go of the microphone (macOS 14.2 or later), the meeting window closes, the calendar event ends, or nobody has spoken for a few minutes
• A 30-second banner lets you keep recording

DELETE, OR STRIKE FROM THE RECORD
• Remove words, whole lines or screenshots
• Delete leaves no trace, with five seconds to undo
• Strike from the record removes it from the transcript, search, screenshots and the app's backups, and leaves only a marker with the time and an optional reason. No undo

PRIVATE BY DESIGN
• Meetings are stored only on your Mac
• API keys are kept in the Keychain
• Before anything goes to a cloud AI provider, noFriction asks you and says what will be sent
• No analytics, no ads, no tracking

FREE AND PRO
Free: recording, transcription, screenshots and Snap, calendar and people, search, Delete and Strike, and export to Obsidian.
noFriction Pro adds the AI features. One subscription also unlocks noFriction for iPhone and iPad.

Recording laws vary, and in many places everyone must agree to be recorded. Tell people when you record.

Requires macOS 12.3 or later.

noFriction Pro is an auto-renewing subscription (monthly or yearly). Payment is charged to your Apple Account when you confirm the purchase, or when a free trial ends. It renews automatically unless canceled at least 24 hours before the end of the current period. Manage or cancel in your Apple Account settings. Any unused part of a free trial ends when you subscribe. AI provider usage is billed by your provider, not by noFriction.
Terms of Use: https://www.apple.com/legal/internet-services/itunes/dev/stdeula/
Privacy Policy: https://nofriction.ai/privacy
```

### Keywords (limit 100)

**96 / 100 characters**

```text
transcription,recorder,minutes,action items,summary,AI,audio,calendar,screenshot,private,offline
```

### What's New in this version (limit 4000)

Not shown for the first macOS version either; use it for TestFlight.

**409 / 4000 characters**

```text
noFriction is now on the Mac App Store.
• Bring your own AI: paste a key from OpenAI, Anthropic, Google Gemini and more, or use Apple's on-device model
• Delete, or Strike from the record: remove words, lines or screenshots everywhere
• Recordings stop when the meeting ends, with a 30-second banner to keep going
• Local transcription is the default
• One noFriction Pro subscription for Mac, iPhone and iPad
```

---

## 7. Subscriptions (App Store Connect → Monetization → Subscriptions)

Group reference name: `noFriction Pro`. Products:
`com.nofriction.meetings.pro.monthly`, `com.nofriction.meetings.pro.yearly`.
Prices and the introductory offer are the owner's call (see
LAUNCH_CHECKLIST.md §3). These display names and descriptions show on both
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
Information → Notes* (limit 4000). Replace `<OPENAI_TEST_KEY>`, `<NAME>`,
`<PHONE>`. Create the key as a separate OpenAI project key with a low monthly
budget; never put it in the app or the repo, and revoke it after review.

### iOS

**1791 / 4000 characters**

```text
No account or sign-in is needed. Everything is stored on the device; we run no servers.

RECORD
1. Record tab → tap Record. A one-time notice reminds the user that recording laws vary → "I understand".
2. Allow Microphone, Speech Recognition and Calendars. Speak for a minute; the transcript appears live (on-device speech recognition; it doesn't run in the Simulator).
3. Optional: Take Photo / Choose from Photos adds images to the meeting.
4. Tap Stop. The meeting is in the Meetings tab. Auto-stop: Settings → Recording.

PAYWALL (noFriction Pro, AI only)
5. Meetings → open the meeting → "Summarize" (or "Follow-up email"). Without Pro this opens the paywall: price, period, trial, Restore Purchases, Terms (Apple standard EULA) and Privacy Policy. Also: Settings → Subscription → Upgrade to Pro.
6. Buy with the sandbox account. Products: com.nofriction.meetings.pro.monthly and .pro.yearly (group "noFriction Pro").

AI (bring your own key)
AI runs on the user's own provider account. After subscribing, Summarize asks to set up AI:
• Paste this spending-capped test key: <OPENAI_TEST_KEY> (OpenAI is detected automatically), or
• On a device with Apple Intelligence (iOS 26+), tap "Use Apple on-device (no key)"; nothing leaves the device.
Before the first request to a cloud provider, a consent sheet names the provider and what is sent (transcript, title, attendee names, invite notes). Tap Allow. Notes and an email draft appear in the meeting.

EDITING
In a meeting, select transcript text → Delete (5-second undo) or "Strike from the record…" (permanent; leaves a marker).

Privacy: no analytics or tracking. Transcription is on-device. AI requests go directly from the device to the provider the user chose, using the user's key.
Contact: <NAME>, <PHONE>, support@nofriction.ai
```

### macOS

**1941 / 4000 characters**

```text
No account or sign-in is needed. Everything is stored on the Mac; we run no servers.

SETUP
1. First launch: the setup wizard. Choose "Private & Offline" transcription (downloads a 547 MB speech model once; needs internet for that one download).
2. Grant Microphone, Screen & System Audio Recording and Calendars when asked (System Settings → Privacy & Security).

RECORD
3. LIVE → Record. Speak or play a video call; the transcript appears live. Screenshots of the chosen screens appear in the timeline; "Snap" saves one now; "Change" picks displays/windows.
4. Stop. The meeting is in REWIND → RECORDINGS with transcript and screenshots. Auto-stop: Settings → General → Recording.

PAYWALL (noFriction Pro, AI only)
5. Settings → Subscription shows the plans, Restore Purchases, Terms (Apple standard EULA) and Privacy Policy. Any AI action without Pro (for example asking a question in CHAT) opens the same paywall.
6. Buy with the sandbox account. Products: com.nofriction.meetings.pro.monthly and .pro.yearly (group "noFriction Pro", shared with the iOS app via Universal Purchase).

AI (bring your own key)
• Settings → AI Engine → paste this spending-capped test key: <OPENAI_TEST_KEY>. OpenAI is detected and validated.
• Or, on macOS 26 with Apple Intelligence, the Apple on-device model is used automatically with no key; nothing leaves the Mac.
Before the first request to a cloud provider, a consent dialog names the provider and what is sent. Allow it.
Then: CHAT → ask "What did we decide?" The answer comes from the recorded transcripts. During a recording, LIVE shows AI insights.

EDITING
REWIND → a meeting → select transcript words or a screenshot → Delete (5-second undo) or Strike from the record (permanent; leaves a marker).

Privacy: no analytics or tracking. The app is sandboxed; AI requests go directly from the Mac to the user's chosen provider with the user's key.
Contact: <NAME>, <PHONE>, support@nofriction.ai
```

Optional attachment: a short screen recording of a recording → Summarize →
paywall → notes, if review asks how to reach a feature.

### TestFlight (external testing) Test Information

Beta app description (limit 4000):

**233 / 4000 characters**

```text
noFriction records and transcribes your meetings on your device, matches them to your calendar, and writes notes and follow-ups with the AI you choose (your own key, a local model or Apple's on-device model). No accounts, no servers.
```

What to Test (limit 4000):

**478 / 4000 characters**

```text
Please test:
• A real meeting from start to finish: recording, live transcript, calendar match and attendees
• Leaving the phone locked during a long meeting
• Auto-stop when the meeting ends (and "Keep recording")
• AI notes and follow-up email with your own key or Apple on-device
• Delete and Strike from the record
• Subscribing, restoring and the free trial (purchases are free in TestFlight)
Send feedback with a screenshot from TestFlight, or email support@nofriction.ai.
```

Feedback email: `support@nofriction.ai`. Beta review notes: the same as the
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
| 3 | Meetings list | Meetings tab | Every meeting, named from your calendar |
| 4 | People | People tab | Know who was there |
| 5 | Strike from the record marker | Meeting → select words → Strike | Strike it from the record |
| 6 | Settings → Connect AI | Settings tab (no key entered) | Your key. Your provider. |
| 7 (optional) | Paywall | Meeting → Summarize without Pro | Free to record. Pro for AI. |

Check before upload: the status bar shows a clean time (Simulator status bar
override `xcrun simctl status_bar booted override --time 9:41 --batteryLevel 100`),
no real key digits in Settings (the field must be empty), no real calendar
names.

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
   user. In the setup wizard choose "Private & Offline" and let the model
   download. Grant Microphone, Screen & System Audio Recording and Calendars.
6. Settings → Subscription: subscribe with a **Sandbox** Apple Account
   (TestFlight purchases are free). Settings → AI Engine: paste a test key
   (a capped project key). After the screenshots, delete the key and the
   Demo user.
7. Turn on Do Not Disturb; hide the Dock (System Settings → Desktop & Dock →
   Automatically hide); use the default wallpaper; set Appearance to Dark.
8. Display: set the built-in display to "Default" scaling (1440 × 900 points
   on 13" Retina gives 2880 × 1800 pixel captures) or use any 16:10 display.

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
| 3 | CHAT: ask "What did we decide about the cutover?" with the answer | Ask your meetings anything |
| 4 | LIVE → "LIVE INTELLIGENCE" cards during the recording (needs Pro + provider) | Action items as they happen |
| 5 | Settings → AI Engine with the provider list (key field empty, or showing only `••••` + last 4 of the throwaway key) | Bring your own AI |
| 6 | A transcript line struck from the record (marker visible) | Strike it from the record |
| 7 (optional) | REWIND → PEOPLE with invented attendees | Know who was there |

4. Stop the recording and let the auto-stop banner appear for an optional
   shot ("Meeting seems to have ended").
5. Check every image at 100% for real names, emails, notifications, menu-bar
   items and key characters before uploading.

---

## 10. Character count summary

| Field | Characters | Limit |
|---|---|---|
| Name | 25 | 30 |
| Name alt 1 | 19 | 30 |
| Name alt 2 | 28 | 30 |
| Subtitle | 29 | 30 |
| Subtitle alt | 30 | 30 |
| iOS promotional text | 153 | 170 |
| iOS description | 3345 | 4000 |
| iOS keywords | 95 | 100 |
| iOS What's New 1.0 | 365 | 4000 |
| Mac promotional text | 153 | 170 |
| Mac description | 3432 | 4000 |
| Mac keywords | 96 | 100 |
| Mac What's New 3.6.0 | 409 | 4000 |
| Group display name | 14 | 30 |
| Monthly display name | 11 | 30 |
| Monthly description | 38 | 45 |
| Yearly display name | 10 | 30 |
| Yearly description | 37 | 45 |
| iOS review notes | 1791 | 4000 |
| Mac review notes | 1941 | 4000 |
| Beta description | 233 | 4000 |
| What to Test | 478 | 4000 |
