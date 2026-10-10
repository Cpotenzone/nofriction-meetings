# noFriction user guide

noFriction records meetings, classes and everyday conversations, transcribes
them on your device, matches meetings to your calendar, lets you rewind to any
moment and writes notes with Apple's on-device model or an AI endpoint you set
up yourself. It works on iPhone, iPad and Mac. There's no account to create.

- [Get started](#get-started)
- [Record](#record)
- [Add slides and screens](#add-slides-and-screens)
- [Find and review recordings](#find-and-review-recordings)
- [Connect your AI](#connect-your-ai)
- [Use the AI features](#use-the-ai-features)
- [Mark moments and review](#mark-moments-and-review)
- [Delete or strike something](#delete-or-strike-something)
- [Export your recordings](#export-your-recordings)
- [Sync your iPhone and Mac](#sync-your-iphone-and-mac)
- [Subscription](#subscription)
- [Privacy](#privacy)
- [Troubleshooting](#troubleshooting)

---

## Get started

### iPhone and iPad (iOS/iPadOS 18 or later)

1. Install noFriction from the App Store and open it.
2. The first time you record, allow **Microphone**, **Speech Recognition**
   and **Calendars**. Calendar access is read-only; it's used to name
   meetings and list who attended.

### Mac (macOS 12.3 or later)

1. Install noFriction from the Mac App Store and open it.
2. Setup is one screen. Click **Allow** next to **Microphone**, **Screen &
   System Audio** (so the other people on a call are captured, and screens can
   be saved) and **Calendar** (optional). If you miss a prompt, open System
   Settings → Privacy & Security.
3. The speech model (547 MB) downloads on its own while you're on that
   screen; a progress line shows how far it is. **Smaller model** offers a
   lighter one for older or Intel Macs. It's downloaded once, and then the app
   transcribes on your Mac, even offline. No cloud transcription service or
   key is used. Click **Continue**.

---

## Record

**iPhone/iPad:** open the **Record** tab and tap **Record**. The first time,
a notice reminds you that recording laws vary; tap **I understand** (it's
asked once). Choose what it is and how long (see below), and tap **Record**.
The transcript appears as people talk. Recording continues with the screen
locked. Tap **Stop** when you're done.

**Mac:** click **Record**, choose what it is and how long, and start. When you stop, the new recording opens in **Recordings**. For a meeting, the bar shows the matched calendar
event and how many people are invited. Stop when you're done.

> Tell people you're recording. In many places everyone must agree to be
> recorded.

### Automatic stop

When the calendar event is over and the room goes quiet, or nobody has spoken
for a few minutes, noFriction shows a banner and stops after 30 seconds.
Choose **Keep recording** to carry on (it won't ask again for 10 minutes on
iPhone/iPad), or **Stop**. Nothing said before the stop is lost. On the Mac
it also notices when the call app releases the microphone (macOS 14.2 or later)
or the meeting window closes.

Turn it off:
- iPhone/iPad: **Settings tab → Recording → Stop when it's over** (the
  silence time is 3 minutes).
- Mac: **Settings → Recording → Stop when it's over**. The silence time is
  always 3 minutes.

### What is it, how long, and notebook

When you tap or click **Record**, noFriction asks:

- **What is it?** **Meeting**, **Class** or **Personal**. Personal covers
  everything else: conversations, appointments, talks, ideas. Your last
  choice is selected for you (Meeting the first time). The type decides how
  notes are written, what the third mark is called and whether you get a
  review or a study guide (see below). You can change it later on the
  recording.
- **How long?** **15, 30, 60 or 90 minutes, or No limit (∞)**. Your last
  choice is selected for you. The recording stops by itself at the end,
  through the same Stop as yours, so notes are still written.
- **Notebook** (optional): a name that groups related recordings, like
  "Acme project", "BIO 101" or "Health". Tap a recent notebook to reuse it.

On the Mac, press **M**, **C** or **P** to pick the type and **1–5** to pick
the length (these keys type normally while you're in the Notebook field),
Enter to start, Esc to cancel.

A recording that isn't matched to a calendar event is named after its
notebook (or its type) and the date, for example "BIO 101 — Oct 7" or
"Personal — Oct 7".

**Time limit:**

- Five minutes before the end (two for a 15-minute recording) you get a
  warning with **+15 min** and **No limit**. The same buttons sit next to the
  time left on the recording screen (Mac: the capture bar; also in the
  menu-bar icon).
- Mac: ⌘N and the menu-bar icon's **Start Recording**
  skip the questions and use your last type and length (the menu-bar icon
  shows them, for example "Start Recording (Meeting, 60 min)").
- iPhone/iPad: the warning is also a notification, if you've allowed
  notifications for noFriction. You're asked the first time you record with a
  time limit (or a meeting-end auto-stop), never when the app opens.

### Notes by type

| Type | Notes |
|---|---|
| Meeting | Summary, key topics, decisions and action items, plus a follow-up email |
| Class | Lecture notes: key concepts, definitions and examples, and announcements or deadlines the instructor mentioned |
| Personal | Summary, key points, and to-dos and reminders |

Filter your recordings by notebook with the **Notebooks** chips in
**Recordings** (iPhone/iPad and Mac).

The first time you record a **Class**, noFriction shows this once:

> Many schools require the instructor's permission to record a class, and
> some require classmates' consent. Check your school's policy.

---

## Add slides and screens

**iPhone/iPad:** while recording, tap **Photo** to take a picture of a slide
or whiteboard, or touch and hold it and choose **Choose from Photos** to add
screenshots. They appear in the recording.

### Capture your screen (iPhone and iPad)

noFriction can keep a picture of what's on your screen each time it changes
(at most one a second) while you record, the way the Mac does: a class
video, slides in a call, a page you're reading. They appear in the recording
as **Screens**, next to the transcript.

1. Tap **Record**, turn on **Capture screen**, and tap **Record**.
2. The system sheet opens with noFriction selected. Tap **Start Broadcast**,
   then use any app. The status bar turns red while your screen is captured.
3. Back in noFriction, Record shows **Capturing your screen** and how many
   screens it has kept. **Stop** there ends the screen capture; the
   recording goes on until you stop it.
4. Stop the recording. The screens are in the recording, at the times they
   were captured.

You can also start or stop it in Control Center: touch and hold **Screen
Recording**, pick **noFriction**, and tap **Start Broadcast**. If you're
recording, the screens join that recording. If you aren't, a recording
starts when you next open noFriction (the type and notebook you used last,
no time limit), or, if you stopped before that, the screens become a
recording of their own.

**Transcribe what's playing** (noFriction Pro): under Capture screen, this
also turns the sound of what's playing on your iPhone or iPad into text,
marked **On screen** and placed in time with what your microphone heard. It
runs on the device, and the sound itself isn't kept. It's on by default with
Pro; without Pro, turning it on shows what Pro includes.

Good to know:
- Some apps hide their video from screen capture (protected films and
  shows); those parts come out black and are skipped. noFriction tells you
  once.
- Locking your iPhone ends the screen capture. Pause in noFriction pauses it too.
- Screens and On screen lines can be deleted or stricken from the record
  like photos and lines; deleting the recording removes them all.
- With AirPods or other Bluetooth headphones, a recording with Capture
  screen keeps them in full-quality sound and records with the iPhone's
  microphone.

**Mac:** noFriction saves a screenshot of the chosen screens when they change
(about once a second at most) while you record.
- Choose what's captured: **Change** in the bar while recording, then pick any screens or
  windows. If nothing is selected, the main display is captured.
- Save one right now: **Capture screen**.

---

## Find and review recordings

**iPhone/iPad:** the **Recordings** tab lists every recording by day, named
from your calendar when it matches an event. Search by title, person, topic
or anything said. Filter with the **Notebooks** chips. Open a recording to
read the transcript, play the audio and see photos. **People**, a row at the
top of Recordings, lists everyone you've met with and their recordings; on a
person's page you can add their LinkedIn link.

**Topics** are 1–4 short phrases naming what a recording was about ("Q4
roadmap", "Mitosis"). On iPhone/iPad they're named by your AI when notes are
made, or with **Find topics** in the recording's Notes section. Tap a topic
there to **Rename** or **Remove** it, or **Add** your own; your own topics
are kept when topics are found again, and a topic you removed doesn't come
back. Spellings that mean the same thing ("Q4 roadmap", "the q4 roadmaps")
count as one topic in the chips, the grouping and Chat.

**Mac:** **Recordings** lists every recording by day. The search field at
the top (⌘K) finds titles, people, topics and anything said; a line that
matches opens the recording at that moment. Filter with the **Notebooks**
chips. Open a recording to see its type, notebook and attendees, and views
for **Rewind** (the transcript next to the screenshot timeline: pick any
moment to see what was on screen and what was said), **Notes**, **Links**
and **Review guide**. Topics are named by your AI when notes are made, and
listed under the notes; **Edit** there renames, removes or adds them, and
**Find again** asks your AI. Your own topics are kept when topics are found
again, and a topic you removed doesn't come back.

---

## Connect your AI

AI runs in one of two places, and you choose which. noFriction doesn't host a
model, doesn't supply an API key and doesn't pick a service for you. Nothing
remote is set up until you enter it.

### Apple on-device (no setup, no key)

Needs iOS 26 or macOS 26 or later on a device that supports Apple
Intelligence, with Apple Intelligence turned on and its model downloaded.
Nothing leaves the device.

- If you haven't chosen anything else, the app uses it automatically when it's
  available.
- iPhone/iPad: to switch to it, tap the **Apple on-device** card under
  **Settings tab → Connect** (or **Apple on-device** under **Saved
  connections**). It's also offered in the **Set up AI** sheet that opens the
  first time you tap **Make notes**, **Chat** or make a review guide.
- Mac: **Settings → AI → Saved connections → Apple on-device → Use**.
  On the Mac, Apple on-device doesn't analyze screenshots.

### Your own endpoint

Any server that speaks the OpenAI-compatible chat-completions API. Examples
are Ollama or LM Studio running on your Mac or on another computer on your
network, or an HTTPS service you've chosen and have an account with. From
your server's documentation you need:

- **Base URL**: the address the API lives under, usually ending in `/v1`. For
  example, Ollama on the same Mac is `http://localhost:11434/v1` and LM Studio
  is `http://localhost:1234/v1`. On iPhone/iPad, `localhost` means the phone
  itself, so use the other computer's network address (for example
  `http://192.168.1.20:11434/v1` or `http://my-mac.local:11434/v1`), and make
  sure the server accepts connections from your network.
- **Model ID**: exactly as your server names it.
- **API key**: only if your server requires one. Leave it empty otherwise.

Enter them:
- iPhone/iPad: **Settings tab → Connect**: pick a card, or **Custom
  endpoint**, then **Base URL**, **Model ID** and **API key** → **Save**.
- Mac: **Settings → AI**: pick a card, or **Custom endpoint**, then
  **Base URL**, **Model** and **API key** → **Save connection**.

Saving doesn't contact the server. To check it, tap **Test connection**
(iPhone/iPad) or click **Test connection** (Mac; or **Test** next to it under
**Saved connections**); it sends only the word "Hi". On the Mac, **↻** under
**Models** reloads the server's model list.

Good to know:
- **HTTPS for the internet.** An address on the internet must start with
  `https://`. Plain `http://` works only for this device, your local network
  (including `.local` names) or a Tailscale network.
- **Permission before sending.** Before recording content first goes to a public
  (internet) endpoint, the app asks you and shows where it will go. Revoke it
  any time: iPhone/iPad, **Settings tab → What leaves this device → Revoke**;
  Mac, **Settings → AI → Revoke permission to send to …**. Endpoints on
  your own network don't ask, but the content still travels to that machine.
- **What's sent.** Depending on the feature: transcript text, the recording's
  title and notebook, attendee names and emails, notes and, on the Mac, screenshots for
  screen features. The endpoint's operator decides what it keeps; noFriction
  never receives any of it.
- **Keys.** A key is stored in the system Keychain, tied to the endpoint you
  entered it for, and is never shown in full again. If you change the
  endpoint's address, its old key, permission and model are cleared, and you
  enter them again.
- **Coming from an earlier version?** If an older version had an AI service
  chosen by name, it's no longer available and AI asks you to set it up again.
  Choose Apple on-device or enter that service's endpoint yourself. Your
  recordings aren't affected.

To remove a connection and its key: iPhone/iPad, swipe left on it under
**Saved connections**; Mac, **Settings → AI → Saved connections →
Remove**.

---

## Use the AI features

AI features need **noFriction Pro** (see [Subscription](#subscription)) and
Apple on-device or your own endpoint (see [Connect your AI](#connect-your-ai)).

**iPhone/iPad**, in a recording:
- **Make notes** (**Make again** afterwards): notes in the recording type's
  style (see [Notes by type](#notes-by-type)). Owners and due dates appear
  only if someone said them. The recording's **Topics** are named at the
  same time (see [Find and review recordings](#find-and-review-recordings)).
- **Find topics**: name the topics without redoing the notes.
- **Follow-up email** (meetings): a draft to the attendees, recapping
  decisions and next steps.

**iPhone/iPad**, the **Chat** tab: ask about your recordings ("What did we
decide about the launch date?", "What did the professor say about the
midterm?"). Pick the scope at the top first: **All recordings**, **this
Notebook**, **this Topic** or **this recording**; every answer says which
scope it used. Answers come only from your transcripts, notes and marks,
and cite them as [1], [2]…; tap a citation chip to open that
recording at that moment. An empty chat suggests questions from the scope's
titles and topics. Chats are kept on the device; **New chat** starts another,
and the list button shows past chats (swipe to delete). If you delete or
edit a recording, answers that cited it are removed and the chat says so.
The same Pro subscription and AI connection as the other features apply, and
the same consent dialog when the endpoint is a public one.

**Mac:**
- **Chat**: ask about your recordings ("What did we decide about the launch
  date?", "What did the professor say about the midterm?"). Pick the scope
  at the top first: **All recordings**, **This Notebook**, **This Topic** or
  **This recording** (a new chat starts on the recording open in Recordings,
  if any); every answer says which scope it used. Answers come only from your
  transcripts, notes and marked moments, searched on this Mac, and cite them
  as [1], [2]…; click a citation chip (or a line under **Sources**) to open
  that recording in Rewind at that moment. An empty chat suggests questions
  from the scope's titles and topics. Chats are kept on this Mac; **New
  chat** starts another, and **Chats** lists past ones (delete with the
  bin). If you delete or edit a recording, answers that drew on it are
  removed and the chat says so.
- **Notes**: after a recording longer than six minutes, noFriction makes
  notes in the recording type's style with your chosen AI. Read them in the
  recording's **Notes** view, or export them to Obsidian (see
  [Export](#export-your-recordings)). A meeting's notes also offer a
  **Follow-up email**.

AI can be wrong. Check names, numbers and dates before you send anything.

---

## Mark moments and review

**Mark a moment while you record.** Press **Mark** (Mac: in the bar at the
top of the Record screen, or the menu-bar icon; iPhone/iPad: **Mark this moment** above the record button). One
press marks it **★ Important**. Right after, pick **? Question** (something
to ask, or you were confused) or the third mark, named for the recording's
type, and add a short note if you like:

| Type | Third mark |
|---|---|
| Meeting | **✎ Follow up** |
| Class | **✎ On the test** |
| Personal | **✎ Remember** |

On the Mac, **⌃⌥⌘M** marks the moment even while your slides, browser or
video call are in front, and **File → Mark** does the same.

**Find your marks later.** Mac: in **Rewind**, marks sit on the timeline and
in the transcript at their time, and **Marks** (above the transcript, closed
until you open it) jumps to each one. Filter it to show only **✎ Follow up** after a meeting, or **✎ On the test**
before an exam. You can change a mark's type or note, delete it, or add one
at the scrubber's time. iPhone/iPad: **Marks** in the recording.

**Make a review guide** (noFriction Pro, with Apple on-device or your own AI
endpoint). It's called a **study guide** for a class; the contents are the
same for every type: for a class it's what to study, for a meeting what to
remember and follow up, for personal recordings what to remember. Mac: open
the recording → **Review guide** → **Make review guide** (**Study guide** →
**Make study guide** for a class). iPhone/iPad: open the recording →
**Review** → **Make review guide** (**Make study guide** for a class). You get:
- **Summary** as short notes
- **Key terms** with definitions
- **Flashcards**: click or tap to flip, then **Known** or **Again** (Mac:
  Space flips, K and A answer, S shuffles). "Again" cards come back until you
  know them all.
- **Practice quiz**: see right or wrong with a one-line explanation, and
  jump to the moment in the recording where the answer is. Your score is
  shown at the end.
- **Questions to ask** (your instructor, or to follow up on), including the
  moments you marked **? Question**.

Moments you marked with the third mark (✎) and **★ Important** get extra
weight. On an iPhone or an older Mac, Apple's on-device model reads a long
recording in parts, so it can take a minute or two.

**Export:** flashcards as a CSV file that popular flashcard apps (such as
Anki or Quizlet) can import, and the whole guide as Markdown (Mac: buttons
under the guide; iPhone/iPad: the share button in the guide).

The guide is made from the transcript only. Deleted or stricken words are
never sent, and deleting or striking any transcript text deletes the guide
(make it again afterwards). Deleting a time range on the Mac removes the
marks inside it too.

---

## Delete or strike something

Select words, whole lines, or a photo/screenshot, then choose:

| | **Delete** | **Strike from the record** |
|---|---|---|
| Use for | Mistakes, junk, false starts | Content that must not exist anywhere |
| What's left | Nothing; the transcript closes up | A marker: "Stricken from the record", the time it covered, when, and an optional reason |
| Undo | 5 seconds | None |

Both remove the content from the transcript, search, the saved audio
(iPhone/iPad, replaced with silence), screenshots, the app's backups (Mac)
and future exports. Notes made before the edit are marked so you can
make them again. Copies you already shared, exported or sent to an AI
endpoint can't be recalled. On the Mac, you can't edit the screens of a
recording that is still running.

To delete a whole recording: iPhone/iPad, open it → **⋯ → Delete Recording**; Mac,
the bin next to it in **Recordings** (you get 5 seconds to **Undo**).

---

## Export your recordings

- **iPhone/iPad:** in a recording, tap the **Share** button to send it
  as text (Markdown) to Notes, Mail, Files or any app.
- **Mac:** **Export everything as JSON** in **Settings → Recording** saves
  one file you choose (free). **Export to Obsidian** (noFriction Pro), in the
  same place, picks your vault folder; the switch beside it exports each
  recording as Markdown when it stops.

Each device keeps what it recorded. With noFriction Pro, **Sync** keeps
your iPhone and Mac in step (see [Sync your iPhone and Mac](#sync-your-iphone-and-mac)).

---

## Sync your iPhone and Mac

With noFriction Pro, your iPhone (or iPad) and your Mac keep the same
recordings. They talk directly over your own Wi-Fi, encrypted. There is no
noFriction server, no iCloud and no relay in between, and noFriction
receives nothing.

**Pair once**

1. On the Mac: **Settings → Sync**, turn on **Sync on this Mac**, then
   **Pair a device**. A code appears; it works once, for 5 minutes.
2. On the iPhone: **Settings → Sync with your Mac → Pair with your Mac** and
   scan the code. Allow **Local Network** when asked. No camera, or scanning
   on an iPad? Click **Copy pairing link** on the Mac and tap **Paste pairing
   link** on the iPhone (Universal Clipboard).

Both devices then list each other. **Forget** (Mac: next to the device;
iPhone: swipe the Mac) unpairs them; recordings already copied stay.

**When it syncs.** The iPhone starts every sync: when you open noFriction,
when a recording stops, and when you tap **Sync now**. iOS doesn't let apps
listen in the background, so the Mac can't send to a closed iPhone app. Both
devices must be on the same network, with noFriction open on the Mac and
Sync turned on. **Last synced** on both shows when it last happened.

**What syncs**

- Recordings: title, type (Meeting · Class · Personal), notebook, planned
  length, and the calendar details and people.
- Transcripts, Notes, Marks, Links you added, and Topics.
- Photos and screens: the iPhone's photos and the screens from its screen
  capture, and the Mac's screens, each at the moment it was taken, in
  Rewind on both. They're copied as they are (no lower quality).
- **Delete** and **Strike from the record**. What you delete or strike on
  one device is removed on the other at the next sync, through the same
  clean-up as an edit made there: search, notes, review guide, AI topics,
  chat answers, and on the iPhone the audio; for a photo or screen, the
  picture (and on the Mac the screen video at that moment). A strike leaves
  the same marker on both. The removed words are never sent, and something deleted on either
  device never comes back.

**What doesn't:** audio (it stays on the iPhone that recorded it), chats
and review guides (each device makes its own).

**If both changed the same thing** (say, the title) between two syncs, the
iPhone's version wins. Notes made on the iPhone show on the Mac as written
there.

---

## Subscription

noFriction Pro turns every recording into notes, a review guide and answers,
and keeps your iPhone and Mac in sync. Recording and microphone
transcription stay free. The full list is the same everywhere (owner
decision 2026-10-10).

| Free | noFriction Pro |
|---|---|
| Recording on iPhone, iPad, Mac and Apple Watch, with types, notebooks, timed recording and automatic stop | **Notes** (**Make notes**, **Make again**) and, on the Mac, automatic notes |
| On-device microphone transcription (Mac: also the call's audio) | **Follow-up email** and **Topics** |
| Screens: Mac screenshots, iPhone screen snapshots, photos | **Review guides** (study guides for a class) with flashcards, a practice quiz and CSV/Markdown export |
| Marks, Rewind, search, Links, calendar and people | **Chat** with your recordings |
| Edit, Delete and Strike from the record | **Sync with your Mac**: recordings, transcripts, notes, marks and screens move between iPhone and Mac directly on your Wi-Fi. No server; pair once with a QR code ([details](#sync-your-iphone-and-mac)) |
| Share a recording as text; export everything as JSON (Mac) | **Transcribe what's playing** (iPhone): while you capture the screen, noFriction also transcribes the audio of the video or call you're watching |
| | **Export to Obsidian** (Mac): each recording saved as Markdown in your vault |

When you reach a Pro feature without Pro, the paywall says which one ("Sync
is part of noFriction Pro").

- Pro is $0.99 a month or $5.99 a year in the United States, each with a
  1-week free trial for new subscribers. The price, the period and the trial
  are shown before you buy. After the trial, the subscription renews
  automatically at that price unless you cancel at least 24 hours before the
  end of the period.
- One subscription covers iPhone, iPad and Mac on the same Apple Account.
- **Restore Purchases**: iPhone/iPad, **Settings tab → Subscription**; Mac,
  **Settings → Subscription**.
- **Cancel**: **Manage Subscription** in the same place or on the paywall, or
  your device's Settings → your name → Subscriptions. You keep Pro until the
  end of the period.
- Pro doesn't include an AI service. If your own endpoint charges for use,
  its operator bills you, not noFriction.

---

## Privacy

- Audio, transcripts, photos and screenshots are stored only on your device.
- Transcription always runs on the device: Apple speech recognition on
  iPhone/iPad, Whisper on the Mac. There's no cloud transcription.
- With Apple on-device AI, nothing leaves the device. With your own endpoint,
  AI features send recording text (and, on the Mac, screenshots for screen
  features) straight to that endpoint, after your permission if it's on the
  internet. On the Mac, once AI is set up (and approved, for an internet
  endpoint), the automatic notes use it. Turn that off in
  **Settings → AI → Make notes automatically**. No AI runs while you record.
- Sync (Pro) goes only to your own paired iPhone or Mac, directly over your
  local network and encrypted; nothing passes through a server.
- No accounts, no analytics, no tracking, no noFriction servers.

Full policy: [nofriction.io/privacy](https://nofriction.io/privacy).

---

## Troubleshooting

| Problem | Fix |
|---|---|
| No transcript | Allow Microphone and Speech Recognition (iPhone/iPad), or Microphone (Mac), in System Settings → Privacy & Security. On a Mac, make sure the speech model finished downloading (Settings → Transcription). |
| Other people on the call aren't transcribed (Mac) | Allow **Screen & System Audio Recording**, then restart the recording. |
| No screenshots (Mac) | Allow Screen & System Audio Recording; check **Change** in the recording bar has a screen or window selected. |
| Recording not named from the calendar | Allow Calendars. The recording must overlap a calendar event; otherwise it's named after its notebook or type and the date. |
| AI asks to be set up | Choose Apple on-device, or enter your endpoint's base URL and model (see [Connect your AI](#connect-your-ai)). A service chosen by name in an older version is no longer available. |
| Apple on-device isn't offered | It needs iOS 26 or macOS 26 or later on a device that supports Apple Intelligence, with Apple Intelligence turned on and its model finished downloading. |
| "Wrong key" | Enter the endpoint's key again in full; check it hasn't been revoked. |
| "No credit" or rate-limited | Your endpoint refused the request for billing or rate limits; check with its operator. |
| Model not found or no answer | Check the model ID is exactly what your server lists. On the Mac, **↻** under **Models** reloads the list. |
| Sync can't find the Mac | Both on the same Wi-Fi; noFriction open on the Mac with **Settings → Sync → Sync on this Mac** turned on; on the iPhone, Settings → Privacy & Security → Local Network → noFriction on. Networks that isolate devices (some guest or school Wi-Fi) block it. |
| "Pair again" | The Mac forgot this iPhone, or was reinstalled. Pair again from **Settings → Sync** on both. |
| Can't reach a local server | Check the address and that the server is running. Plain `http://` only works for this device, your local network or Tailscale. On iPhone/iPad, allow Local Network access, and use the computer's network address, not `localhost`. |
| Recording stopped by itself | That's the time limit you chose or automatic stop; see [What is it, how long, and notebook](#what-is-it-how-long-and-notebook) and [Automatic stop](#automatic-stop). |
| Notes are in the wrong style | Change the recording's type (Mac: the type row under the recording's title), then **Make again** on the notes. |
| Subscription not recognized | **Restore Purchases**, signed in with the Apple Account you subscribed with. |

Still stuck? Email [casey@nofriction.io](mailto:casey@nofriction.io) or use
[nofriction.io/contact](https://nofriction.io/contact), with your device, OS
version and app version.
