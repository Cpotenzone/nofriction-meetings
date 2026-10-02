import React, { useState } from "react";
import "./HelpView.css";
import { useCapabilities, type BuildCapabilities } from "../../../lib/build";

type HelpTab = "tech" | "howto" | "guide" | "security" | "services";

// `caps` is null until the backend answers. Build-specific lines render only
// once it's known, so the App Store build never shows Developer ID features.
type Caps = { caps: BuildCapabilities | null };

export const HelpView: React.FC = () => {
    const [activeTab, setActiveTab] = useState<HelpTab>("guide");
    const caps = useCapabilities();

    const tabs: { id: HelpTab; label: string; icon: string }[] = [
        { id: "guide", label: "Help Guide", icon: "📖" },
        { id: "howto", label: "How-To", icon: "🛠️" },
        { id: "tech", label: "How It Works", icon: "⚙️" },
        { id: "security", label: "Privacy & Security", icon: "🔒" },
        { id: "services", label: "Services", icon: "☁️" },
    ];

    return (
        <div className="help-view">
            <div className="help-header">
                <h2>noFriction Documentation</h2>
                <p>How to record, review and work with your meetings, and where your data goes.</p>
            </div>

            <div className="help-tabs">
                {tabs.map((tab) => (
                    <button
                        key={tab.id}
                        className={`help-tab ${activeTab === tab.id ? "active" : ""}`}
                        onClick={() => setActiveTab(tab.id)}
                    >
                        <span className="mode-icon">{tab.icon}</span> {tab.label}
                    </button>
                ))}
            </div>

            <div className="help-content">
                {activeTab === "guide" && <HelpGuide caps={caps} />}
                {activeTab === "howto" && <HowTo caps={caps} />}
                {activeTab === "tech" && <TechSpec caps={caps} />}
                {activeTab === "security" && <SecurityManual caps={caps} />}
                {activeTab === "services" && <ServicesManual caps={caps} />}
            </div>
        </div>
    );
};

/** App Store build only: which features need noFriction Pro. */
const ProNote: React.FC<Caps> = ({ caps }) =>
    caps?.pro_gating ? (
        <div className="help-callout">
            <p><strong>noFriction Pro:</strong> in the Mac App Store version, AI features (notes, summaries,
                action items, follow-up emails, chat and briefings) need a noFriction Pro subscription. Recording, transcription,
                Rewind, editing, calendar and export are free. Manage it in <strong>Settings → Subscription</strong>.</p>
        </div>
    ) : null;

/* ═══════════════════════════════════════════════════════════════════════
   HELP GUIDE
   ═══════════════════════════════════════════════════════════════════════ */
const HelpGuide: React.FC<Caps> = ({ caps }) => (
    <>
        <div className="help-section">
            <h3>Getting Started</h3>
            <p>
                noFriction Meetings records your meetings on your Mac. It captures your microphone and the
                sound from your call app, transcribes it live with Whisper running on this Mac (no account or
                key needed), takes screenshots for a visual timeline, and can write AI meeting notes with the
                AI provider you choose.
            </p>
            <div className="help-callout">
                <p><strong>Quick Start:</strong> On first launch the setup assistant walks you through permissions,
                    downloading the transcription model, and (optionally) adding an AI key. You can run it again
                    from <strong>Settings → General → Run setup assistant again</strong>. Then click
                    <strong> START CAPTURE</strong> in the top-right to record.</p>
            </div>
            <ProNote caps={caps} />
        </div>

        <div className="help-section">
            <h3>Navigation</h3>
            <table className="help-table">
                <thead>
                    <tr><th>View</th><th>Purpose</th></tr>
                </thead>
                <tbody>
                    <tr><td>LIVE</td><td>The current recording: live transcript, screen captures, and a Snap button to capture a screen or window on demand.</td></tr>
                    <tr><td>REWIND</td><td>Your recordings. Pick a meeting and switch between Rewind (screenshots + transcript) and Notes (AI notes). Tabs: Recordings, People, Insights, Search.</td></tr>
                    <tr><td>INTEL</td><td>Upcoming calendar meetings with AI attendee briefings, recent meetings, and your Obsidian knowledge graph.</td></tr>
                    <tr><td>CHAT</td><td>Ask questions about your meetings and get answers with links to the sources.</td></tr>
                    <tr><td>MORE → VAULT</td><td>Browse the meeting notes exported to your Obsidian vault, by topic and tag.</td></tr>
                    <tr><td>MORE → ZEN</td><td>A minimal, distraction-free recording screen.</td></tr>
                    <tr><td>MORE → PROMPTS</td><td>Prompt Studio: view, edit, test and duplicate the AI prompts, per persona.</td></tr>
                    <tr><td>MORE → HELP</td><td>This documentation.</td></tr>
                </tbody>
            </table>
            <p>
                The gear icon opens Settings (General, Transcription, Obsidian, AI Engine,
                {caps?.storekit ? " Subscription," : ""} Data and About). Press <strong>⌘K</strong> or click the
                search button for the command palette.
            </p>
        </div>

        <div className="help-section">
            <h3>Genie Mode</h3>
            <p>
                While recording, click <strong>GENIE</strong> in the toolbar to shrink noFriction to a small floating
                overlay showing recording status and transcript snippets, so you can keep working. With an AI
                provider set up, live insights appear during the meeting (turn off with <strong>Live insights
                during meetings</strong> in Settings → AI Engine).
            </p>
        </div>

        <div className="help-section">
            <h3>Keyboard Shortcuts</h3>
            <table className="help-table">
                <thead>
                    <tr><th>Shortcut</th><th>Action</th></tr>
                </thead>
                <tbody>
                    <tr><td>⌘N</td><td>New recording</td></tr>
                    <tr><td>⌘.</td><td>Stop recording</td></tr>
                    <tr><td>⌘1</td><td>Live</td></tr>
                    <tr><td>⌘2</td><td>Recordings (REWIND)</td></tr>
                    <tr><td>⇧⌘I</td><td>Chat with your meetings</td></tr>
                    <tr><td>⇧⌘P</td><td>Prompts</td></tr>
                    <tr><td>⌘K</td><td>Command palette</td></tr>
                    <tr><td>⌘,</td><td>Settings</td></tr>
                    <tr><td>⇧⌘S</td><td>Snap a screenshot (in LIVE)</td></tr>
                    <tr><td>Esc</td><td>Clear a transcript or screenshot selection</td></tr>
                </tbody>
            </table>
            <p>Pause Recording is in the File menu and the menu-bar icon.</p>
        </div>

        <div className="help-section">
            <h3>Automatic Stop When the Meeting Ends</h3>
            <p>
                noFriction can tell when a meeting is over: the call app releases the microphone, the meeting
                window closes, the calendar event ends, or nobody speaks for a set time (3 minutes by default).
                A 30-second countdown banner then appears with <strong>Keep recording</strong> and
                <strong> Stop now</strong>. If noFriction isn't in front, you get a macOS notification
                ("Meeting seems to have ended — stopping in 30 s"); notification permission is requested the
                first time you record.
            </p>
            <p>
                Turn it on or off and set the silence time in <strong>Settings → General → Recording</strong>, or
                toggle it from the menu-bar icon.
            </p>
        </div>

        <div className="help-section">
            <h3>Meeting Notes</h3>
            <p>
                When you stop a recording longer than <strong>6 minutes</strong> and an AI provider is set up,
                noFriction writes meeting notes automatically. You can also generate or regenerate them for
                any meeting from its <strong>Notes</strong> view in REWIND. Notes include:
            </p>
            <ul>
                <li><strong>Summary</strong>: a short overview of the meeting</li>
                <li><strong>Key Topics</strong>: the main areas discussed</li>
                <li><strong>Decisions</strong>: what was decided</li>
                <li><strong>Action Items</strong>: tasks and who owns them</li>
            </ul>
            <p>
                <strong>Follow-up email</strong> on a meeting drafts a plain-text follow-up with your AI provider,
                which you can copy or open in Mail.
            </p>
            <div className="help-callout">
                <p><strong>Tip:</strong> No AI provider yet? Recording and transcription work fully without one; AI
                    features show an "Add an AI key" button that takes you to <strong>Settings → AI Engine</strong>.
                    Turn automatic notes off there with <strong>Write a report after each meeting</strong>. You can
                    change the notes prompt per persona in <strong>PROMPTS</strong>.</p>
            </div>
        </div>

        <div className="help-section">
            <h3>Editing and "Strike from the record"</h3>
            <p>
                In a past meeting's Rewind, click a word to select it (shift-click or drag for more,
                double-click for a whole line), or select screenshots with ⌘-click or <strong>Select screens</strong>.
                Then choose:
            </p>
            <ul>
                <li><strong>Delete</strong>: removes the content for good, with a 5-second Undo. Use it for mistakes and junk.</li>
                <li><strong>Strike from the record…</strong>: removes the content and leaves a visible
                    "Stricken from the record" marker with the time and an optional reason. It can't be undone,
                    and a confirmation lists exactly what will be destroyed.</li>
            </ul>
            <p>
                Both remove the content everywhere in the app: transcript, search, screenshots and AI outputs.
                AI notes made before an edit show a banner with <strong>Regenerate</strong>. Files you already
                exported outside the app, and Time Machine backups, can't be recalled. Editing is available once
                a recording has stopped.
            </p>
        </div>

        <div className="help-section">
            <h3>Chat — Asking AI About Your Meetings</h3>
            <p>
                <strong>CHAT</strong> answers natural-language questions from your own meetings, for example:
            </p>
            <ul>
                <li>"What decisions were made in last Tuesday's standup?"</li>
                <li>"Summarize all mentions of the budget across my meetings."</li>
                <li>"Who has action items from the last 3 meetings?"</li>
            </ul>
            <p>
                noFriction searches your meetings on this Mac, sends the most relevant passages with your question
                to your AI provider, and shows which sources the answer came from.
            </p>
        </div>

        <div className="help-section">
            <h3>Long Recordings</h3>
            <p>
                At <strong>75 minutes</strong>, noFriction asks whether to start a new segment. Splitting very long
                recordings keeps transcription quality up; choose Keep Recording to carry on.
            </p>
        </div>

        <div className="help-section">
            <h3>Calendar</h3>
            <p>
                With Calendar access, noFriction links a recording to the calendar event happening when you start,
                so it gets the meeting title and attendees, and shows upcoming meetings in INTEL.
            </p>
        </div>

        <div className="help-section">
            <h3>Obsidian Export</h3>
            <p>
                Point noFriction at your Obsidian vault in <strong>Settings → Obsidian</strong> and turn on
                Auto-Export to save each meeting as Markdown in a <code>noFriction</code> folder inside the vault
                when recording stops.
            </p>
        </div>
    </>
);

/* ═══════════════════════════════════════════════════════════════════════
   HOW-TO GUIDES
   ═══════════════════════════════════════════════════════════════════════ */
const HowTo: React.FC<Caps> = ({ caps }) => (
    <>
        <div className="help-section">
            <h3>How to Record a Meeting</h3>
            <ol>
                <li>Pick your microphone in <strong>Settings → General → Microphone</strong> (optional; the system default is used otherwise).</li>
                <li>Leave <strong>Capture System Audio</strong> on (Settings → General) to record the other people on the call.</li>
                <li>Click <strong>START CAPTURE</strong> in the top-right, press ⌘N, or use the menu-bar icon. The status pill shows <strong>RECORDING</strong>.</li>
                <li>The live transcript appears in <strong>LIVE</strong>.</li>
                <li>Click <strong>STOP CAPTURE</strong> (⌘.), or let auto-stop end it when the meeting is over.</li>
                <li>For recordings over 6 minutes, AI notes are written automatically if an AI provider is set up and <strong>Write a report after each meeting</strong> is on.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>How to Set Up AI (Bring Your Own Key)</h3>
            <ol>
                <li>Open <strong>Settings → AI Engine</strong>.</li>
                <li>Paste an API key from your provider. noFriction detects the provider, checks the key, and picks a model (you can change it).</li>
                <li>The first time, confirm <strong>"Send meeting content to {"{Provider}"}?"</strong>. Nothing is sent until you allow it.</li>
                <li>Choose whether to get <strong>Live insights during meetings</strong> and <strong>Write a report after each meeting</strong> (both on by default).</li>
                <li>Prefer to stay offline? Use a local server (Ollama, LM Studio, or any OpenAI-compatible server) under
                    <strong> Local &amp; custom servers</strong>, or Apple's on-device model on macOS 26 with Apple Intelligence on (no key).</li>
            </ol>
            <ProNote caps={caps} />
        </div>

        <div className="help-section">
            <h3>How to View and Regenerate Notes</h3>
            <ol>
                <li>Go to <strong>REWIND</strong> and select a meeting.</li>
                <li>Switch to <strong>Notes</strong> to see the summary, key topics, decisions and action items.</li>
                <li>Click <strong>Generate notes</strong> (no notes yet) or <strong>Regenerate</strong> (to redo them, for example after an edit).</li>
                <li>For a follow-up email, use <strong>Follow-up email</strong> on the meeting, then Copy or Open in Mail.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>How to Delete or Strike Something</h3>
            <ol>
                <li>Go to <strong>REWIND</strong>, select a meeting, and find the words or screenshots in its Rewind view.</li>
                <li>Click a word (shift-click or drag for more), or ⌘-click screenshots / use <strong>Select screens</strong>.</li>
                <li>Choose <strong>Delete</strong> (5-second Undo) or <strong>Strike from the record…</strong> (permanent, leaves a marker).</li>
                <li>Press <strong>Esc</strong> to clear the selection.</li>
            </ol>
            <p>To delete a whole meeting, use the delete button next to it in the REWIND list.</p>
        </div>

        <div className="help-section">
            <h3>How to Use Chat</h3>
            <ol>
                <li>Open <strong>CHAT</strong> (⇧⌘I) and type a question about your meetings.</li>
                <li>Press Enter. The answer lists the meetings and passages it used.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>How to Use Prompt Studio</h3>
            <ol>
                <li>Open <strong>MORE → PROMPTS</strong>.</li>
                <li>Filter by category (All, Intelligence, Meeting, VLM) and persona (Prospecting, Fundraising, Product Dev, Admin, Personal).</li>
                <li>Select a prompt and edit its name, description, prompt text and temperature, or toggle it Active.</li>
                <li>Click <strong>✓ Save</strong>. Use <strong>⧉ Duplicate</strong> to make a variant, and the <strong>Test</strong> panel to try it on sample text.</li>
            </ol>
            <div className="help-callout">
                <p><strong>Tip:</strong> To change the format of your meeting notes, edit the <strong>meeting_report</strong> prompt
                    for your persona (Meeting category). It's used when you generate or regenerate notes.</p>
            </div>
        </div>

        <div className="help-section">
            <h3>How to Export to Obsidian</h3>
            <ol>
                <li>Open <strong>Settings → Obsidian</strong> and choose your vault folder, then click Save.</li>
                <li>Turn on <strong>Auto-Export Meetings</strong> to save each meeting to the vault when recording stops.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>How to Get an Attendee Briefing</h3>
            <ol>
                <li>Open <strong>INTEL</strong>; upcoming calendar meetings are listed.</li>
                <li>Click <strong>Lookup</strong> on a meeting to have your AI provider write a briefing on its attendees.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>How to Change Transcription</h3>
            <ol>
                <li>Open <strong>Settings → Transcription</strong>.</li>
                <li><strong>Local Whisper</strong> (the default) runs on this Mac, offline, with no key. Download a model once; larger models are more accurate but slower.</li>
                <li>Optionally switch to a cloud service (Deepgram, Google Gemini Live, Gladia, or Google Cloud Speech-to-Text) with your own key. Meeting audio is then sent to that service.</li>
            </ol>
        </div>
    </>
);

/* ═══════════════════════════════════════════════════════════════════════
   HOW IT WORKS
   ═══════════════════════════════════════════════════════════════════════ */
const TechSpec: React.FC<Caps> = ({ caps }) => (
    <>
        <div className="help-section">
            <h3>Overview</h3>
            <p>
                noFriction Meetings is a native Mac app. Everything is stored in a local SQLite database on this Mac,
                with full-text search. There is no noFriction account and no noFriction server.
            </p>
            <div className="help-code">
                Transcription: Whisper on this Mac (default); Deepgram, Gemini, Gladia or Google STT optional with your key{"\n"}
                AI: your provider and key (OpenAI, Anthropic, Gemini, xAI, Groq, OpenRouter, Mistral,{"\n"}
                {"    "}DeepSeek, Perplexity, Together), a local server, or Apple's on-device model{"\n"}
                Screens: periodic screenshots; unchanged screens are skipped{"\n"}
                Calendar: Apple Calendar (EventKit){"\n"}
                Secrets: macOS Keychain
            </div>
        </div>

        <div className="help-section">
            <h3>Transcription</h3>
            <ol>
                <li><strong>Capture</strong>: your microphone and (optionally) system audio from the call app.</li>
                <li><strong>Transcribe</strong>: by default, Whisper runs on this Mac. If you pick a cloud service, the audio is streamed to it over an encrypted connection.</li>
                <li><strong>Store</strong>: transcript lines are saved locally and indexed for search.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>Screen Capture</h3>
            <p>
                While recording, noFriction takes screenshots (about once a second) and keeps only those where the
                screen changed. They build the visual timeline in Rewind.
                {caps && !caps.video_recording && " The Mac App Store version records screenshots only, not screen video."}
                {caps?.video_recording && " This version can also record screen video."}
            </p>
        </div>

        <div className="help-section">
            <h3>How Notes Are Made</h3>
            <ol>
                <li><strong>Trigger</strong>: automatically when you stop a recording longer than 6 minutes (if an AI provider is ready and "Write a report after each meeting" is on), or when you click Generate notes / Regenerate.</li>
                <li><strong>Prompt</strong>: the transcript is combined with the notes prompt (editable in PROMPTS). Stricken passages appear only as "[stricken from the record]".</li>
                <li><strong>Send</strong>: the request goes straight from this Mac to the provider you chose in Settings → AI Engine, using your key, after you've allowed that provider. With Apple's on-device model or a local server it never leaves your Mac or network.</li>
                <li><strong>Save</strong>: the notes are stored with the meeting and shown in its Notes view.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>Chat</h3>
            <p>
                Your question is matched against your meetings with local full-text search. The best-matching
                passages are sent with the question to your AI provider, and the answer lists its sources.
            </p>
        </div>

        <div className="help-section">
            <h3>Settings Storage</h3>
            <p>
                Settings are stored in the local database. API keys (AI and cloud transcription) are stored only in the
                macOS Keychain; the app shows just the last 4 characters.
            </p>
        </div>

        <div className="help-section">
            <h3>Prompts and Personas</h3>
            <p>
                Each AI feature uses a prompt you can edit in PROMPTS. Prompts come in versions for five personas
                (Prospecting, Fundraising, Product Dev, Admin, Personal); the active persona's version is used,
                with a built-in default as a fallback.
            </p>
        </div>
    </>
);

/* ═══════════════════════════════════════════════════════════════════════
   PRIVACY & SECURITY
   ═══════════════════════════════════════════════════════════════════════ */
const SecurityManual: React.FC<Caps> = ({ caps }) => (
    <>
        <div className="help-section">
            <h3>Data Privacy & Local Storage</h3>
            <div className="help-callout security">
                <p><strong>Your data is stored only on this Mac.</strong> Recordings, transcripts, screenshots and meeting
                    notes live in a local database. There is no noFriction server or cloud database. The only
                    network calls go directly to the AI or cloud-transcription provider you set up with your own key,
                    and the app asks before sending anything to an AI provider for the first time.</p>
            </div>
            <p>Your data folder:</p>
            <div className="help-code">
                {caps?.sandboxed
                    ? "~/Library/Containers/com.nofriction.meetings/Data/Library/Application Support/com.nofriction.meetings/"
                    : "~/Library/Application Support/com.nofriction.meetings/"}
                {"\n"}Database: nofriction_meetings.db
            </div>
        </div>

        <div className="help-section">
            <h3>API Key Security</h3>
            <ul>
                <li><strong>Where keys live</strong>: every API key (AI providers and cloud transcription) is stored in
                    the macOS Keychain, never in the database or in files. Settings only ever shows the last 4 characters.</li>
                <li><strong>Where keys go</strong>: each key is sent only to its own provider, over HTTPS (local models
                    can use http on your own network). Redirects are refused, so a key can't be forwarded elsewhere.</li>
                <li><strong>Removing a key</strong>: Settings → AI Engine → Saved providers → Remove, which deletes it from the Keychain.</li>
            </ul>
        </div>

        <div className="help-section">
            <h3>Consent Before Sending</h3>
            <p>
                Before the first request to a cloud AI provider, noFriction asks <strong>"Send meeting content to
                {" {Provider}"}?"</strong> and explains what will be sent. You approve each provider once.
                Settings → AI Engine shows what leaves this device for the active provider and lets you revoke the
                permission. Local servers and Apple's on-device model don't need this, because nothing leaves your Mac.
            </p>
        </div>

        <div className="help-section">
            <h3>Network Communication</h3>
            <table className="help-table">
                <thead>
                    <tr><th>Service</th><th>Protocol</th><th>Data Sent</th></tr>
                </thead>
                <tbody>
                    <tr><td>Your AI provider (only once you've added a key and allowed it)</td><td>HTTPS</td><td>Transcript text, meeting title, attendee names; screenshots for screen features</td></tr>
                    <tr><td>Cloud transcription (only if you choose Deepgram, Gemini, Gladia or Google)</td><td>WSS / HTTPS</td><td>Meeting audio</td></tr>
                    <tr><td>Hugging Face (once)</td><td>HTTPS</td><td>Nothing; downloads the Whisper model</td></tr>
                    {caps?.storekit && <tr><td>Apple App Store</td><td>HTTPS</td><td>Subscription purchase and status (handled by Apple)</td></tr>}
                </tbody>
            </table>
            <p>
                Cloud services are always reached over TLS. With local Whisper and a local or Apple on-device model,
                nothing leaves this Mac.
            </p>
        </div>

        <div className="help-section">
            <h3>Permissions</h3>
            <table className="help-table">
                <thead>
                    <tr><th>Permission</th><th>Why</th></tr>
                </thead>
                <tbody>
                    <tr><td>Microphone</td><td>Recording your voice for transcription</td></tr>
                    <tr><td>Screen Recording</td><td>Screenshots for the visual timeline, and system audio from your call app</td></tr>
                    <tr><td>Calendar</td><td>Meeting titles, attendees and end times</td></tr>
                    <tr><td>Notifications</td><td>The "meeting seems to have ended" alert (optional)</td></tr>
                    {caps?.accessibility_capture && <tr><td>Accessibility</td><td>Reading on-screen text from other apps (optional; not in the Mac App Store version)</td></tr>}
                </tbody>
            </table>
        </div>

        <div className="help-section">
            <h3>Data Retention & Deletion</h3>
            <p>
                Your data stays until you delete it. Delete a whole meeting with the delete button in the REWIND
                list. To remove part of a meeting everywhere it appears, use Delete or Strike from the record
                (see Help Guide). <strong>Settings → Data</strong> has storage cleanup and an export of your data.
            </p>
            <p>
                To remove everything, quit the app and delete the data folder shown above. API keys can be removed in
                Settings → AI Engine (or in Keychain Access).
            </p>
        </div>
    </>
);

/* ═══════════════════════════════════════════════════════════════════════
   SERVICES
   ═══════════════════════════════════════════════════════════════════════ */
const ServicesManual: React.FC<Caps> = ({ caps }) => (
    <>
        <div className="help-section">
            <h3>Nothing Is Required</h3>
            <div className="help-callout">
                <p><strong>No account and no third-party service is required.</strong> Recording and transcription work
                    offline with local Whisper. Each service below is optional and uses your own account and key.</p>
            </div>
        </div>

        <div className="help-section">
            <h3>AI Providers <span className="help-version">OPTIONAL</span></h3>
            <table className="help-table">
                <tbody>
                    <tr><td><strong>Cloud (your key)</strong></td><td>OpenAI, Anthropic, Google Gemini, xAI Grok, Groq, OpenRouter, Mistral, DeepSeek, Perplexity, Together AI</td></tr>
                    <tr><td><strong>Local (no key)</strong></td><td>Ollama, LM Studio, or any OpenAI-compatible server</td></tr>
                    <tr><td><strong>On-device</strong></td><td>Apple's model on macOS 26 with Apple Intelligence on (no key, never leaves your Mac)</td></tr>
                    <tr><td><strong>Used For</strong></td><td>Meeting notes, follow-up emails, live insights, Chat answers, attendee briefings, screen analysis</td></tr>
                    <tr><td><strong>Billing</strong></td><td>By your provider, under your account; their privacy policy and terms apply</td></tr>
                    <tr><td><strong>Setup</strong></td><td>Settings → AI Engine → paste your key</td></tr>
                </tbody>
            </table>
            <ProNote caps={caps} />
        </div>

        <div className="help-section">
            <h3>Cloud Transcription <span className="help-version">OPTIONAL</span></h3>
            <table className="help-table">
                <tbody>
                    <tr><td><strong>Default</strong></td><td>Local Whisper on this Mac (no service used)</td></tr>
                    <tr><td><strong>Alternatives</strong></td><td>Deepgram (Nova-3), Google Gemini Live, Gladia, Google Cloud Speech-to-Text (Chirp 2)</td></tr>
                    <tr><td><strong>Data Sent</strong></td><td>Meeting audio, only while that service is selected</td></tr>
                    <tr><td><strong>Setup</strong></td><td>Settings → Transcription → choose a service and enter its key</td></tr>
                </tbody>
            </table>
        </div>

        <div className="help-section">
            <h3>Apple Calendar <span className="help-version">OPTIONAL</span></h3>
            <table className="help-table">
                <tbody>
                    <tr><td><strong>Used For</strong></td><td>Meeting titles and attendees, upcoming meetings, and detecting when a meeting ends</td></tr>
                    <tr><td><strong>Setup</strong></td><td>Allow Calendar access when asked (or in System Settings → Privacy &amp; Security → Calendars)</td></tr>
                </tbody>
            </table>
        </div>

        <div className="help-section">
            <h3>Obsidian Vault <span className="help-version">OPTIONAL</span></h3>
            <table className="help-table">
                <tbody>
                    <tr><td><strong>Integration Type</strong></td><td>Local files only (writes Markdown into the folder you choose)</td></tr>
                    <tr><td><strong>Used For</strong></td><td>Exporting meetings; the VAULT and INTEL views read it back</td></tr>
                    <tr><td><strong>Setup</strong></td><td>Settings → Obsidian → choose your vault, turn on Auto-Export</td></tr>
                </tbody>
            </table>
        </div>

        <div className="help-section">
            <h3>Checking a Service</h3>
            <p>
                Use <strong>Test</strong> next to a saved provider in Settings → AI Engine to check an AI key. If cloud
                transcription can't connect, the LIVE view shows "Transcription stopped" with the reason.
            </p>
        </div>

        <div className="help-section">
            <h3>Support</h3>
            <p>
                Questions or problems? Email <strong>support@nofriction.ai</strong>, or use Help → Contact Support… in the menu bar. Version, Privacy Policy and Terms of
                Use are in <strong>Settings → About</strong>.
            </p>
        </div>
    </>
);
