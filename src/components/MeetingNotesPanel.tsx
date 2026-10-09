// Notes for one recording (Recordings → Notes): the saved AI notes (made
// automatically after a recording over 6 minutes, or with Make notes), the
// "made before an edit" banner, (meetings) a follow-up email draft, and the
// Topics under the notes. The notes' style follows the recording's type:
// meeting notes, lecture notes (class) or personal notes.

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import * as tauri from "../lib/tauri";
import { aiErrorClass, friendlyAiError, isNoProviderError, withAiConsent } from "../lib/ai";
import { useCapabilities } from "../lib/build";
import { AiSetupNotice, useAiStatus } from "./AiSetupNotice";
import { notesLayout, notesStyleHint } from "../lib/recordingKind";
import { useRecordingKind } from "../hooks/useRecordingKind";
import ErrorState from "./ErrorState";
import { TopicsEditor } from "./TopicChips";
import "./MeetingNotesPanel.css";

interface SavedNotes {
    id: string;
    meeting_id: string;
    summary: string | null;
    key_topics: string | null;
    decisions: string | null;
    action_items: string | null;
    generated_at: string;
    model_used: string | null;
    stale_after_edit?: boolean;
}

interface FollowUpEmail {
    subject: string;
    body: string;
    to: string[];
}

type Decision = { text: string; made_by?: string | null; context?: string | null };
type ActionItem = { task: string; assignee?: string | null; due_date?: string | null };

function parseList<T>(json: string | null): T[] {
    if (!json) return [];
    try {
        const v = JSON.parse(json);
        return Array.isArray(v) ? (v as T[]) : [];
    } catch {
        return [];
    }
}

/** Error text for a failed AI action: never a raw machine prefix. */
function aiFailure(e: unknown, what: string): string {
    switch (aiErrorClass(e)) {
        case "pro_required":
            return `${what} are part of noFriction Pro.`;
        case "consent_required":
            return `${what} need your permission to send this recording to your AI provider. Try again and choose Allow.`;
        default:
            return friendlyAiError(e);
    }
}

export function MeetingNotesPanel({ meetingId }: { meetingId: string }) {
    const [notes, setNotes] = useState<SavedNotes | null>(null);
    const [loading, setLoading] = useState(true);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [generating, setGenerating] = useState(false);
    const [actionError, setActionError] = useState<string | null>(null);
    const [needsAi, setNeedsAi] = useState(false);
    const [email, setEmail] = useState<FollowUpEmail | null>(null);
    const [drafting, setDrafting] = useState(false);
    const { configured } = useAiStatus();
    const caps = useCapabilities();
    const kind = useRecordingKind(meetingId);
    // A provider was just connected in Settings: clear the earlier failure
    useEffect(() => {
        if (configured) setNeedsAi(false);
    }, [configured]);

    const load = useCallback(async () => {
        setLoading(true);
        setLoadError(null);
        try {
            setNotes(await invoke<SavedNotes | null>("get_meeting_notes", { meetingId }));
        } catch (e) {
            setLoadError(String(e));
        } finally {
            setLoading(false);
        }
    }, [meetingId]);

    useEffect(() => {
        setNotes(null);
        setEmail(null);
        setActionError(null);
        setNeedsAi(false);
        load();
    }, [load]);

    const generate = async () => {
        setGenerating(true);
        setActionError(null);
        setNeedsAi(false);
        try {
            // The notes prompt follows the recording's type (recording_kind.rs)
            await tauri.generateMeetingReport(meetingId);
            await load();
        } catch (e) {
            if (isNoProviderError(e)) setNeedsAi(true);
            else setActionError(aiFailure(e, "Notes"));
        } finally {
            setGenerating(false);
        }
    };

    const draftEmail = async () => {
        setDrafting(true);
        setActionError(null);
        setNeedsAi(false);
        try {
            setEmail(await withAiConsent(() => invoke<FollowUpEmail>("draft_followup_email", { meetingId })));
        } catch (e) {
            if (isNoProviderError(e)) setNeedsAi(true);
            else setActionError(aiFailure(e, "Follow-up emails"));
        } finally {
            setDrafting(false);
        }
    };

    if (loading && !notes) {
        return (
            <div className="mn-panel mn-center">
                <div className="loading-spinner" />
            </div>
        );
    }

    if (loadError) {
        return (
            <div className="mn-panel">
                <ErrorState title="Couldn't load the notes" message="Try again in a moment." onRetry={load} />
            </div>
        );
    }

    const proNote = caps?.pro_gating ? " (noFriction Pro)" : "";
    const showSetup = configured === false || needsAi;

    const actions = (
        <div className="mn-actions">
            <button className="mn-btn primary" onClick={generate} disabled={generating || showSetup}>
                {generating ? "Making notes…" : notes ? "Make again" : "Make notes"}
            </button>
            {kind === "meeting" && (
                <button className="mn-btn" onClick={draftEmail} disabled={drafting || showSetup}>
                    {drafting ? "Drafting…" : `Follow-up email${proNote}`}
                </button>
            )}
        </div>
    );

    return (
        <div className="mn-panel">
            {showSetup && <AiSetupNotice feature={kind === "meeting" ? "Notes and follow-up emails" : "Notes"} />}
            {actionError && (
                <p className="mn-error" role="alert">
                    {actionError}
                </p>
            )}

            {!notes ? (
                <div className="mn-empty">
                    <h3>No notes yet</h3>
                    <p>
                        Notes are made from the transcript by the AI you set up{proNote}, on their own when a recording
                        longer than 6 minutes stops (Settings → AI). {notesStyleHint(kind)}
                    </p>
                    {actions}
                </div>
            ) : (
                <div className="mn-notes">
                    {notes.stale_after_edit && (
                        <div className="rd-stale" role="status">
                            <span>These notes were made before an edit.</span>
                            <button className="rd-btn" onClick={generate} disabled={generating || showSetup}>
                                {generating ? "Making notes…" : "Make again"}
                            </button>
                        </div>
                    )}
                    <NotesBody notes={notes} />
                    <p className="mn-meta">
                        Made {new Date(notes.generated_at).toLocaleString()}
                    </p>
                    {actions}
                </div>
            )}
            <TopicsEditor meetingId={meetingId} />

            {email && <FollowUpSheet email={email} onClose={() => setEmail(null)} />}
        </div>
    );
}

/** The layout follows the prompt that wrote the notes (`model_used`), so
 *  notes written before a type change keep their headings until regenerated. */
function NotesBody({ notes }: { notes: SavedNotes }) {
    const topics = parseList<string>(notes.key_topics);
    const decisions = parseList<Decision>(notes.decisions);
    const actions = parseList<ActionItem>(notes.action_items);
    const layout = notesLayout(notes.model_used);
    if (layout === "lecture") {
        return <LectureNotesBody summary={notes.summary} concepts={topics} definitions={decisions} announcements={actions} />;
    }
    if (layout === "personal") {
        return <PersonalNotesBody summary={notes.summary} points={topics} todos={actions} />;
    }
    return (
        <>
            <section className="mn-section">
                <h4>Summary</h4>
                <p>{notes.summary?.trim() || "No summary."}</p>
            </section>
            {topics.length > 0 && (
                <section className="mn-section">
                    <h4>Key topics</h4>
                    <ul>
                        {topics.map((t, i) => (
                            <li key={i}>{t}</li>
                        ))}
                    </ul>
                </section>
            )}
            <section className="mn-section">
                <h4>Decisions</h4>
                {decisions.length === 0 ? (
                    <p className="mn-muted">None recorded.</p>
                ) : (
                    <ul>
                        {decisions.map((d, i) => (
                            <li key={i}>
                                {d.text}
                                {d.made_by && <span className="mn-muted"> — {d.made_by}</span>}
                            </li>
                        ))}
                    </ul>
                )}
            </section>
            <section className="mn-section">
                <h4>Action items</h4>
                {actions.length === 0 ? (
                    <p className="mn-muted">None recorded.</p>
                ) : (
                    <ul>
                        {actions.map((a, i) => (
                            <li key={i}>
                                {a.task}
                                <span className="mn-muted">
                                    {" "}
                                    — {a.assignee || "owner not stated"}
                                    {a.due_date ? ` · ${a.due_date}` : ""}
                                </span>
                            </li>
                        ))}
                    </ul>
                )}
            </section>
        </>
    );
}

/** Same stored shape, framed for a class: concepts, definitions with examples, announcements. */
function LectureNotesBody({
    summary,
    concepts,
    definitions,
    announcements,
}: {
    summary: string | null;
    concepts: string[];
    definitions: Decision[];
    announcements: ActionItem[];
}) {
    return (
        <>
            <section className="mn-section">
                <h4>Lecture summary</h4>
                <p>{summary?.trim() || "No summary."}</p>
            </section>
            <section className="mn-section">
                <h4>Key concepts</h4>
                {concepts.length === 0 ? (
                    <p className="mn-muted">None recorded.</p>
                ) : (
                    <ul>
                        {concepts.map((t, i) => (
                            <li key={i}>{t}</li>
                        ))}
                    </ul>
                )}
            </section>
            <section className="mn-section">
                <h4>Definitions and examples</h4>
                {definitions.length === 0 ? (
                    <p className="mn-muted">None recorded.</p>
                ) : (
                    <ul>
                        {definitions.map((d, i) => (
                            <li key={i}>
                                {d.text}
                                {d.context && <span className="mn-muted"> — e.g. {d.context}</span>}
                            </li>
                        ))}
                    </ul>
                )}
            </section>
            <section className="mn-section">
                <h4>Announcements and deadlines</h4>
                {announcements.length === 0 ? (
                    <p className="mn-muted">None mentioned.</p>
                ) : (
                    <ul>
                        {announcements.map((a, i) => (
                            <li key={i}>
                                {a.task}
                                {a.due_date && <span className="mn-muted"> — {a.due_date}</span>}
                            </li>
                        ))}
                    </ul>
                )}
            </section>
        </>
    );
}

/** Same stored shape, framed for a personal recording: key points, to-dos and reminders. */
function PersonalNotesBody({ summary, points, todos }: { summary: string | null; points: string[]; todos: ActionItem[] }) {
    return (
        <>
            <section className="mn-section">
                <h4>Summary</h4>
                <p>{summary?.trim() || "No summary."}</p>
            </section>
            <section className="mn-section">
                <h4>Key points</h4>
                {points.length === 0 ? (
                    <p className="mn-muted">None recorded.</p>
                ) : (
                    <ul>
                        {points.map((t, i) => (
                            <li key={i}>{t}</li>
                        ))}
                    </ul>
                )}
            </section>
            <section className="mn-section">
                <h4>To-dos and reminders</h4>
                {todos.length === 0 ? (
                    <p className="mn-muted">None mentioned.</p>
                ) : (
                    <ul>
                        {todos.map((a, i) => (
                            <li key={i}>
                                {a.task}
                                {a.due_date && <span className="mn-muted"> — {a.due_date}</span>}
                            </li>
                        ))}
                    </ul>
                )}
            </section>
        </>
    );
}

function FollowUpSheet({ email, onClose }: { email: FollowUpEmail; onClose: () => void }) {
    const [subject, setSubject] = useState(email.subject);
    const [body, setBody] = useState(email.body);
    const [copied, setCopied] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [onClose]);

    const copy = async () => {
        try {
            await navigator.clipboard.writeText(`Subject: ${subject}\n\n${body}`);
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
        } catch {
            setError("Couldn't copy. Select the text and press ⌘C.");
        }
    };

    const openInMail = () => {
        const to = email.to.map(encodeURIComponent).join(",");
        const url = `mailto:${to}?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body)}`;
        openUrl(url).catch(() => setError("Couldn't open Mail. Copy the email instead."));
    };

    return (
        <div className="modal-overlay mn-sheet-overlay" role="dialog" aria-modal="true" aria-labelledby="mn-sheet-title" onClick={onClose}>
            <div className="mn-sheet" onClick={(e) => e.stopPropagation()}>
                <h3 id="mn-sheet-title">Follow-up email</h3>
                <p className="mn-muted">
                    Drafted by your AI from the transcript. Check it before sending.
                    {email.to.length > 0 ? ` To: ${email.to.join(", ")}` : ""}
                </p>
                <label className="mn-label" htmlFor="mn-subject">Subject</label>
                <input id="mn-subject" className="mn-input" value={subject} onChange={(e) => setSubject(e.target.value)} />
                <label className="mn-label" htmlFor="mn-body">Message</label>
                <textarea id="mn-body" className="mn-textarea" value={body} onChange={(e) => setBody(e.target.value)} rows={14} />
                {error && <p className="mn-error">{error}</p>}
                <div className="mn-actions end">
                    <button className="mn-btn" onClick={onClose}>Close</button>
                    <button className="mn-btn" onClick={copy}>{copied ? "Copied" : "Copy"}</button>
                    <button className="mn-btn primary" onClick={openInMail}>Open in Mail</button>
                </div>
            </div>
        </div>
    );
}
