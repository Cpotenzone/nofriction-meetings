// noFriction Meetings - Notes for one recording (Recordings → Notes)
// Shows the saved AI notes (written automatically after meetings over 6
// minutes, or on demand), with Generate / Regenerate, the "made before an
// edit" banner, and a follow-up email draft.

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import * as tauri from "../lib/tauri";
import { aiErrorClass, friendlyAiError, isNoProviderError, withAiConsent } from "../lib/ai";
import { useCapabilities } from "../lib/build";
import { AiSetupNotice, useAiStatus } from "./AiSetupNotice";
import ErrorState from "./ErrorState";
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

type Decision = { text: string; made_by?: string | null };
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
            return `${what} need your permission to send this meeting to your AI provider. Try again and choose Allow.`;
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
            // Uses the persona's meeting_report prompt (PROMPTS) and saves it
            await tauri.generateMeetingReport(meetingId);
            await load();
        } catch (e) {
            if (isNoProviderError(e)) setNeedsAi(true);
            else setActionError(aiFailure(e, "AI notes"));
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
                {generating ? (notes ? "Regenerating…" : "Writing notes…") : notes ? "Regenerate" : "Generate notes"}
            </button>
            <button className="mn-btn" onClick={draftEmail} disabled={drafting || showSetup}>
                {drafting ? "Drafting…" : `Follow-up email${proNote}`}
            </button>
        </div>
    );

    return (
        <div className="mn-panel">
            {showSetup && <AiSetupNotice feature="AI notes and follow-up emails" />}
            {actionError && (
                <p className="mn-error" role="alert">
                    {actionError}
                </p>
            )}

            {!notes ? (
                <div className="mn-empty">
                    <h3>No AI notes yet</h3>
                    <p>
                        Notes are written automatically when a recording longer than 6 minutes stops (Settings → AI
                        Engine → Automatic AI). Generate them now from the transcript{proNote}.
                    </p>
                    {actions}
                </div>
            ) : (
                <div className="mn-notes">
                    {notes.stale_after_edit && (
                        <div className="rd-stale" role="status">
                            <span>These notes were made before an edit. Regenerate?</span>
                            <button className="rd-btn" onClick={generate} disabled={generating || showSetup}>
                                {generating ? "Regenerating…" : "Regenerate"}
                            </button>
                        </div>
                    )}
                    <NotesBody notes={notes} />
                    <p className="mn-meta">
                        Written {new Date(notes.generated_at).toLocaleString()}
                    </p>
                    {actions}
                </div>
            )}

            {email && <FollowUpSheet email={email} onClose={() => setEmail(null)} />}
        </div>
    );
}

function NotesBody({ notes }: { notes: SavedNotes }) {
    const topics = parseList<string>(notes.key_topics);
    const decisions = parseList<Decision>(notes.decisions);
    const actions = parseList<ActionItem>(notes.action_items);
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
                    Drafted by your AI provider from the transcript. Check it before sending.
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
