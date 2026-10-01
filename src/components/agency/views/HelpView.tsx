import React, { useState } from "react";
import "./HelpView.css";

type HelpTab = "tech" | "howto" | "guide" | "security" | "services";

export const HelpView: React.FC = () => {
    const [activeTab, setActiveTab] = useState<HelpTab>("guide");

    const tabs: { id: HelpTab; label: string; icon: string }[] = [
        { id: "guide", label: "Help Guide", icon: "📖" },
        { id: "howto", label: "How-To", icon: "🛠️" },
        { id: "tech", label: "Tech Spec", icon: "⚙️" },
        { id: "security", label: "Security", icon: "🔒" },
        { id: "services", label: "Services", icon: "☁️" },
    ];

    return (
        <div className="help-view">
            <div className="help-header">
                <h2>noFriction Documentation</h2>
                <p>Everything you need to know about capturing, analyzing, and managing your meetings.</p>
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
                {activeTab === "guide" && <HelpGuide />}
                {activeTab === "howto" && <HowTo />}
                {activeTab === "tech" && <TechSpec />}
                {activeTab === "security" && <SecurityManual />}
                {activeTab === "services" && <ServicesManual />}
            </div>
        </div>
    );
};

/* ═══════════════════════════════════════════════════════════════════════
   HELP GUIDE
   ═══════════════════════════════════════════════════════════════════════ */
const HelpGuide: React.FC = () => (
    <>
        <div className="help-section">
            <h3>Getting Started</h3>
            <p>
                noFriction Meetings is your AI-powered meeting intelligence platform. It captures
                audio from microphone and system sources, produces real-time transcriptions via
                Deepgram, and generates structured meeting reports using AI.
            </p>
            <div className="help-callout">
                <p><strong>Quick Start:</strong> Click <strong>START CAPTURE</strong> in the top-right to begin
                    recording. noFriction automatically captures audio, transcribes it live, and
                    generates a meeting report when you stop (for sessions longer than 6 minutes).</p>
            </div>
        </div>

        <div className="help-section">
            <h3>Navigation Modes</h3>
            <table className="help-table">
                <thead>
                    <tr><th>Mode</th><th>Purpose</th></tr>
                </thead>
                <tbody>
                    <tr><td>🌊 FLOW</td><td>Live recording view — see transcription in real-time as it streams.</td></tr>
                    <tr><td>REWIND</td><td>Meeting history — browse past meetings, view reports, summaries, and action items.</td></tr>
                    <tr><td>🧘 ZEN</td><td>Minimal focus mode — distraction-free recording with ambient status indicators.</td></tr>
                    <tr><td>📚 VAULT</td><td>Document vault — export meetings to Obsidian, manage files, and tag recordings.</td></tr>
                    <tr><td>🔍 INTEL</td><td>Intelligence dashboard — attendee lookup and company research powered by AI.</td></tr>
                    <tr><td>💬 CHAT</td><td>Data chat — ask AI questions about your meeting history and get cited answers.</td></tr>
                    <tr><td>PROMPTS</td><td>Prompt Studio — view, edit, test, and duplicate all AI master prompts per persona.</td></tr>
                    <tr><td>📖 HELP</td><td>Documentation — you're here! Technical specifications, guides, and security info.</td></tr>
                </tbody>
            </table>
        </div>

        <div className="help-section">
            <h3>Genie Mode</h3>
            <p>
                While recording, click <strong>GENIE</strong> in the navbar to enter Genie Mode — a floating
                minimal overlay that lets you keep working while noFriction captures in the background.
                Genie Mode shows transcript snippets and recording status without taking up screen space.
            </p>
        </div>

        <div className="help-section">
            <h3>Meeting Reports</h3>
            <p>
                After every recording longer than <strong>6 minutes</strong>, noFriction automatically runs
                the transcript through AI to generate a structured meeting report including:
            </p>
            <ul>
                <li><strong>Executive Summary</strong> — concise overview of the entire meeting</li>
                <li><strong>Key Topics</strong> — major areas discussed</li>
                <li><strong>Decisions Made</strong> — explicit decisions with who made them</li>
                <li><strong>Action Items</strong> — tasks, assignees, and priorities</li>
                <li><strong>Participants</strong> — detected speakers and attendees</li>
            </ul>
            <div className="help-callout">
                <p><strong>Tip:</strong> You can now manage all AI prompts — including report, chat, live intel, and catch-up — in the <strong>PROMPTS</strong> tab. Each prompt can be customized per persona.</p>
            </div>
        </div>

        <div className="help-section">
            <h3>Data Chat — Asking AI About Your Meetings</h3>
            <p>
                The <strong>CHAT</strong> mode lets you ask natural language questions about all your
                recorded meetings. Examples:
            </p>
            <ul>
                <li>"What decisions were made in last Tuesday's standup?"</li>
                <li>"What did the team commit to regarding the Q2 deadline?"</li>
                <li>"Summarize all mentions of the budget across my meetings."</li>
                <li>"Who has action items from the last 3 meetings?"</li>
            </ul>
            <p>
                AI answers are grounded in your actual transcripts, with source citations linking
                back to the exact meeting and timestamp.
            </p>
        </div>

        <div className="help-section">
            <h3>Recording Segmentation</h3>
            <p>
                If a recording exceeds <strong>75 minutes</strong>, noFriction will show a native system alert
                asking if you'd like to start a new segment. Long recordings can degrade transcription
                accuracy, so segmenting is recommended.
            </p>
        </div>

        <div className="help-section">
            <h3>Calendar Integration</h3>
            <p>
                On macOS, noFriction integrates with Apple Calendar to automatically tag recordings
                with meeting titles and attendees. When you start a recording, the app checks your
                calendar for any currently active event and links it.
            </p>
        </div>

        <div className="help-section">
            <h3>Obsidian Vault Export</h3>
            <p>
                Enable auto-export in Settings to automatically push meeting notes to your Obsidian vault
                after every recording. Supports both default and Zettelkasten templates.
            </p>
        </div>
    </>
);

/* ═══════════════════════════════════════════════════════════════════════
   HOW-TO GUIDES
   ═══════════════════════════════════════════════════════════════════════ */
const HowTo: React.FC = () => (
    <>
        <div className="help-section">
            <h3>How to Record a Meeting</h3>
            <ol>
                <li>Launch noFriction Meetings.</li>
                <li>Select your preferred microphone in <strong>Settings → Audio</strong> (optional — defaults to system mic).</li>
                <li>Click <strong>START CAPTURE</strong> in the top-right of the navbar.</li>
                <li>The status pill will show <strong>LIVE INTELLIGENCE ACTIVE</strong>.</li>
                <li>Speak normally — transcripts appear in real-time in FLOW mode.</li>
                <li>Click <strong>STOP CAPTURE</strong> to end the session.</li>
                <li>If the recording was longer than 6 minutes, an AI meeting report is automatically generated.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>How to Use Data Chat</h3>
            <ol>
                <li>Navigate to the <strong>💬 CHAT</strong> tab.</li>
                <li>Type a natural language question in the input box.</li>
                <li>Press Enter or click Send.</li>
                <li>AI searches all your meeting transcripts and returns an answer with source citations.</li>
                <li>Click on a source badge to see the exact meeting and transcript segment.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>How to Use Prompt Studio</h3>
            <ol>
                <li>Navigate to the <strong>PROMPTS</strong> tab in the Agency view.</li>
                <li>Use the <strong>category tabs</strong> (All, Intelligence, Meeting, VLM) to filter by feature area.</li>
                <li>Use the <strong>persona dropdown</strong> to filter by persona (Prospecting, Fundraising, Product Dev, Admin, Personal).</li>
                <li>Click any prompt in the list to open it in the editor.</li>
                <li>Edit the <strong>display name</strong>, <strong>description</strong>, <strong>system prompt text</strong>, and <strong>temperature</strong> slider.</li>
                <li>Toggle <strong>Active</strong> on/off to enable or disable a prompt.</li>
                <li>Click <strong>✓ Save</strong> to persist changes.</li>
                <li>Use <strong>⧉ Duplicate</strong> to create a variant of any prompt.</li>
            </ol>
            <div className="help-callout">
                <p><strong>Tip:</strong> Expand the <strong>Test</strong> panel to run any prompt against sample input and see AI responses inline before deploying.</p>
            </div>
        </div>

        <div className="help-section">
            <h3>How to Customize the Meeting Report Prompt</h3>
            <ol>
                <li>Open the <strong>PROMPTS</strong> tab.</li>
                <li>Filter by <strong>Meeting</strong> category.</li>
                <li>Select the <strong>meeting_report</strong> prompt for your active persona.</li>
                <li>Edit the system prompt to match your team's report format.</li>
                <li>Click <strong>✓ Save</strong>.</li>
                <li>All future reports will use your custom prompt.</li>
            </ol>
            <div className="help-callout">
                <p><strong>Tip:</strong> Include format instructions like "use markdown headings" or "include a
                    risk assessment section" to tailor reports to your workflow.</p>
            </div>
        </div>

        <div className="help-section">
            <h3>How to Manually Generate a Report</h3>
            <ol>
                <li>Navigate to <strong>REWIND</strong> and select a meeting.</li>
                <li>Click <strong>Generate Report</strong> in the meeting detail view.</li>
                <li>The AI will process the transcript and add the report to the meeting record.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>How to Export to Obsidian</h3>
            <ol>
                <li>Open <strong>Settings → Obsidian</strong>.</li>
                <li>Set your vault path (the root folder of your Obsidian vault).</li>
                <li>Toggle <strong>Auto Export</strong> to enable automatic export after each recording.</li>
                <li>Choose a template: <strong>Default</strong> or <strong>Zettelkasten</strong>.</li>
                <li>Meetings will be exported as markdown files into your vault's Inbox folder.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>How to Use the Intel Dashboard</h3>
            <ol>
                <li>Navigate to <strong>🔍 INTEL</strong>.</li>
                <li>Enter an attendee's email or name to look up their profile.</li>
                <li>AI generates a briefing with company info, recent interactions, and context.</li>
                <li>Use briefings before meetings to prepare talking points.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>How to Configure Transcription</h3>
            <ol>
                <li>Open <strong>Settings → Transcription</strong>.</li>
                <li>Add your Deepgram API key (required for transcription).</li>
                <li>Select model — <code>nova-3</code> is recommended for best accuracy.</li>
                <li>Choose audio sources: microphone, system audio, or both.</li>
            </ol>
        </div>
    </>
);

/* ═══════════════════════════════════════════════════════════════════════
   TECHNICAL SPECIFICATION
   ═══════════════════════════════════════════════════════════════════════ */
const TechSpec: React.FC = () => (
    <>
        <div className="help-section">
            <h3>Architecture Overview</h3>
            <p>
                noFriction Meetings is a desktop application built with <strong>Tauri v2</strong> (Rust backend)
                and <strong>React</strong> (TypeScript frontend). Data is stored locally in an <strong>SQLite</strong> database
                with full-text search support via FTS5.
            </p>
            <div className="help-code">
                Frontend: React + TypeScript + Vite{"\n"}
                Backend: Rust (Tauri v2){"\n"}
                Database: SQLite (sqlx){"\n"}
                Transcription: Whisper on-device (default); Deepgram, Gemini, Gladia, Google optional with your key{"\n"}
                AI: your provider and key (OpenAI default; Anthropic, Gemini, xAI Grok, Groq, local models, …){"\n"}
                Screen Capture: xcap (native screen capturing){"\n"}
                Calendar: Apple EventKit (macOS native)
            </div>
        </div>

        <div className="help-section">
            <h3>Data Model</h3>
            <table className="help-table">
                <thead>
                    <tr><th>Table</th><th>Purpose</th><th>Key Fields</th></tr>
                </thead>
                <tbody>
                    <tr><td><code>meetings</code></td><td>Recording sessions</td><td>id, title, started_at, ended_at, duration_seconds, calendar_event_id</td></tr>
                    <tr><td><code>transcripts</code></td><td>Speech segments</td><td>meeting_id, text, speaker, timestamp, is_final, confidence</td></tr>
                    <tr><td><code>meeting_notes</code></td><td>AI-generated reports</td><td>meeting_id, summary, key_topics (JSON), decisions (JSON), action_items (JSON), participants (JSON), model_used</td></tr>
                    <tr><td><code>meeting_comments</code></td><td>User annotations</td><td>meeting_id, comment, comment_type, timestamp_ref</td></tr>
                    <tr><td><code>meeting_attendees</code></td><td>Calendar attendees</td><td>meeting_id, name, email, company, role</td></tr>
                    <tr><td><code>screen_states</code></td><td>Deduplicated screen captures</td><td>meeting_id, phash, delta_score, keyframe_path, state_type</td></tr>
                    <tr><td><code>document_episodes</code></td><td>Focus sessions on apps/documents</td><td>meeting_id, app_name, window_title, state_count</td></tr>
                    <tr><td><code>meeting_timeline_events</code></td><td>Timeline markers</td><td>meeting_id, event_type, title, topic, importance</td></tr>
                    <tr><td><code>topic_clusters</code></td><td>Topic groupings</td><td>meeting_id, name, event_count, total_duration_ms</td></tr>
                    <tr><td><code>settings</code></td><td>Key-value config</td><td>key, value, updated_at</td></tr>
                </tbody>
            </table>
        </div>

        <div className="help-section">
            <h3>Transcription Pipeline</h3>
            <ol>
                <li><strong>Audio Capture</strong>: System audio and/or microphone captured via native APIs.</li>
                <li><strong>WebSocket Stream</strong>: Raw audio streamed to Deepgram's nova-3 model over WebSocket.</li>
                <li><strong>Real-time Results</strong>: Deepgram returns interim and final transcript segments.</li>
                <li><strong>Deduplication</strong>: Transcript segments are hashed (SHA-256) and deduplicated before storage.</li>
                <li><strong>FTS Indexing</strong>: Finalized transcripts are indexed in an FTS5 virtual table for full-text search.</li>
                <li><strong>Keepalive</strong>: A periodic keepalive message is sent during silence to prevent WebSocket disconnection.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>AI Report Generation Flow</h3>
            <ol>
                <li><strong>Trigger</strong>: When <code>stop_recording</code> completes and duration &gt; 6 minutes.</li>
                <li><strong>Prompt Assembly</strong>: The stored master prompt (editable in Settings) is combined with the transcript text.</li>
                <li><strong>API Call</strong>: Sent straight from the app to the AI provider you chose in Settings → AI Engine, with your own key (after you've allowed that provider).</li>
                <li><strong>Parse</strong>: Response is parsed as JSON into structured <code>GeneratedNotes</code>.</li>
                <li><strong>Storage</strong>: Report is saved to the <code>meeting_notes</code> table.</li>
                <li><strong>Display</strong>: Report appears in REWIND view when viewing the meeting.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>Screen Capture Pipeline (Stateful Ingest)</h3>
            <ol>
                <li><strong>Frame Capture</strong>: Screenshots taken at configurable intervals (default 5s).</li>
                <li><strong>Perceptual Hashing</strong>: Each frame gets a pHash for deduplication.</li>
                <li><strong>State Building</strong>: Similar consecutive frames are merged into "screen states."</li>
                <li><strong>Episode Building</strong>: Continuous focus on the same app/window creates "episodes."</li>
                <li><strong>Timeline Generation</strong>: Significant changes trigger timeline events and topic clusters.</li>
            </ol>
        </div>

        <div className="help-section">
            <h3>Data Chat (Retrieval-Augmented Generation)</h3>
            <p>
                The CHAT mode uses a RAG pipeline: your question is used to search transcripts via FTS5,
                relevant segments are retrieved as context, and the AI generates an answer grounded in
                those segments. Each answer includes source citations (meeting ID, timestamp, relevance score).
            </p>
        </div>

        <div className="help-section">
            <h3>Settings Storage</h3>
            <p>
                All settings are stored in a SQLite <code>settings</code> table as key-value pairs.
                The <code>SettingsManager</code> serializes/deserializes typed settings fields.
            </p>
        </div>

        <div className="help-section">
            <h3>Prompt Management System</h3>
            <p>
                AI prompts are stored in the <code>prompts</code> table, managed by <code>PromptManager</code>.
                Each prompt has a <strong>category</strong> (intelligence, meeting, vlm), <strong>theme</strong> (persona),
                <strong>version</strong>, and <strong>active/inactive</strong> toggle. The system ships with 20 built-in
                prompts across 4 features × 5 personas. All AI surfaces resolve prompts by name with
                theme fallback.
            </p>
            <table className="help-table">
                <thead>
                    <tr><th>Feature</th><th>Prompt Pattern</th><th>Fallback</th></tr>
                </thead>
                <tbody>
                    <tr><td>Catch-Up Capsule</td><td><code>catch_up_capsule_&#123;persona&#125;</code></td><td>Hardcoded default</td></tr>
                    <tr><td>Meeting Report</td><td><code>meeting_report_&#123;persona&#125;</code></td><td>Settings prompt</td></tr>
                    <tr><td>Genie Chat</td><td><code>genie_system_&#123;persona&#125;</code></td><td>AIPreset::qa()</td></tr>
                    <tr><td>Live Intelligence</td><td><code>live_intel_system_&#123;persona&#125;</code></td><td>Rule-based only</td></tr>
                    <tr><td>VLM Analysis</td><td><code>&#123;theme&#125;_context_analysis</code></td><td>frame_analysis → hardcoded</td></tr>
                </tbody>
            </table>
        </div>
    </>
);

/* ═══════════════════════════════════════════════════════════════════════
   SECURITY MANUAL
   ═══════════════════════════════════════════════════════════════════════ */
const SecurityManual: React.FC = () => (
    <>
        <div className="help-section">
            <h3>Data Privacy & Local Storage</h3>
            <div className="help-callout security">
                <p><strong>Your data is stored only on this Mac.</strong> Recordings, transcripts, frames and meeting
                    reports live in a local SQLite database. There is no noFriction server or cloud database. The only
                    network calls go directly to the AI or cloud-transcription provider you set up with your own key,
                    and the app asks before sending anything to an AI provider for the first time.</p>
            </div>
            <p>
                The SQLite database is located in your system's app data directory:
            </p>
            <div className="help-code">
                macOS: ~/Library/Application Support/com.nofriction.meetings/nofriction_meetings.db
            </div>
        </div>

        <div className="help-section">
            <h3>API Key Security</h3>
            <ul>
                <li><strong>Where keys live</strong>: every API key (AI providers and cloud transcription) is stored in
                    the macOS Keychain, never in the database or in files. Settings only ever shows the last 4 characters.</li>
                <li><strong>Where keys go</strong>: each key is sent only to its own provider, over HTTPS (local models
                    can use http on your own network). Redirects are refused, so a key can't be forwarded elsewhere.</li>
                <li><strong>Removing a key</strong>: Settings → AI Engine → remove, which deletes it from the Keychain.</li>
            </ul>
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
                </tbody>
            </table>
            <p>
                Cloud services are always reached over TLS. With local Whisper and a local or Apple on-device model,
                nothing leaves this Mac.
            </p>
        </div>

        <div className="help-section">
            <h3>Permissions Required</h3>
            <table className="help-table">
                <thead>
                    <tr><th>Permission</th><th>Why</th></tr>
                </thead>
                <tbody>
                    <tr><td>Microphone</td><td>Audio capture for transcription</td></tr>
                    <tr><td>Screen Recording</td><td>Screen capture for visual timeline (optional)</td></tr>
                    <tr><td>Calendar (macOS)</td><td>Meeting title and attendee association</td></tr>
                    <tr><td>Accessibility (macOS)</td><td>Window title detection for episode tracking (optional)</td></tr>
                </tbody>
            </table>
        </div>

        <div className="help-section">
            <h3>Data Retention & Deletion</h3>
            <p>
                All data is retained indefinitely in local storage. You can delete individual meetings
                from the REWIND view — this cascades to delete all associated transcripts, frames,
                screen states, episodes, timeline events, and notes.
            </p>
            <p>
                To completely reset: delete the SQLite database file. The app will create a fresh
                database on next launch.
            </p>
        </div>

        <div className="help-section">
            <h3>Audit Logging</h3>
            <p>
                Administrative actions (deletions, exports, data modifications) are logged to the
                <code>audit_log</code> table with action type, target, timestamp, and bytes affected.
                Data field changes are versioned in the <code>data_versions</code> table.
            </p>
        </div>
    </>
);

/* ═══════════════════════════════════════════════════════════════════════
   DEPENDENT SERVICES MANUAL
   ═══════════════════════════════════════════════════════════════════════ */
const ServicesManual: React.FC = () => (
    <>
        <div className="help-section">
            <h3>Required Services</h3>
            <div className="help-callout">
                <p><strong>Only Deepgram is required</strong> for core functionality. All other
                    integrations are optional and enhance the experience.</p>
            </div>
        </div>

        <div className="help-section">
            <h3>Deepgram — Speech-to-Text <span className="help-version">REQUIRED</span></h3>
            <table className="help-table">
                <tbody>
                    <tr><td><strong>Website</strong></td><td>deepgram.com</td></tr>
                    <tr><td><strong>API Type</strong></td><td>WebSocket Streaming (WSS)</td></tr>
                    <tr><td><strong>Model Used</strong></td><td><code>nova-3</code> (configurable)</td></tr>
                    <tr><td><strong>Features Used</strong></td><td>Smart formatting, punctuation, utterance detection, speaker diarization</td></tr>
                    <tr><td><strong>Pricing</strong></td><td>Pay-per-minute; free tier available (~$200 credit)</td></tr>
                    <tr><td><strong>Setup</strong></td><td>Settings → enter API key</td></tr>
                </tbody>
            </table>
            <h4>Connection Details</h4>
            <div className="help-code">
                Endpoint: wss://api.deepgram.com/v1/listen{"\n"}
                Auth: Token (API key in query param){"\n"}
                Encoding: linear16 (PCM 16-bit){"\n"}
                Sample Rate: 16000 Hz{"\n"}
                Channels: 1 (mono)
            </div>
        </div>

        <div className="help-section">
            <h3>AI Engine — Report Generation <span className="help-version">LOCAL</span></h3>
            <table className="help-table">
                <tbody>
                    <tr><td><strong>Endpoint</strong></td><td><code>http://localhost:11434</code> (local Ollama; remote optional)</td></tr>
                    <tr><td><strong>API Type</strong></td><td>REST (Ollama API)</td></tr>
                    <tr><td><strong>Model</strong></td><td><code>qwen3:8b</code> (Sage — deep reasoning)</td></tr>
                    <tr><td><strong>Auth</strong></td><td>None for local Ollama (bearer token for remote hosts)</td></tr>
                    <tr><td><strong>Used For</strong></td><td>Meeting report generation, Data Chat answers, attendee intelligence</td></tr>
                </tbody>
            </table>
            <h4>Endpoints Used</h4>
            <div className="help-code">
                POST /v1/chat/completions — AI chat and report generation{"\n"}
                GET  /api/models          — List available models{"\n"}
                GET  /api/tags            — Check API availability
            </div>
        </div>

        <div className="help-section">
            <h3>Apple Calendar (EventKit) <span className="help-version">macOS ONLY</span></h3>
            <table className="help-table">
                <tbody>
                    <tr><td><strong>API Type</strong></td><td>Native macOS framework</td></tr>
                    <tr><td><strong>Used For</strong></td><td>Auto-tagging recordings with meeting titles and attendees</td></tr>
                    <tr><td><strong>Setup</strong></td><td>Grant calendar access when prompted on first launch</td></tr>
                </tbody>
            </table>
        </div>

        <div className="help-section">
            <h3>Obsidian Vault — Note Export <span className="help-version">OPTIONAL</span></h3>
            <table className="help-table">
                <tbody>
                    <tr><td><strong>Integration Type</strong></td><td>Local filesystem (writes markdown files)</td></tr>
                    <tr><td><strong>Used For</strong></td><td>Exporting meeting notes as Obsidian-compatible markdown</td></tr>
                    <tr><td><strong>Templates</strong></td><td>Default, Zettelkasten</td></tr>
                    <tr><td><strong>Setup</strong></td><td>Settings → Obsidian → set vault path, enable auto-export</td></tr>
                </tbody>
            </table>
        </div>

        <div className="help-section">
            <h3>Service Health Monitoring</h3>
            <p>
                To check if the AI service is reachable, navigate to <strong>CHAT</strong> and send any message.
                If the service is unavailable, you'll receive an "AI service unavailable" message.
                Deepgram connectivity is verified when starting a recording — if the WebSocket
                connection fails, an error notification appears.
            </p>
        </div>
    </>
);
