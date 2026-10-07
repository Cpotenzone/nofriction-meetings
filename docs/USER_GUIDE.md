# noFriction user guide

noFriction records your meetings, transcribes them on your device, matches
them to your calendar and writes notes with Apple's on-device model or an AI
endpoint you set up yourself. It works on iPhone, iPad and Mac. There's no
account to create.

- [Get started](#get-started)
- [Record a meeting](#record-a-meeting)
- [Add slides and screens](#add-slides-and-screens)
- [Find and review meetings](#find-and-review-meetings)
- [Connect your AI](#connect-your-ai)
- [Use the AI features](#use-the-ai-features)
- [Delete or strike something](#delete-or-strike-something)
- [Export your meetings](#export-your-meetings)
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
2. The setup wizard offers a speech model for on-device transcription. Click
   **Download** on the recommended model (547 MB; a smaller one is offered for
   older or Intel Macs). It's downloaded once, and then the app transcribes on
   your Mac, even offline. No cloud transcription service or key is used.
3. Allow **Microphone**, **Screen & System Audio Recording** (so the other
   people on a call are captured, and screens can be saved) and **Calendars**.
   If you miss a prompt, open System Settings → Privacy & Security.

---

## Record a meeting

**iPhone/iPad:** open the **Record** tab and tap **Record**. The first time,
a notice reminds you that recording laws vary; tap **I understand**. The
transcript appears as people talk. Recording continues with the screen
locked. Tap **Stop** when you're done.

**Mac:** open **LIVE** and start recording. The bar shows the matched calendar
event and how many people are invited. Stop when you're done.

> Tell people you're recording. In many places everyone must agree to be
> recorded.

### Automatic stop

When the calendar event is over and the room goes quiet, or nobody has spoken
for a few minutes, noFriction shows a banner and stops after 30 seconds.
Choose **Keep recording** to carry on (it won't ask again for 10 minutes on
iPhone/iPad), or **Stop now**. Nothing said before the stop is lost. On the Mac
it also notices when the call app releases the microphone (macOS 14.2 or later)
or the meeting window closes.

Turn it off or change the silence time:
- iPhone/iPad: **Settings tab → Recording**.
- Mac: **Settings → General → Recording** ("Stop automatically when the
  meeting ends", "Silence before stopping").

### How long? (timed recording)

When you tap or click **Record**, noFriction asks how long: **15, 30, 60 or
90 minutes, or No limit (∞)**. Your last choice is selected for you. On the
Mac, press 1–5 to pick, Enter to start, Esc to cancel. The recording stops by
itself at the end, through the same Stop as yours, so notes are still written.

- Five minutes before the end (two for a 15-minute recording) you get a
  warning with **+15 min** and **No limit**. The same buttons sit next to the
  time left on the recording screen (Mac: the capture bar; also in the
  menu-bar icon).
- Mac: ⌘N, the menu-bar icon's **Start Recording** and the command palette
  skip the question and use your last choice. **Start Recording For** in the
  menu-bar icon picks a length directly.
- iPhone/iPad: the warning is also a notification, if you've allowed
  notifications for noFriction. You're asked the first time you record with a
  time limit (or a meeting-end auto-stop), never when the app opens.

### Classes

Recording a lecture? Type the class in the same sheet (for example
"BIO 101 — Cell Biology"), or tap one of your recent classes. You can change
it later on the recording. Filter your recordings by class in **Meetings**
(iPhone/iPad) or **REWIND → Recordings** (Mac). AI notes for a class are
lecture notes: key concepts, definitions, examples, and announcements or
deadlines the instructor mentioned.

> Many schools require the instructor's permission to record a class, and
> some require classmates' consent. Check your school's policy.

---

## Add slides and screens

**iPhone/iPad:** while recording, tap **Take Photo** for a slide or whiteboard,
or **Choose from Photos** to add screenshots. They appear in the meeting.

**Mac:** noFriction saves a screenshot of the chosen screens when they change
(about once a second at most) while you record.
- Choose what's captured: **LIVE → Change**, then pick any screens or
  windows. If nothing is selected, the main display is captured.
- Save one right now: **Snap**.

---

## Find and review meetings

**iPhone/iPad:** the **Meetings** tab lists every recording, named from your
calendar. Search by title, person or anything said. Open a meeting to read
the transcript, play the audio and see photos. The **People** tab lists
everyone you've met with; tap **LinkedIn** to search for someone and paste
their profile link.

**Mac:** **REWIND** has:
- **Recordings**: each meeting with its attendees, the transcript and the
  screenshot timeline.
- **People**: everyone from your meetings, with invite notes, join links and
  LinkedIn links.
- **Search**: full-text search across your transcripts.

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
- iPhone/iPad: to switch to it, go to **Settings tab → Saved connections** and
  tap **Use Apple on-device (no key)**, or tap **Apple on-device** if it's
  already listed. It's also offered in the **Set up AI** sheet that opens from
  a meeting.
- Mac: **Settings → AI Engine → Saved providers → Apple on-device → Use**.
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
- iPhone/iPad: **Settings tab → Your AI endpoint**: **Base URL**, **Model
  ID**, **API key (optional)** → **Save endpoint**.
- Mac: **Settings → AI Engine → Local & custom servers**: the URL, **Model
  name** and **API key (optional)** → **Save connection**.

Saving doesn't contact the server. To check it on the Mac, click **Test**
next to it under **Saved providers**; **↻** under **Models** reloads the
server's model list. On iPhone/iPad, try **Summarize** on a meeting.

Good to know:
- **HTTPS for the internet.** An address on the internet must start with
  `https://`. Plain `http://` works only for this device, your local network
  (including `.local` names) or a Tailscale network.
- **Permission before sending.** Before meeting content first goes to a public
  (internet) endpoint, the app asks you and shows where it will go. Revoke it
  any time: iPhone/iPad, **Settings tab → What leaves this device → Revoke**;
  Mac, **Settings → AI Engine → Revoke permission to send to …**. Endpoints on
  your own network don't ask, but the content still travels to that machine.
- **What's sent.** Depending on the feature: transcript text, the meeting
  title, attendee names and emails, notes and, on the Mac, screenshots for
  screen features. The endpoint's operator decides what it keeps; noFriction
  never receives any of it.
- **Keys.** A key is stored in the system Keychain, tied to the endpoint you
  entered it for, and is never shown in full again. If you change the
  endpoint's address, its old key, permission and model are cleared, and you
  enter them again.
- **Coming from an earlier version?** If an older version had an AI service
  chosen by name, it's no longer available and AI asks you to set it up again.
  Choose Apple on-device or enter that service's endpoint yourself. Your
  meetings aren't affected.

To remove a connection and its key: iPhone/iPad, swipe left on it under
**Saved connections**; Mac, **Settings → AI Engine → Saved providers →
Remove**.

---

## Use the AI features

AI features need **noFriction Pro** (see [Subscription](#subscription)) and
Apple on-device or your own endpoint (see [Connect your AI](#connect-your-ai)).

**iPhone/iPad**, in a meeting:
- **Summarize**: notes with a summary, decisions and action items. Owners
  and due dates appear only if someone said them.
- **Follow-up email**: a draft to the attendees, recapping decisions and next
  steps.

**Mac:**
- **CHAT**: ask a question about your meetings ("What did we decide about
  the launch date?"). The answer is drawn from your transcripts.
- **Live insights**: during a recording, LIVE shows action items, decisions,
  risks and deadlines as they come up.
- **Meeting report**: after a recording longer than six minutes, noFriction
  writes a summary, decisions and action items with your chosen AI. Export
  it to Obsidian to read it (see [Export](#export-your-meetings)).
- **Meeting prep**: in **INTEL**, **Lookup** on an upcoming meeting gives a
  prep brief on the attendees (uses notes in your Obsidian vault).

AI can be wrong. Check names, numbers and dates before you send anything.

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
and future exports. AI notes made before the edit are marked so you can
regenerate them. Copies you already shared, exported or sent to an AI
endpoint can't be recalled. On the Mac, you can't edit the screens of a
meeting that is still recording.

To delete a whole meeting: iPhone/iPad, open it → **⋯ → Delete Meeting**; Mac,
from the meeting list in **REWIND → Recordings**.

---

## Export your meetings

- **iPhone/iPad:** in a meeting, tap the **Share** button to send the meeting
  as text (Markdown) to Notes, Mail, Files or any app.
- **Mac:** **Settings → Obsidian** to pick your vault folder and, if you like,
  turn on **Auto-Export Meetings**. Export by hand from **VAULT**. To export
  everything as JSON: **Settings → Data → Export Data to JSON**.

Meetings don't sync between devices. Each device keeps what it recorded.

---

## Subscription

| Free | noFriction Pro |
|---|---|
| Recording, on-device transcription, calendar and people, photos and screenshots, search, Delete and Strike, export | The AI features above |

- Pro is a monthly or yearly subscription. Prices (and any free trial) are
  shown before you buy.
- One subscription covers iPhone, iPad and Mac on the same Apple Account.
- **Restore Purchases**: iPhone/iPad, **Settings tab → Subscription**; Mac,
  **Settings → Subscription**.
- **Cancel**: **Manage Subscription** in the same place, or your device's
  Settings → your name → Subscriptions. You keep Pro until the end of the
  period.
- Pro doesn't include an AI service. If your own endpoint charges for use,
  its operator bills you, not noFriction.

---

## Privacy

- Audio, transcripts, photos and screenshots are stored only on your device.
- Transcription always runs on the device: Apple speech recognition on
  iPhone/iPad, Whisper on the Mac. There's no cloud transcription.
- With Apple on-device AI, nothing leaves the device. With your own endpoint,
  AI features send meeting text (and, on the Mac, screenshots for screen
  features) straight to that endpoint, after your permission if it's on the
  internet. On the Mac, once AI is set up (and approved, for an internet
  endpoint), live insights and the after-meeting report use it automatically.
  Turn either off in **Settings → AI Engine → Automatic AI**.
- No accounts, no analytics, no tracking, no noFriction servers.

Full policy: [nofriction.io/privacy](https://nofriction.io/privacy).

---

## Troubleshooting

| Problem | Fix |
|---|---|
| No transcript | Allow Microphone and Speech Recognition (iPhone/iPad), or Microphone (Mac), in System Settings → Privacy & Security. On a Mac, make sure the speech model finished downloading (Settings → Transcription). |
| Other people on the call aren't transcribed (Mac) | Allow **Screen & System Audio Recording**, then restart the recording. |
| No screenshots (Mac) | Allow Screen & System Audio Recording; check **LIVE → Change** has a screen or window selected. |
| Meeting not named from the calendar | Allow Calendars. The recording must overlap a calendar event. |
| AI asks to be set up | Choose Apple on-device, or enter your endpoint's base URL and model (see [Connect your AI](#connect-your-ai)). A service chosen by name in an older version is no longer available. |
| Apple on-device isn't offered | It needs iOS 26 or macOS 26 or later on a device that supports Apple Intelligence, with Apple Intelligence turned on and its model finished downloading. |
| "Wrong key" | Enter the endpoint's key again in full; check it hasn't been revoked. |
| "No credit" or rate-limited | Your endpoint refused the request for billing or rate limits; check with its operator. |
| Model not found or no answer | Check the model ID is exactly what your server lists. On the Mac, **↻** under **Models** reloads the list. |
| Can't reach a local server | Check the address and that the server is running. Plain `http://` only works for this device, your local network or Tailscale. On iPhone/iPad, allow Local Network access, and use the computer's network address, not `localhost`. |
| Recording stopped by itself | That's automatic stop; see [Automatic stop](#automatic-stop). |
| Subscription not recognized | **Restore Purchases**, signed in with the Apple Account you subscribed with. |

Still stuck? Email [casey@nofriction.io](mailto:casey@nofriction.io) or use
[nofriction.io/contact](https://nofriction.io/contact), with your device, OS
version and app version.
