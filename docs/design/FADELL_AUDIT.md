# noFriction: product design audit (Fadell lens)

Date: 2026-10-09. Scope: Mac (Tauri, `src/`), iOS (`ios/NoFriction`), Apple Watch
(`ios/NoFrictionWatch`), the site (`site/`) and the App Store listing
(`docs/APP_STORE_LISTING.md`). Read-only: nothing in product code changed.

How it was looked at: the Mac React app ran in the film harness
(`marketing/film/mac-harness`, mocked backend, demo data) at 1440×900; iOS
ran in a fresh iPhone 17 Pro simulator (iOS 26.2) with `-NFSeedDemo` /
`-NFResetOnboarding` / `-NFDemoLive`; the watch ran in an Apple Watch Series 11
simulator with every `-NFWatchDemo` state. Screenshots are in
`docs/design/audit/` and referenced by path. Counts come from the screenshots
and from `AgencyNavbar.tsx`, `FullSettings.tsx`, `SettingsView.swift`,
`tray_builder.rs`, `menu_builder.rs`, `CommandPalette.tsx`.

The lens: the product is the whole experience, not the screen. Every step
must be obviously necessary, every surface must earn its place, and the app
must be understandable in 90 seconds without a manual.

---

## 1. Verdict

noFriction is a private recorder with a memory: it records a meeting, a
class or a conversation, transcribes it on the device, lets you mark the
moments that matter and go back to any second of it, and (for Pro) turns the
transcript into notes, a review guide and answers. It is excellent at the
core loop on iPhone and Watch: one red button, three honest questions, a
live transcript, a mark button, and a recording that lands in a list with
its notes; the privacy promise is stated in plain words on the screen where
it matters, and Delete/Strike is the most carefully designed destructive
flow I have seen in a note app. It is confused on the Mac, where a second,
older product ("command center": INTEL, VAULT, ZEN, PROMPTS, GENIE, Knowledge
Base, Activity Insights, personas, VLM frame analysis) still surrounds the
new one and competes with it for the top of the window, so a new user sees
eleven destinations before they see a recording. It is also confused about
its own nouns: the same thing is a recording, a meeting, a capture, a
session and a file depending on which screen, menu or platform you are on,
and the same act is "Summarize", "Generate notes", "Write notes" and "Redo
notes". Cut the old product, keep one vocabulary, make the Mac as single-minded
as the phone, and this is a product that explains itself.

---

## 2. The 90-second test

What a new user understands after 90 seconds, from the screenshots, honestly.

**iPhone** (`audit/ios-34-onb-1-welcome.png` → `ios-38-onb-5-pro.png` →
`ios-39-after-onboarding.png`). They understand: this records, it transcribes
on the phone, nothing leaves, there is a red button, AI is extra and I can
choose it later. The welcome screen says it in one sentence and four lines,
and the Record tab repeats the promise in one box. The weak spots in 90
seconds: five onboarding screens before the first button (and the "Before
you record" warning appears twice if you skip onboarding), the Permissions
screen asks for Notifications before the user knows what a timed recording
is, and "Set up AI" presents a Base URL / Model ID / API key form to someone
who has not yet recorded anything. The tab bar is honest (Record, Recordings,
Chat, People, Settings) but "People" is a fifth tab for a feature most
personal and class users will never fill.

**Mac** (`audit/mac-01-live-idle.png`). They understand: there is a big
"Start recording" button and a privacy line. They do not understand: why
there are two start buttons (START CAPTURE in the toolbar, Start recording in
the body), what REWIND / INTEL / CHAT / MORE mean before anything is
recorded, what "LIVE INTELLIGENCE · IDLE" is, why a "Main display · Change"
chooser and a "Snap" button sit above an empty transcript, what GENIE is, or
what "⌘K" does. The setup wizard (`SetupWizard.tsx`, six steps, model
download, AI form, Pro pitch) explains rather than does. The vocabulary on
this one screen alone: capture, recording, display, snap, mark, live,
rewind, intel, chat, more.

**Watch** (`audit/watch-idle.png`, `watch-01-start.png`,
`watch-recording.png`). They understand everything. A red Record button, one
line that says where the transcript happens, a Start button that carries
the remembered choices, and a recording screen with the time, Mark, Pause
and Stop. This is the 90-second bar the other two platforms should be held
to.

---

## 3. Findings, ranked by impact

Severity: **blocks** (a user cannot get to value, or gets the wrong idea),
**hurts** (slows or confuses a real task), **nags** (noise, a word, a
pixel). Recommendation verbs: cut / merge / move / rename / default.

### Top 10

**F-1 · Mac · blocks · Two products share one window.** The top bar has
LIVE / REWIND / INTEL / CHAT / MORE, and MORE opens VAULT, ZEN, PROMPTS,
HELP; REWIND has its own RECORDINGS / PEOPLE / INSIGHTS / SEARCH tabs; INTEL
has OVERVIEW / GRAPH / TIMELINE / PEOPLE / TOPICS / SEARCH. INTEL shows
"0 PEOPLE · 0 TOPICS · 0 CONNECTIONS", "TOP TAG —", "TOP HUB —" and "No
graph data yet. Export recordings to your Obsidian vault first" with seven
recordings in the library. PROMPTS shows "Master prompts powering Genie,
Reports, Catch-Up, and Live Intelligence — customize per persona". None of
this is in the positioning, the user guide, the store description or the
iOS app. *Why it matters:* a new user cannot tell what the product is;
the recordings library (the product) is one of eleven places to go.
*Evidence:* `audit/mac-18-intel.png`, `audit/mac-19-more-menu.png`,
`audit/mac-20-vault.png`, `audit/mac-22-prompts.png`,
`audit/mac-14-rewind-insights.png`. *Recommendation:* **cut** INTEL, VAULT
(as a mode), PROMPTS, ZEN, GENIE, Activity Insights, Knowledge Base search
and the "SYSTEM / Close Overlay / Admin Console" chrome. Top bar becomes
Record · Recordings · Chat, plus the gear. Obsidian export stays as a
Settings row.

**F-2 · Mac + iOS · blocks · The same act has four names and the same
object has five.** Notes: "Summarize" (iOS button), "Redo notes" (iOS after),
"Generate notes" / "Regenerate" (Mac panel), "Write notes after each
recording" (Mac setting), "AI notes" (both captions), "Meeting notes /
Lecture notes" (docs). Object: "Recordings" (tabs, iOS), "meeting"
(`meetings` table, "Delete Meeting" in the user guide, `Section("Meetings")`
in iOS People), "capture" (START CAPTURE / STOP CAPTURE / "Capture Mode" in
the tray / "Captures" panel / "Ready to capture" in ZEN), "session"
(store copy), "Recordings · Total Files" (Mac delete dialog). *Why it
matters:* the user learns the product by its nouns; every synonym is a
small "is this the same thing?" *Evidence:* `audit/ios-19-meeting-detail-1.png`
("Summarize"), `audit/mac-10-notes.png` ("Find again"), `audit/mac-27-settings-ai-engine.png`
("Write notes"), `audit/mac-01-live-idle.png` ("START CAPTURE" and "Start
recording" on one screen), `audit/mac-35-delete-confirm.png`. *Recommendation:*
**rename** to one set (§5): *recording*, *Record / Stop*, *Notes* with one
verb *Make notes* (and *Make again*), *Mark*, *Review guide*.

**F-3 · Mac · hurts · The idle Record screen is a cockpit.** Before anything
is recorded the screen shows a capture bar ("Not recording · Main display ·
Change · Mark · Snap"), a CAPTURES panel ("Screens you capture appear
here"), a LIVE INTELLIGENCE panel with an IDLE badge and a dotted "Action
items, decisions and risks surface here while you record" card, two start
buttons and a ⌘K search. *Why it matters:* the one decision the user has to
make (press Record) is surrounded by nine things that do nothing yet.
*Evidence:* `audit/mac-01-live-idle.png`. *Recommendation:* **default**
the idle state to the iOS layout: a title, one line of promise, one button.
Show the capture bar, captures and insights only while recording.

**F-4 · Mac · hurts · Six setup screens before the first recording, and
the first is a lecture.** Welcome (four facts in a table), macOS
permissions, model download (547 MB; "you can continue" but transcription
will not work), Optional AI (the full AI settings panel inline), noFriction
Pro (a feature table and "Nothing to buy now"), You're all set (six
keyboard shortcuts). *Why it matters:* the user came to record; every
screen that is not "Allow microphone" and "Download" is reading. *Evidence:*
`src/features/onboarding/SetupWizard.tsx` lines 100–130, 480–560 (the
harness cannot show the wizard; see `audit/mac-31-settings-help.png` for
the same material repeated in Help). *Recommendation:* **merge** to two
screens: (1) Microphone + Screen & System Audio + Calendar with one Allow
each, and the model download started automatically in the background with
a progress line; (2) the Record screen. AI and Pro appear the first time
the user presses Make notes (they already do: the paywall and "Set up AI"
sheet exist on both platforms).

**F-5 · iOS + Mac · hurts · The Pro pitch and the AI form are shown
before the user has anything to summarize.** iOS onboarding step 4 is "Set
up AI" with Base URL / Model ID / API key and a "Use Apple on-device (no
key)" button under it; step 5 is "Free to record. Pro for AI." Mac steps 4
and 5 are the same. The same form is reachable again from a recording's
Summarize (`AISetupSheet`) and from Settings. *Why it matters:* a form the
user cannot evaluate (what endpoint?) and a price before value are the two
classic reasons to abandon onboarding. *Evidence:* `audit/ios-37-onb-4-ai.png`,
`audit/ios-38-onb-5-pro.png`. *Recommendation:* **cut** both steps from
onboarding on both platforms. **Default** Apple on-device when it exists
(the code already does). Keep the in-context sheet.

**F-6 · iOS + Watch · nags → hurts on first run · "Before you record" is
asked twice, and the consent copy is in three places.** Onboarding step 2
("Before you record… I understand") sets `recordingNoticeAccepted`; the
Record button shows the same sheet if the flag is not set (skip onboarding
and you get it on first Record; `LiveView.swift:183`). The Record screen
then shows "Let everyone know you're recording." permanently, idle and
recording. The watch shows its own "Before you record" once. *Why it
matters:* one legal reminder once is respectful; the same sentence on three
surfaces is nagging and dilutes the one that counts. *Evidence:*
`audit/ios-35-onb-2-before-you-record.png`, `audit/ios-03-record-notice.png`,
`audit/ios-02-record-idle.png` (bottom line), `audit/ios-40-live-class.png`
(above the mark chips). *Recommendation:* **merge** into one: the sheet on
the first Record only (not an onboarding page), no permanent caption. Keep
the watch's once.

**F-7 · Mac · hurts · Four different search boxes, none of them the
obvious one.** REWIND → SEARCH ("Knowledge Base Search", placeholder
"Search your knowledge base…"), INTEL's "Search across all recordings…" bar
and its own SEARCH tab, ⌘K ("Type a command or search…", lists recent
recordings), and the Recordings library has no search field at all (the
`RecordingsLibrary` "Search recordings…" input lives in Settings → Data
when storage stats load). iOS has one `.searchable` field on Recordings
with the right prompt: "Titles, people, topics, or anything said". *Why it
matters:* "find what was said last week" is the second job of the product,
and on the Mac it has no home. *Evidence:* `audit/mac-15-rewind-search.png`,
`audit/mac-18-intel.png`, `audit/mac-32-command-palette.png`,
`audit/mac-07-rewind-list.png` (no field). *Recommendation:* **merge** into
one search field at the top of Recordings, with the iOS prompt; keep ⌘K as
a shortcut to it; **cut** the other three.

**F-8 · Mac · hurts · The transcript editing toolbar is a spreadsheet of
verbs.** Under the transcript: "Select all · From here to the end · Last
[12] min · Select · Edit words"; under the screens: "Select screens · Select
all · From here to the end · Last [12] min · Select · Time range…"; the
Markers card adds "Mark 0:10" and per-marker kind chips plus a bin; a
selection adds "Linked ✓ · Edit words · Time range… · Delete · Strike from
the record… · ×" and the status line "0 screens · 1 line · 09:02:15–09:02:19";
plus title tooltips ("⌘A in the transcript", "Or ⌘-click thumbnails; Shift-click
picks a range"). iOS does the same job with one "Select" and an "Edit line"
sheet with two buttons. *Why it matters:* the Delete/Strike promise is the
product's best idea; it should feel like a scalpel, not a console.
*Evidence:* `audit/mac-09-rewind-detail.png`, `audit/mac-36-transcript-selected.png`
vs `audit/ios-57-word-editor.png`. *Recommendation:* **merge**: click a line
to select, drag for a range, ⌘A for all; one bar with Delete and Strike…;
"Last N min", "From here to the end" and "Time range…" become one "Select
time…" item in a menu; move the Markers list into the timeline (the pins
already exist). **Cut** "Linked" as a visible state.

**F-9 · Mac + iOS · hurts · Settings that should be decisions.** Mac
General: "Silence before stopping" (a free number field, 10 vs the iOS
stepper's 3), "Capture system audio", "Capture screenshots" (each one
breaks a documented feature when off), "Run setup assistant again"; AI
Engine: "Live insights while recording" and "Write notes after each
recording" as separate toggles (so auto-notes is off by default in the
harness and a user who never finds the toggle never gets notes without
clicking); Obsidian: "Auto-analyze frames" and "Processing interval" (VLM)
in `KnowledgeBaseSettings.tsx`; Data: "Permissions Diagnostics… Test All",
"Clear Cache". iOS: "After silence of N min" stepper. *Why it matters:* a
setting is a decision the product declined to make; most of these the user
cannot make better than the product. *Evidence:* `audit/mac-24-settings-general.png`,
`audit/mac-27-settings-ai-engine.png`, `audit/mac-29-settings-data.png`,
`audit/ios-53-settings-2.png`. *Recommendation:* **default** silence to one
value (3 min) with no field; **cut** the capture toggles (the permission
state is the toggle); **merge** the two Automatic AI switches into "Make
notes automatically" on by default once AI is set up; **cut** VLM settings,
Permissions Diagnostics, Clear Cache, Run setup assistant.

**F-10 · Mac · hurts · "Live Intelligence" and GENIE are a second AI
product during recording.** While recording, the right column streams KEY
POINT / TOPIC SHIFT / ACTION ITEM / DECISION cards, each with a timestamp,
and GENIE shrinks the app to an overlay that shows the same stream plus an
"AI INSIGHT" strip. The user is in a meeting. *Why it matters:* reading AI
cards during a conversation is the opposite of "stay in the moment" (the
store's own words), and it doubles the Pro surface with a second name.
*Evidence:* `audit/mac-03-live-recording.png`, `audit/mac-04-live-mark.png`,
`audit/mac-05-genie.png`. *Recommendation:* **cut** live insights and GENIE.
If a floating window is wanted, it is the capture bar (time left, Mark,
Stop) and nothing else.

### 11–30

**F-11 · Mac · hurts · START CAPTURE and Start recording are the same
button twice, and STOP CAPTURE / ■ STOP / Stop now are three stops.**
`audit/mac-01-live-idle.png`, `audit/mac-05-genie.png`,
`audit/mac-39-meeting-end-banner.png`. **Merge**: one Record/Stop in the
toolbar; the body shows state, not a second button.

**F-12 · Mac · hurts · Group-by and topic chips make every recording
appear twice.** "Group by Topic" lists "Acme project sync" under Q4 launch
and again under Pilot, and a "No topics" group holds the rest; rows show
type chip + notebook chip + up to two topic chips + the group header.
`audit/mac-08-rewind-group-topic.png`, `audit/ios-09-recordings-by-topic.png`.
**Cut** Group by Topic (and Group by on iOS; the menu icon in the corner is
unlabeled); keep Notebook chips as the one filter, topics as a search
facet.

**F-13 · Mac · nags · The Markers card hides the transcript.** Expanded by
default, it takes the top third of the transcript column with a filter row,
a "Mark 0:10" button and per-row kind chips (`audit/mac-09-rewind-detail.png`;
the film script collapses it before every shot, `capture.mjs rewindReady`).
**Move** marks to the timeline pins and inline chips they already have;
collapse the list by default.

**F-14 · Mac · hurts · The Mac has no visible way to add a photo or
play audio.** Rewind shows frames and a scrubber; there is no play button
for the audio, and the "Snap" button is a screenshot, not a photo. iOS has
"Take Photo", "Choose from Photos" and a playback control. `audit/mac-09-rewind-detail.png`
vs `audit/ios-13-detail-3.png`. **Default** a play control on the Mac
scrubber; rename Snap to "Capture screen".

**F-15 · Mac · hurts · The REWIND → PEOPLE tab is empty with seven
meetings in the library, and offers "No LinkedIn yet" and "Re-sync
calendar".** `audit/mac-13-rewind-people.png`. The People tab competes with
INTEL → PEOPLE and with the attendees already shown on each recording. **Cut**
the tab; people live on the recording (as on iOS) and in search.

**F-16 · iOS · hurts · People is a fifth tab whose empty state is a
calendar pitch, and LinkedIn is its main verb.** `audit/ios-50-people.png`:
"Connect your calendar… so you can link each person's LinkedIn", a "No
LinkedIn yet" filter switch, "+ LinkedIn" on every row. For classes and
personal recordings the tab is permanently empty. **Move** People under
Recordings (a section, like Notebooks) and **cut** the LinkedIn filter;
keep the link on a person's page.

**F-17 · iOS · nags · "Connect your calendar" card is shown on
Recordings and People at the same time, after Permissions already asked.**
`audit/ios-07-recordings.png`, `audit/ios-50-people.png`. **Merge** into
one, on Recordings only, dismissed once.

**F-18 · iOS · hurts · The Record sheet opens at half height and hides
the Start button under the fold on first open.** `audit/ios-04-record-sheet-half.png`:
the user sees "What is it?" and must know to pull up for "How long?",
Notebook and the red button (the film script has an `expandRecordSheet`
helper for exactly this). **Default** the sheet to full height, or put Start
in the half-sheet.

**F-19 · iOS + Mac + Watch · hurts · "How long?" asks a question the
app can answer.** Five choices on every start (15/30/60/90/∞), remembered,
with a two-line explanation; a meeting matched to a calendar event already
knows its end; the meeting-end detector already stops on silence. The watch
makes it a second screen. `audit/ios-05-record-sheet-full.png`,
`audit/mac-02-record-sheet.png`, `audit/watch-length.png`. **Default**: no
limit unless the user taps "Stop at…" (one chip that opens the five
choices); the calendar end and silence stop cover the real cases.

**F-20 · All · nags · The type picker's help line explains the third
option.** "Personal covers everything else: conversations, appointments,
talks, ideas." on iOS, Mac and the watch, plus "Notes for a class are written
as lecture notes." under the Notebook field. **Rename** the third option to
"Other" (or "Life", matching the site's "Everyday life") and **cut** both
help lines.

**F-21 · Mac · nags · Recording type and notebook are a `<select>` and
a text field in the recording header, labeled "Type" and "Notebook".**
`audit/mac-09-rewind-detail.png`. iOS shows them as two tappable rows with
icons. **Move** them into the title block the way iOS does, without
labels.

**F-22 · Mac · hurts · The tray menu is 17 items with three ways to
start.** "Start Recording (Meeting, 60 min)", "Start Recording For ▸ 15/30/60/90/No
Limit", "Capture Mode ▸ Ambient (Background) / Meeting (Full Capture) /
Paused", "Stop When Meeting Ends" checkbox, "Auto-stop: on" status line,
"Keep Recording (cancel auto-stop)", "Time limit: not recording", "Add 15
Minutes", "Remove Time Limit", "Show Window", "Activity Insights",
"Knowledge Base", "Settings…", "Quit" (`src-tauri/src/tray_builder.rs`
128–208). "Ambient" and "Meeting (Full Capture)" are capture modes that the
app's own `App.tsx` handlers treat as plain start/resume. **Cut** to: Start
Recording / Stop, Pause, Mark Moment, Open noFriction, Settings…, Quit.

**F-23 · Mac · nags · The menu bar's View menu says "Recordings" and
"Chat with Your Recordings" while the window says REWIND and CHAT, and
"Prompts" has a menu item but PROMPTS is under MORE.** `src-tauri/src/menu_builder.rs`
91–119. The command palette says "Go to Recordings ⌘2" for REWIND and has
an "Open Prompts ⇧⌘P" entry. **Rename** REWIND to Recordings everywhere
(Rewind is the view inside a recording, which the user guide already says);
**cut** Prompts.

**F-24 · Mac · nags · ALL-CAPS tracked labels (LIVE, REWIND, START
CAPTURE, CHAT WITH YOUR RECORDINGS, SEARCHED ON THIS MAC · THIS RECORDING ·
7 RECORDINGS, ASK ABOUT, LIVE INTELLIGENCE, CAPTURES, SYSTEM) read as a
dashboard, not a notebook.** `audit/mac-16-chat-empty.png`. The iOS app uses
sentence case throughout and is calmer for it. **Rename** to sentence case;
reserve caps for nothing.

**F-25 · Mac · nags · Emoji icons in Settings and the command palette
(⚙️ 🎙️ 📚 ✨ ⭐ 💾 ℹ️ ❓ 🔍 💡 📊 🗑️) against DESIGN.md's own rule "Never use
emoji as UI icons".** `audit/mac-24-settings-general.png`,
`audit/mac-32-command-palette.png`, `audit/mac-14-rewind-insights.png`.
**Cut** the emoji; use `icons.tsx`.

**F-26 · Mac · hurts · The Settings modal has a modal inside it:
"SYSTEM" sidebar (Settings / Help & Docs / Close Overlay), then a Settings
sidebar of seven categories, then the panel.** `audit/mac-24-settings-general.png`.
Help & Docs duplicates MORE → HELP (two different help documents, with
different content: `HelpSection` vs `HelpView`). **Merge** to one Settings
window with five sections (Recording, Transcription, AI, Subscription,
About), one Help.

**F-27 · Mac · hurts · "Obsidian" is a top-level settings category and
a top-level view (VAULT), and INTEL's empty state tells the user to export
to Obsidian first.** A knowledge graph of exported markdown is a different
product. **Move** Obsidian to one row in Settings → Data ("Export to
Obsidian: folder, auto-export"); **cut** VAULT, VaultGraph, VaultTags,
BacklinksPanel.

**F-28 · iOS · nags · Lecture notes render raw Markdown ("## Summary",
"- Glycolysis…", "**Final electron acceptor**").** `audit/ios-11-detail-1.png`;
meeting notes on the same screen are formatted (`audit/ios-19-meeting-detail-1.png`).
**Default** the same renderer for every type.

**F-29 · iOS + Mac · nags · The paywall promises less than Pro does.**
iOS paywall bullets: "Summaries, decisions and action items", "Follow-up
email drafts", "Apple on-device or your own AI endpoint and model";
onboarding step 5 lists the same two. Review guides, topics and Chat
(the features most likely to sell) are missing from both. `audit/ios-21-paywall.png`,
`audit/ios-38-onb-5-pro.png`. **Rename** the three bullets to Notes, Review
guide, Chat.

**F-30 · Mac · nags · "Find again" / "Find topics" / "edit" / "Edit"
appear as four controls for one topic list, and the Notes view shows Topics
above the notes it was derived from.** `audit/mac-10-notes.png`. **Merge**
to one "Edit" that includes "Find again"; move Topics below the notes.

**F-31 · Mac · hurts · Meeting-end banner truncates its own sentence
and shows "stopping in NaNs" when the countdown is not a number; the
time-limit warning overlaps the top bar.** `audit/mac-39-meeting-end-banner.png`,
`audit/mac-40-time-limit-warning.png` (harness-emitted payloads; the shape
is the component's, the data is mocked, so the NaN is the mock's fault, the
truncation and overlap are not). **Default** both to one banner slot under
the toolbar with room for a full sentence.

**F-32 · Watch · nags · The record flow's first screen puts the Start
button above the question it answers, and the subtitle "Meeting · No limit"
is the only hint that the lists below change it.** `audit/watch-01-start.png`.
This is still the best first screen in the product. **Rename** the Start
row to "Start · Meeting · No limit" with a "change" hint, or move it below
the three lists.

**F-33 · Mac · nags · The delete confirmation is a browser `confirm()`
with "Delete this recording? Its transcript, screenshots and AI notes are
removed from this Mac. This can't be undone." while word deletion has a
5-second undo toast.** `MeetingHistory.tsx:137`, `audit/mac-38-delete-undo-toast.png`.
Two deletions, two safety models. **Default** both to the undo toast.

**F-34 · Site + Store · nags · The store name "noFriction: Record your
Life" and subtitle "Transcribe, summarize, rewind" use "summarize"; the
apps never do (Summarize is the iOS button, Notes is the thing).** Pick
one (Notes) and use it on the site's "Notes and review" step too.

**F-35 · Mac · nags · Help in the app is three documents: MORE → HELP
(tabs: Help Guide, How-To, How It Works, Privacy & Security, Services),
Settings → Help & Docs (Getting Started, Shortcuts, Troubleshooting), and
the wizard's Done step shortcut list.** `audit/mac-23-help.png`,
`audit/mac-31-settings-help.png`. **Merge** into one Help, opened from the
Help menu, that is `docs/USER_GUIDE.md`.

---

## 4. Cut list

Each item, with the reason a user would give.

| Cut | Platform | User-facing reason |
|---|---|---|
| INTEL view (Overview, Graph, Timeline, People, Topics, Search) | Mac | "It shows zeros and tells me to export to Obsidian first. I have seven recordings." |
| VAULT view, Vault graph, tags, backlinks | Mac | "I don't use Obsidian. Why is it a tab?" |
| PROMPTS (Prompt Studio, personas) | Mac | "I don't want to edit the AI's instructions. I want notes." |
| ZEN | Mac | "It's the Record screen with fewer words. Make the Record screen that." |
| GENIE mode and Live Intelligence stream | Mac | "I'm in a meeting. I don't want AI cards scrolling next to the person talking." |
| Activity Insights (REWIND → INSIGHTS) | Mac | "Total hours and a duration bar chart about myself is not why I opened this." |
| Knowledge Base Search (REWIND → SEARCH), INTEL search, palette search | Mac | "Four search boxes. Give me one, on the list of recordings." |
| REWIND → PEOPLE tab | Mac | "The people are already on the meeting." |
| Tray: Capture Mode (Ambient / Meeting / Paused), Start Recording For ▸, status lines, Activity Insights, Knowledge Base | Mac | "I need Start, Stop, Mark." |
| Settings: Obsidian category (keep one export row), Data → Permissions Diagnostics / Test All / Clear Cache, General → Run setup assistant, capture toggles, silence field, VLM frame settings | Mac | "These are things the app should know." |
| Settings modal chrome: SYSTEM sidebar, Close Overlay, Help & Docs tab, Admin Console | Mac | "Settings inside a settings inside a window." |
| Setup wizard steps: Welcome table, Optional AI, noFriction Pro, You're all set | Mac | "I'll find AI when I need it. Let me record." |
| Onboarding steps: Before you record (as a page), Set up AI, Free/Pro | iOS | "Ask me once, when it matters." |
| Permanent "Let everyone know you're recording." caption | iOS | "You told me. Twice." |
| Group by (Date / Notebook / Topic) menu; topic chips on rows | iOS, Mac | "The same recording three times on one list." |
| "No LinkedIn yet" filter; LinkedIn as the row action | iOS | "LinkedIn isn't what People is for." |
| Second "Connect your calendar" card (People) | iOS | "I saw this on the last tab." |
| Notifications in the onboarding Permissions list | iOS | "Ask when a timer is about to end, not before I've recorded." |
| "How long?" as a required second question (keep as an optional chip) | iOS, Mac, Watch | "I don't know. Stop when it's over." |
| Words: capture, session, intel, insights, vault, genie, zen, knowledge base, persona, VLM, engine, overlay, "Rewind" as a top-level label | All | See §5. |

---

## 5. Vocabulary table

Every term found per concept, and the one to keep.

| Concept | Mac | iOS | Watch | Site | Store | Keep |
|---|---|---|---|---|---|---|
| The thing you made | Recording, meeting (`meetings`, "Delete this recording"), capture ("START CAPTURE", "Captures"), file ("Total Files"), REWIND (nav) | Recording, Meeting ("Delete Meeting" in guide, `Section("Meetings")`), session (notice copy) | Recording | recording | recording, session | **recording** |
| Start / stop | START CAPTURE, Start recording, Start Recording (tray/menu), START (Zen), STOP CAPTURE, ■ STOP (Genie), Stop now, Stop Recording | Tap to record, Start recording, Stop | Record, Start, Stop recording | Record | Record | **Record / Stop** |
| The library | REWIND, Recordings, Recordings library, Knowledge Base, Rewind | Recordings | Recordings | Rewind (feature) | Rewind | **Recordings** (list); **Rewind** only for the inside-a-recording timeline |
| AI notes | Generate notes, Regenerate, Write notes after each recording, AI notes, Notes, Meeting notes, Lecture notes, Reports (Prompt Studio), Catch-Up | Summarize, Redo notes, AI notes, Notes, Regenerate | — | Notes and review | summarize, notes | **Notes**; verb **Make notes** / **Make again** |
| Live AI during recording | Live Intelligence, Live insights, AI INSIGHT, Genie | — | — | — | — | **cut** |
| Marks | Mark, Mark this moment, Marker(s), Markers (3), moment, pins, kind chips | Mark this moment, Marked moments, marker | Mark | Mark what matters, Mark this moment | Mark this moment | **Mark** (verb), **Marks** (list) |
| Third mark | Follow up / On the test / Remember | same | same | same | same | keep (consistent) |
| The grouping | Notebook, Notebooks, class_name (code), course | Notebook, Notebooks, courseName (code) | Notebook | Notebooks | Notebooks | **Notebook** |
| Type | What is it? Meeting · Class · Personal; Type (select) | What is it? Meeting · Class · Personal | What is it? | Meetings / Classes / Everyday life | Meeting, Class or Personal | **Meeting · Class · Other** (or Life); drop the "What is it?" title |
| Review material | REVIEW, Review guide, Study guide, STUDY (old), study_materials (code) | Review, Review guide, Study guide | — | Notes and review | review guide, study guide | **Review guide** (Study guide for Class is fine) |
| Topics | Topics, Find topics, Find again, Edit, edit, Top Tag, Tag Cloud, Tagged | Topics, Find topics, Add, Rename, Remove | — | — | — | **Topics**; one **Edit** |
| Links | LINKS, Links & References, reference, Said / On screen / Added | Links, reference | — | Links | links, references | **Links** |
| Screens | screenshots, frames, screens, captures, Snap, Main display, Frame preview, video | Photos, Snap, Take Photo | — | screens | photos, screenshots | **Screens** (Mac), **Photos** (iOS); verb **Capture screen** |
| Editing | Delete, Strike from the record, Stricken from the record, redaction (code), Edit words, Time range | Delete, Strike from the record, Edit line, Edit words | — | Edit, delete, or strike from the record | same | keep |
| Chat | CHAT, Chat with Your Recordings, Ask your recordings, Ask about, Chats, New chat, threads (code) | Chat, Chats, New chat, Ask about your recordings | — | — | chat | **Chat** |
| AI source | AI Engine, AI provider, Saved providers, Local & custom servers, endpoint, connection, Models, Automatic AI | AI provider, Your AI endpoint, Saved connections, Active | — | AI is yours to choose | AI you choose | **AI** (section), **endpoint** (the URL), **connection** (saved) |
| Transcription | Transcription, Local Whisper (Offline), speech model, ggml-small.en.bin, "transcribing on this Mac" | Speech recognition, transcribed on this device | Transcribed on your iPhone | Transcribed on your device | on-device transcription | **Transcription** (on this Mac / iPhone) |
| Auto-stop | Stop automatically when the meeting ends, Auto-stop, Stop When Meeting Ends, meeting-end, Keep recording, Silence before stopping | Stop automatically when the meeting ends, After silence of | — | Timed recording | stops when the meeting ends | **Stops when it's over** (one line) |
| Time limit | How long?, No limit, time limit, Timed recording, planned length, +15 min, Add 15 Minutes, Remove Time Limit | How long?, left, Stops at | How long?, left | Timed recording | how long to record | **Stop at…** / **+15 min** / **No limit** |
| Settings | Settings, Settings…, SYSTEM, Overlay, Admin Console, General, Data | Settings | — | — | — | **Settings** |
| Help | HELP, Help & Docs, noFriction Documentation, Help & Documentation, Help Guide, How-To, Troubleshooting | — | — | Support | — | **Help** |
| Privacy line | Works offline · Nothing leaves this Mac unless you want it to; What leaves this device; Nothing leaves this Mac | Works offline. Stays on this iPhone; Nothing leaves this iPhone unless you want it to; What leaves this device | Transcribed on your iPhone | No servers. No account. | Recordings stay on your device | **Nothing leaves this [device] unless you want it to** (once per screen at most) |

Distinct nouns for "the thing you made": **6** (recording, meeting, capture,
session, file, rewind). For "make notes": **7** verbs.

---

## 6. Flow maps

Taps are counted from a cold first launch (permissions as separate system
taps where iOS/macOS forces them), then for a returning user.

### A. Launch → first recording

**iOS, today (first run):** Continue (1) → I understand (2) → Allow mic (3)
+ system (4) → Allow speech (5) + system (6) → [Calendar, Notifications
optional] → Continue (7) → Skip for now (8) → Get started (9) → red button
(10) → Start recording (11). If onboarding is skipped: Skip (1) → red button
(2) → I understand (3) → Start recording (4). Returning: red (1) → Start
recording (2). *5 onboarding screens, 2 modal interruptions (recording
notice, Record sheet), 11 taps.*

**iOS, proposed:** Allow mic (1) + system (2) → Allow speech (3) + system
(4) → red button (5) → I understand (6, once) → Record (7). Returning: red
(1). *1 screen, 1 interruption (once), 7 taps first run, 1 returning.* The
type/length sheet becomes optional chips on the Record screen (Meeting ·
Class · Other, "Stop at…"), remembered.

**Mac, today:** Get Started (1) → Allow mic (2) + system (3) → Allow screen
(4) + system (5) + relaunch → Continue (6) → Download (7) + wait → Continue
(8) → Skip for now (9) → Continue (10) → Done (11) → START CAPTURE (12) →
Start recording (13). Returning: START CAPTURE (1) → Start recording (2), or
⌘N (0). *6 wizard screens, 13 taps.*

**Mac, proposed:** Allow mic (1) + system (2) → Allow screen (3) + system
(4) → Record (5), with the model downloading in the background and a one-line
"Transcription will start when the model finishes (2 min)". Returning:
Record (1) or ⌘N. *1 screen, 5 taps.*

**Watch, today and proposed:** Record (1) → I understand (2, once) → Start
(3). Returning: Record (1) → Start (2). Fine as is.

### B. Recording → notes

**iOS, today:** Stop (1) → Recordings tab (2) → recording (3) → scroll →
Summarize (4) → [first time: paywall → Subscribe, or Set up AI sheet, or
consent] → notes. *4 taps + up to 3 first-time sheets.*

**iOS, proposed:** Stop (1) → the recording opens itself with "Making
notes…" (if Pro and AI set) or one "Make notes" button at the top (2). *1–2
taps.* First-time Pro/AI sheets stay, but only here.

**Mac, today:** STOP CAPTURE (1) → REWIND (2) → recording (3) → NOTES (4) →
Generate notes (5), unless "Write notes after each recording" is on and the
recording is over six minutes. *5 taps.*

**Mac, proposed:** Stop (1) → the recording opens on Notes with "Making
notes…" (auto on by default once AI is set), else Make notes (2). *1–2 taps.*

### C. Find something said last week

**iOS, today:** Recordings (1) → pull to reveal search (2) → type (3) →
result (4) → scroll the transcript to the line. *4 taps + scroll; good.*

**Mac, today:** REWIND (1) → scan the list by date (no search) → recording
(2) → scroll the transcript; or REWIND → SEARCH (2) → type (3) → result; or
INTEL (1) → search bar (2); or ⌘K (1) → type (2) → recording (3, lands on
the recording, not the line); or CHAT (1) → scope (2) → type (3) → citation
(4). *Five routes, 3–4 taps each, none lands on the line.*

**Mac, proposed:** Recordings (1) → search field at the top (2) → type (3)
→ result opens the recording at the line (4). Chat stays for questions, not
lookup. ⌘K focuses the same field.

---

## 7. Counts (before)

| Measure | Mac | iOS | Watch |
|---|---|---|---|
| Taps, cold launch → first recording | 13 (6 wizard screens) | 11 (5 onboarding screens; 4 if onboarding skipped) | 3 |
| Taps, returning → recording | 2 (or ⌘N) | 2 | 2 |
| Taps, finished recording → notes | 5 (2 with auto-notes on, off by default) | 4 (+ up to 3 first-time sheets) | n/a (phone) |
| Taps, find something said last week | 3–4, five different routes, none lands on the line | 4, one route | n/a |
| Top-level navigation items | 4 primary + MORE (4) + ⌘K + START CAPTURE + gear = **11**; REWIND adds 4 sub-tabs, INTEL adds 6, a recording adds 4 | **5** tabs | 2 pages |
| Settings categories / controls | 7 categories (+ Help & Docs, + Admin Console when owner) / **~27** user-facing controls (General 6, Transcription 1, Obsidian 2 + 2 VLM, AI Engine 9, Subscription 1, Data 5, About 3) | 7 sections / **~11** controls (AI 5, Recording 2, Consent 1, Subscription 2, About 1 action) | 1 (Discreet) |
| Tray menu items | 17 (3 submenus) | — | — |
| Menu bar items (custom) | 11 across App, File, View, Help | — | — |
| Command palette entries | 8 fixed + recent recordings | — | — |
| Modal interruptions on first run | 6 wizard + 2–3 system permission dialogs + Record sheet = **9–10** | 5 onboarding + 2–4 system dialogs + recording notice + Record sheet = **9–11** | 1 notice + 1 system mic = 2 |
| Distinct nouns for "the thing you made" | 6 | 3 | 1 |
| Distinct verbs for "make notes" | 5 | 3 | — |
| Search entry points | 4 | 1 | — |
| Help documents | 3 | 0 (none needed) | 0 |
| Screens that say the privacy promise | 4 (Record idle, Record live footer, AI Engine, About) | 4 (Record, onboarding welcome, Permissions, Settings) | 1 |

---

## 8. What not to touch

- **The red button and the Record screen on iOS** (`audit/ios-02-record-idle.png`).
  One title, one promise, one button; Pause and Snap appear only when they
  can do something. Keep.
- **The three questions, when asked at all.** "Meeting · Class · Personal",
  a length, a notebook: the words are the same on Mac, iPhone and Watch
  (`RecordingVocabulary.swift`, `recordingKind.ts`), the last choice is
  remembered, and every start path respects it. The discipline is right
  even if the second question should be optional (F-19).
- **Mark this moment.** One tap = ★, six seconds to refine, the third mark
  renamed by type, the same stored kind everywhere, pins on the timeline,
  chips in the transcript. On the watch, tap anywhere in Discreet. This is
  the product's signature interaction; do not add a fourth kind.
- **Delete vs Strike from the record** (`audit/ios-57-word-editor.png`,
  `audit/ios-59-strike-confirm.png`, `audit/mac-37-strike-dialog.png`). Two
  verbs with two honest consequences, a 5-second undo for one and a marker
  for the other, "This permanently destroys" listing exactly what, the
  reason that cannot contain the stricken words, and the guide that is
  deleted because it paraphrases. Keep every word; only the Mac toolbar
  around it needs thinning (F-8).
- **The privacy sentence and where it sits.** "Recording and transcription
  work offline. Nothing leaves this iPhone unless you want it to." on the
  Record screen; the consent sheet that names the actual host; "What leaves
  this device" as a settings section with Revoke. Say it fewer times, but
  not differently.
- **The review guide** (`audit/ios-45-review-guide.png` → `ios-48-quiz.png`).
  Summary, Key terms, Flashcards (Again/Known), Practice quiz with a jump to
  the moment, Questions to ask. Five tabs, no settings, exports to CSV and
  Markdown. Finished.
- **Chat's scope and citations** (`audit/ios-25-chat-empty.png`,
  `audit/mac-17-chat-answer.png`). "Ask about: All recordings / This Notebook
  / This recording", suggested questions built from real titles, [1] [2]
  chips that open the recording at the moment, and the answer says which
  scope it used. Keep; only the Mac header's caps and the "THIS RECORDING ·
  7 RECORDINGS" status line go.
- **Notebooks as the one grouping.** Derived from recordings, no management
  screen, chips to filter, the last eight sent to the watch. Right size.
- **The Watch app, whole.** Record, three short lists, a recording screen
  with time left and Mark, Discreet, a two-step stop, a list with "Sending to
  iPhone / Delivered". Nothing to cut.
- **Auto-stop with a 30-second banner and Keep recording.** The decision is
  made for the user, reversible, and never loses audio. The settings that
  tune it can go (F-9); the behavior stays.
- **The hazard-yellow on black palette and Inter.** It is distinctive and
  consistent across Mac, iOS and Watch; the problem on the Mac is the caps
  and the emoji, not the colors.
- **The store copy's structure** ("Record anything, find what matters / For
  meetings / For classes / For everyday life / Review any recording / Keep
  control of the record / Free and Pro"). It is already the product this
  audit asks for; the Mac app should catch up to it.
