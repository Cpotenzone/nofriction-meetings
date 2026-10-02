# noFriction user guide

noFriction records your meetings, transcribes them on your device, matches
them to your calendar and, with the AI you choose, writes notes. It works on
iPhone, iPad and Mac. There's no account to create.

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
2. The setup wizard asks how to transcribe. Choose **Private & Offline**: the
   app downloads a 547 MB speech model once and then transcribes on your Mac,
   even offline.
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

AI features use **your own** AI account, so your provider bills you directly
and noFriction never sees your data.

1. Get an API key from a provider, for example
   [OpenAI](https://platform.openai.com/api-keys) (the default),
   [Anthropic](https://console.anthropic.com/settings/keys) or
   [Google Gemini](https://aistudio.google.com/apikey). Also supported:
   xAI Grok, Groq, OpenRouter, Mistral, DeepSeek, Perplexity and Together.
2. Paste it:
   - iPhone/iPad: **Settings tab → Connect AI → Paste your API key → Connect**.
   - Mac: **Settings → AI Engine → Paste your API key**.
3. noFriction recognizes the provider, checks the key and picks a model. You
   can change the model in the same screen.
4. The first time an AI feature sends a meeting to a cloud provider, the app
   asks you and says what will be sent. You can revoke this later in Settings.

**No key needed:**
- **Apple on-device model**: on iOS 26 or macOS 26 or later with Apple
  Intelligence on. On iPhone/iPad, tap **Use Apple on-device (no key)**; on
  the Mac it's used automatically when no other provider is set up. Nothing
  leaves the device. (On the Mac, it doesn't analyze screenshots.)
- **Your own server**: Ollama, LM Studio or any OpenAI-compatible endpoint.
  iPhone/iPad: **Settings tab → Your own server**. Mac: **Settings → AI
  Engine** (local endpoints).

Keys are stored in the system Keychain. To remove one: iPhone/iPad, swipe
left on it under **Saved providers**; Mac, **Settings → AI Engine → Saved
providers**.

---

## Use the AI features

AI features need **noFriction Pro** (see [Subscription](#subscription)) and a
connected AI.

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
  writes a summary, decisions and action items with your connected AI. Export
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
provider can't be recalled. On the Mac, you can't edit the screens of a
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
- Your AI provider's usage is billed by that provider, not included in Pro.

---

## Privacy

- Audio, transcripts, photos and screenshots are stored only on your device.
- Transcription runs on the device (on the Mac, unless you choose a cloud
  transcription service in **Settings → Transcription**, with your own key).
- AI features send meeting text (and, on the Mac, screenshots for screen
  features) straight to the provider you chose. On the Mac, once a provider is
  connected and approved, live insights and the after-meeting report use it
  automatically.
- No accounts, no analytics, no tracking, no noFriction servers.

Full policy: [nofriction.ai/privacy](https://nofriction.ai/privacy).

---

## Troubleshooting

| Problem | Fix |
|---|---|
| No transcript | Allow Microphone and Speech Recognition (iPhone/iPad), or Microphone (Mac), in System Settings → Privacy & Security. On a Mac, make sure the speech model finished downloading (Settings → Transcription). |
| Other people on the call aren't transcribed (Mac) | Allow **Screen & System Audio Recording**, then restart the recording. |
| No screenshots (Mac) | Allow Screen & System Audio Recording; check **LIVE → Change** has a screen or window selected. |
| Meeting not named from the calendar | Allow Calendars. The recording must overlap a calendar event. |
| "Wrong key" | Copy the whole key again; check it hasn't been revoked. |
| "No credit" or rate-limited | Add billing or credit at your provider. |
| Can't reach a local server | Check the address. Plain `http://` only works for this device, your local network or Tailscale. On iPhone/iPad, allow Local Network access. |
| Recording stopped by itself | That's automatic stop; see [Automatic stop](#automatic-stop). |
| Subscription not recognized | **Restore Purchases**, signed in with the Apple Account you subscribed with. |

Still stuck? Email [support@nofriction.ai](mailto:support@nofriction.ai)
with your device, OS version and app version.
