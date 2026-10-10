// Recordings → Review: a guide made from the recording's transcript with
// the user's AI (Apple on-device or their endpoint): a notes summary, key
// terms, flashcards, a practice quiz and questions to ask, steered by the
// moment markers. It is the "Study guide" for a class and the "Review
// guide" for a meeting or a personal recording; the parts are the same for
// every type. Exports flashcards as CSV and the guide as Markdown.
// docs/STUDY_TOOLS.md
//
// Everything shown here came from a model: it is rendered as React text
// (escaped), never as HTML.

import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { aiErrorClass, friendlyAiError, isNoProviderError, withAiConsent } from "../../lib/ai";
import { useCapabilities } from "../../lib/build";
import { AiSetupNotice, useAiStatus } from "../AiSetupNotice";
import { FlashcardDeck } from "./FlashcardDeck";
import { QuizRunner } from "./QuizRunner";
import { MarkerGlyph } from "./MarkerBits";
import { MARKERS_CHANGED_EVENT, STUDY_PROGRESS_EVENT, studyApi, type StudyGuide, type StudyProgress } from "../../lib/study";
import {
    STUDY_PARTS,
    cardsOf,
    clock,
    filterMarkers,
    markerLabel,
    quizOf,
    type StudyPart,
} from "../../lib/studyLogic";
import { guideTitle, parseKind, thirdMarkLabel, type RecordingKind } from "../../lib/recordingKind";
import { useRecordingKind } from "../../hooks/useRecordingKind";
import "../MeetingNotesPanel.css";
import "./Study.css";

type Tab = StudyPart | "marks";

function failureText(e: unknown, title: string): string {
    switch (aiErrorClass(e)) {
        case "pro_required":
            return `${title}s are part of noFriction Pro.`;
        case "consent_required":
            return `${title}s need your permission to send this recording's transcript to your AI endpoint. Try again and choose Allow.`;
        default:
            return friendlyAiError(e);
    }
}

interface Item {
    question?: string;
    at_ms?: number | null;
}

export function StudyPanel({ meetingId, onJump }: { meetingId: string; onJump: (ms: number) => void }) {
    const [guide, setGuide] = useState<StudyGuide | null>(null);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const [progress, setProgress] = useState<StudyProgress | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [failed, setFailed] = useState<{ kind: StudyPart; error: string }[]>([]);
    const [needsAi, setNeedsAi] = useState(false);
    const [notice, setNotice] = useState<string | null>(null);
    const [tab, setTab] = useState<Tab>("summary");
    const { configured } = useAiStatus();
    const caps = useCapabilities();
    // The type names the guide; the loaded guide's own type wins
    const liveKind = useRecordingKind(meetingId);
    const kind: RecordingKind = guide ? parseKind(guide.recording_kind) : liveKind;
    const title = guideTitle(kind);

    const load = useCallback(() => {
        setLoadError(null);
        studyApi
            .get(meetingId)
            .then(setGuide)
            .catch((e) => setLoadError(String(e)));
    }, [meetingId]);

    useEffect(() => {
        setGuide(null);
        setError(null);
        setFailed([]);
        setNotice(null);
        setProgress(null);
        load();
    }, [load]);

    // The type was changed on the recording: the guide's name and labels follow
    useEffect(() => {
        setGuide((g) => (g && g.recording_kind !== liveKind ? { ...g, recording_kind: liveKind } : g));
    }, [liveKind]);

    // Markers changed elsewhere (Recordings view, capture bar)
    useEffect(() => {
        const on = (e: Event) => {
            const id = (e as CustomEvent<{ meetingId: string }>).detail?.meetingId;
            if (!id || id === meetingId) load();
        };
        window.addEventListener(MARKERS_CHANGED_EVENT, on);
        return () => window.removeEventListener(MARKERS_CHANGED_EVENT, on);
    }, [meetingId, load]);

    useEffect(() => {
        let off: (() => void) | null = null;
        let disposed = false;
        listen<StudyProgress>(STUDY_PROGRESS_EVENT, (e) => {
            if (e.payload.meeting_id === meetingId) setProgress(e.payload);
        }).then((fn) => (disposed ? fn() : (off = fn)));
        return () => {
            disposed = true;
            off?.();
        };
    }, [meetingId]);

    useEffect(() => {
        if (configured) setNeedsAi(false);
    }, [configured]);

    const generate = async (kinds?: StudyPart[]) => {
        setBusy(true);
        setError(null);
        setNotice(null);
        setNeedsAi(false);
        setProgress(null);
        try {
            const r = await withAiConsent(() => studyApi.generate(meetingId, kinds), "review_guide");
            setGuide(r.guide);
            setFailed(r.failed);
            if (r.saved.length > 0 && !kinds) setTab((t) => (r.saved.includes(t as StudyPart) ? t : r.saved[0]));
        } catch (e) {
            if (isNoProviderError(e)) setNeedsAi(true);
            else setError(failureText(e, title));
        } finally {
            setBusy(false);
            setProgress(null);
        }
    };

    const doExport = async (what: "csv" | "md") => {
        setError(null);
        setNotice(null);
        try {
            const path = what === "csv" ? await studyApi.exportFlashcards(meetingId) : await studyApi.exportGuide(meetingId);
            if (path) setNotice(`Saved to ${path}`);
        } catch (e) {
            setError(String(e));
        }
    };

    if (loadError) {
        return (
            <div className="mn-panel">
                <p className="mn-error" role="alert">
                    Couldn't load the {title.toLowerCase()}: {loadError}
                </p>
                <button className="mn-btn" onClick={load}>
                    Try again
                </button>
            </div>
        );
    }
    if (!guide) {
        return (
            <div className="mn-panel mn-center">
                <div className="loading-spinner" />
            </div>
        );
    }

    const has = (k: StudyPart) => !!guide.materials[k];
    const any = STUDY_PARTS.some((p) => has(p.kind));
    const stale = Object.values(guide.materials).some((m) => m?.stale);
    const proNote = caps?.pro_gating ? " (noFriction Pro)" : "";
    const showSetup = configured === false || needsAi;
    const marks = filterMarkers(guide.markers, "all");
    const testMarks = guide.markers.filter((m) => m.kind === "test").length;
    const third = thirdMarkLabel(kind);

    return (
        <div className="mn-panel study-panel">
            {showSetup && <AiSetupNotice feature={`${title}s`} />}
            {error && (
                <p className="mn-error" role="alert">
                    {error}
                </p>
            )}
            {notice && (
                <p className="study-notice" role="status">
                    {notice}
                </p>
            )}

            {busy && (
                <div className="study-busy" role="status" aria-live="polite">
                    <div className="loading-spinner" />
                    <div>
                        <strong>{progress?.label ?? "Starting…"}</strong>
                        {progress && progress.total > 0 && (
                            <div className="study-progress">
                                <div style={{ width: `${Math.min(100, (progress.done / progress.total) * 100)}%` }} />
                            </div>
                        )}
                        <span className="study-muted">On-device models take a minute or two for a long recording.</span>
                    </div>
                </div>
            )}

            {!any && !busy ? (
                <div className="mn-empty">
                    <h3>No {title.toLowerCase()} yet</h3>
                    <p>
                        Turn this {kind === "class" ? "lecture" : "recording"} into notes, key terms, flashcards, a
                        practice quiz and questions to ask
                        {testMarks > 0 ? `, with extra weight on the ${testMarks} moment${testMarks === 1 ? "" : "s"} you marked ✎ ${third}` : ""}
                        . {kind === "class" ? "It's for studying" : kind === "meeting" ? "It's for remembering and following up" : "It's for remembering"};
                        it's made from the transcript by the AI you set up, and deleted or stricken text is never sent.
                        {proNote ? " Part of noFriction Pro." : ""}
                    </p>
                    <div className="mn-actions">
                        <button className="mn-btn primary" onClick={() => generate()} disabled={busy || showSetup || !guide.has_transcript}>
                            Make {title.toLowerCase()}
                        </button>
                    </div>
                    {!guide.has_transcript && <p className="study-muted">This recording has no transcript yet.</p>}
                </div>
            ) : (
                <>
                    {stale && (
                        <div className="rd-stale" role="status">
                            <span>Made from an earlier version of the transcript.</span>
                            <button className="rd-btn" onClick={() => generate()} disabled={busy || showSetup}>
                                Make again
                            </button>
                        </div>
                    )}
                    <div className="deck-tabs study-tabs" role="tablist" aria-label={title}>
                        {STUDY_PARTS.map((p) => (
                            <button
                                key={p.kind}
                                role="tab"
                                aria-selected={tab === p.kind}
                                className={`deck-tab ${tab === p.kind ? "active" : ""}`}
                                onClick={() => setTab(p.kind)}
                            >
                                {p.label}
                            </button>
                        ))}
                        <button role="tab" aria-selected={tab === "marks"} className={`deck-tab ${tab === "marks" ? "active" : ""}`} onClick={() => setTab("marks")}>
                            Marked ({marks.length})
                        </button>
                    </div>

                    <div className="study-body">
                        {tab === "marks" ? (
                            <MarkedMoments guide={guide} kind={kind} onJump={onJump} />
                        ) : has(tab) ? (
                            <Part kind={tab} rec={kind} guide={guide} onJump={onJump} />
                        ) : (
                            <div className="mn-empty">
                                <p>{failed.find((f) => f.kind === tab)?.error ?? "Not made yet."}</p>
                            </div>
                        )}
                        {tab !== "marks" && (
                            <div className="mn-actions">
                                <button className="mn-btn" onClick={() => generate([tab])} disabled={busy || showSetup}>
                                    {has(tab) ? "Make this part again" : "Make this part"}
                                </button>
                            </div>
                        )}
                    </div>

                    {failed.length > 0 && (
                        <div className="mn-error" role="alert">
                            {failed.map((f) => (
                                <div key={f.kind}>{f.error}</div>
                            ))}
                        </div>
                    )}

                    <div className="mn-actions study-exports">
                        <button className="mn-btn" onClick={() => doExport("csv")} disabled={!has("flashcards")} title="front,back rows that flashcard apps can import">
                            Export flashcards (CSV)
                        </button>
                        <button className="mn-btn" onClick={() => doExport("md")} title="Summary, key terms, marked moments, questions and the quiz">
                            Export {title.toLowerCase()} (Markdown)
                        </button>
                        <button className="mn-btn" onClick={() => generate()} disabled={busy || showSetup}>
                            Make all again
                        </button>
                    </div>
                    <p className="mn-meta">
                        Made by your AI from the transcript. It can be wrong: check it against the recording. Deleting
                        or striking transcript text deletes this guide.
                    </p>
                </>
            )}
        </div>
    );
}

function Part({ kind, rec, guide, onJump }: { kind: StudyPart; rec: RecordingKind; guide: StudyGuide; onJump: (ms: number) => void }) {
    const data = guide.materials[kind]?.data as Record<string, unknown> | undefined;
    if (!data) return null;
    switch (kind) {
        case "summary": {
            const sections = Array.isArray(data.sections) ? (data.sections as { heading?: unknown; bullets?: unknown }[]) : [];
            return (
                <div className="mn-notes">
                    {typeof data.title === "string" && <h3 className="study-title">{data.title}</h3>}
                    {sections.map((s, i) => (
                        <section className="mn-section" key={i}>
                            <h4>{String(s.heading ?? "")}</h4>
                            <ul>
                                {(Array.isArray(s.bullets) ? s.bullets : []).map((b, j) => (
                                    <li key={j}>{String(b)}</li>
                                ))}
                            </ul>
                        </section>
                    ))}
                </div>
            );
        }
        case "terms": {
            const terms = Array.isArray(data.terms) ? (data.terms as { term?: unknown; definition?: unknown }[]) : [];
            return (
                <dl className="study-terms">
                    {terms.map((t, i) => (
                        <div key={i}>
                            <dt>{String(t.term ?? "")}</dt>
                            <dd>{String(t.definition ?? "")}</dd>
                        </div>
                    ))}
                </dl>
            );
        }
        case "flashcards":
            return <FlashcardDeck cards={cardsOf(data)} />;
        case "quiz":
            return <QuizRunner questions={quizOf(data)} onJump={onJump} />;
        case "questions": {
            const qs = Array.isArray(data.questions) ? (data.questions as Item[]) : [];
            const confused = guide.markers.filter((m) => m.kind === "question");
            return (
                <div className="mn-notes">
                    <ul className="study-asks">
                        {qs.map((q, i) => (
                            <li key={i}>
                                {String(q.question ?? "")}
                                {typeof q.at_ms === "number" && (
                                    <button type="button" className="study-link" onClick={() => onJump(q.at_ms as number)}>
                                        {clock(q.at_ms)}
                                    </button>
                                )}
                            </li>
                        ))}
                    </ul>
                    {confused.length > 0 && (
                        <section className="mn-section">
                            <h4>{rec === "class" ? "You marked as confusing" : "You marked with a question"}</h4>
                            <ul className="study-asks">
                                {confused.map((m) => (
                                    <li key={m.id}>
                                        {m.note || "No note"}
                                        <button type="button" className="study-link" onClick={() => onJump(m.offset_ms)}>
                                            {clock(m.offset_ms)}
                                        </button>
                                    </li>
                                ))}
                            </ul>
                        </section>
                    )}
                </div>
            );
        }
    }
}

function MarkedMoments({ guide, kind, onJump }: { guide: StudyGuide; kind: RecordingKind; onJump: (ms: number) => void }) {
    const marks = filterMarkers(guide.markers, "all");
    if (marks.length === 0) {
        return (
            <div className="mn-empty">
                <p>
                    No marked moments. While recording, press Mark (or ⌃⌥⌘M from any app) to mark ★ Important, ? Question
                    or ✎ {thirdMarkLabel(kind)}.
                </p>
            </div>
        );
    }
    return (
        <ul className="study-markers__list">
            {marks.map((m) => (
                <li key={m.id} className={`study-markers__row is-${m.kind}`}>
                    <button type="button" className="study-time" onClick={() => onJump(m.offset_ms)} title="Jump to this moment">
                        <MarkerGlyph kind={m.kind} /> {clock(m.offset_ms)}
                    </button>
                    <span className="study-note is-static">
                        {markerLabel(m.kind, kind)}
                        {m.note ? `: ${m.note}` : ""}
                    </span>
                </li>
            ))}
        </ul>
    );
}
