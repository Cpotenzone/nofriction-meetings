// Make lesson (docs/LESSONS.md §12.3/§13): pick a Class recording, then make
// an interactive lesson from its transcript with the user's own AI. Opened
// from the Lessons header, or from Recordings → Make lesson on a Class
// recording (which starts right away unless that recording already has a
// lesson). The run lives in lib/lessonMake.ts so leaving Lessons doesn't stop
// it. When it is done the new course opens.
//
// It lives outside components/lessons/ and the shell hands it to the Lessons
// view (Shell.tsx passes it as LessonsView's makePanel): it uses the app's AI
// helpers (withAiConsent, AiSetupNotice, ...), whose modules carry remote
// links, and the Lessons chunk must hold none (scripts/check-lessons-offline.mjs).
//
// Making a lesson is noFriction Pro and goes through ai::client::complete
// (Pro → configured → consent); taking it is free and offline. Everything
// shown came from validated, cleaned data and renders as React text.

import { useCallback, useEffect, useRef, useState } from "react";
import { aiErrorClass, friendlyAiError, isNoProviderError, withAiConsent } from "../lib/ai";
import { useCapabilities, useIsPro } from "../lib/build";
import { proSuffix } from "../lib/pro";
import { getMeetings, type Meeting } from "../lib/tauri";
import { listCatalog, makeFromRecording, type CatalogEntry } from "../lib/lessons";
import {
    classRecordings,
    durationLabel,
    failedPartsSummary,
    isStaleMakeLessonJob,
    madeLessonFor,
    type MakeLessonPanelProps,
} from "../lib/lessonMakeLogic";
import {
    clearMakeLessonJob,
    currentMakeLessonJob,
    markMakeLessonOpened,
    onMakeLessonJob,
    startMakeLesson,
    type MakeLessonJob,
} from "../lib/lessonMake";
import { AiSetupNotice, useAiStatus } from "./AiSetupNotice";
import { SparkleIcon } from "./icons";

function failureText(e: unknown): string {
    switch (aiErrorClass(e)) {
        case "pro_required":
            return "Making lessons is part of noFriction Pro.";
        case "consent_required":
            return "Making a lesson needs your permission to send this recording's transcript to your AI endpoint. Try again and choose Allow.";
        default:
            return friendlyAiError(e);
    }
}

function when(iso: string): string {
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? "" : d.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" });
}

export function MakeLessonPanel({ request, onOpenSet, onBack }: MakeLessonPanelProps) {
    const [recordings, setRecordings] = useState<Meeting[] | null>(null);
    const [catalog, setCatalog] = useState<CatalogEntry[] | null>(null);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [selected, setSelected] = useState<string | null>(request?.meetingId ?? currentMakeLessonJob()?.meetingId ?? null);
    const [job, setJob] = useState<MakeLessonJob | null>(currentMakeLessonJob());
    const { configured } = useAiStatus();
    const caps = useCapabilities();
    const isPro = useIsPro();
    const proNote = proSuffix(caps?.pro_gating, isPro);
    const startedFor = useRef<number | null>(null);

    useEffect(() => onMakeLessonJob(setJob), []);
    // Leaving the panel (← All courses, the top bar, another view) forgets a
    // finished or failed run, so its error doesn't come back next time; a
    // running one keeps going and the library shows it
    useEffect(() => () => clearMakeLessonJob(), []);
    // A finished or failed run that no longer applies is forgotten too:
    // another recording was chosen, or AI was set up after a "no AI" failure
    // (the setup notice then goes and Make lesson is enabled again)
    useEffect(() => {
        if (isStaleMakeLessonJob(job, selected, configured, isNoProviderError)) clearMakeLessonJob();
    }, [job, selected, configured]);

    const load = useCallback(() => {
        setLoadError(null);
        Promise.all([getMeetings(500), listCatalog()])
            .then(([ms, cat]) => {
                setRecordings(classRecordings(ms));
                setCatalog(cat);
            })
            .catch((e) => setLoadError(String(e)));
    }, []);
    useEffect(load, [load]);

    const running = job?.status === "running";
    const needsAi = job?.status === "failed" && isNoProviderError(job.error);
    const showSetup = configured === false || needsAi;
    const chosen = recordings?.find((r) => r.id === selected) ?? null;
    const existing = madeLessonFor(catalog, selected);

    const start = (rec: Meeting) => {
        clearMakeLessonJob();
        // The paywall (Mac App Store build without Pro) and the consent dialog
        // (public endpoint) open here, titled for Make lesson, then it retries
        void startMakeLesson(rec.id, rec.title, () => withAiConsent(() => makeFromRecording(rec.id), "lessons"));
    };

    // Recordings → Make lesson: start once the list is in, unless the
    // recording already has a lesson (then the user chooses Open or Make again)
    useEffect(() => {
        if (!request || startedFor.current === request.n || !recordings || !catalog || configured === null) return;
        startedFor.current = request.n;
        setSelected(request.meetingId);
        const rec = recordings.find((r) => r.id === request.meetingId);
        if (!rec || running || configured === false || madeLessonFor(catalog, rec.id)) return;
        start(rec);
    }, [request, recordings, catalog, configured, running]);

    // Done: open the new course
    useEffect(() => {
        if (job?.status === "done" && job.unopened && job.result) {
            markMakeLessonOpened();
            onOpenSet(job.result.setId, failedPartsSummary(job.result.failed));
        }
    }, [job, onOpenSet]);

    const error = job?.status === "failed" && !needsAi ? failureText(job.error) : null;
    const progress = running ? job?.progress : null;

    return (
        <div className="make-lesson">
            <div className="lesson-player__nav">
                <button className="lessons-btn" type="button" onClick={onBack}>
                    ← All courses
                </button>
                <h2 className="lessons__title">
                    <SparkleIcon size={17} /> Make lesson
                </h2>
            </div>

            <p className="make-lesson__intro">
                Turn a Class recording into an interactive lesson: explanations, worked examples, common mistakes, a
                checkpoint for each lesson and a final exam, made from the transcript by the AI you set up. Deleted or
                stricken text is never sent, and a Delete or Strike in the recording later deletes the lesson. Taking
                it is free and works offline.{proNote ? " Making it is part of noFriction Pro." : ""}
            </p>

            {showSetup && <AiSetupNotice feature="Making a lesson" />}

            {running && (
                <div className="make-lesson__busy" role="status" aria-live="polite">
                    <div className="loading-spinner" />
                    <div className="make-lesson__busy-text">
                        <strong>{progress?.label ?? "Starting…"}</strong>
                        <span className="lesson-card__meta">From “{job?.title}”</span>
                        {progress && progress.total > 0 && (
                            <div className="bar-track">
                                <div
                                    className="bar-fill"
                                    style={{ width: `${Math.min(100, (progress.done / progress.total) * 100)}%` }}
                                />
                            </div>
                        )}
                        <span className="lesson-card__meta">
                            On-device models take a few minutes for a long lecture. You can leave Lessons; it keeps
                            going.
                        </span>
                    </div>
                </div>
            )}

            {error && (
                <p className="quiz-feedback is-wrong" role="alert">
                    Couldn't make the lesson{job?.title ? ` from “${job.title}”` : ""}: {error}
                </p>
            )}

            <section className="lesson-player__section">
                <h3>Choose a Class recording</h3>
                {loadError && (
                    <div className="lessons-state is-error">
                        Couldn't load your recordings: {loadError}
                        <button className="lessons-btn" type="button" onClick={load}>
                            Try again
                        </button>
                    </div>
                )}
                {!recordings && !loadError && <p className="lesson-card__meta">Loading your Class recordings…</p>}
                {recordings && recordings.length === 0 && (
                    <p className="lesson-card__meta">
                        No Class recordings yet. In Recordings, set a recording's type to Class (under its title), or
                        start your next recording as a Class.
                    </p>
                )}
                {recordings && recordings.length > 0 && (
                    <div className="make-lesson__list" role="radiogroup" aria-label="Class recordings">
                        {recordings.map((r) => {
                            const made = madeLessonFor(catalog, r.id);
                            const meta = [when(r.started_at), durationLabel(r.duration_seconds)].filter(Boolean).join(" · ");
                            return (
                                <button
                                    key={r.id}
                                    type="button"
                                    role="radio"
                                    aria-checked={selected === r.id}
                                    className={`make-lesson__item ${selected === r.id ? "is-selected" : ""}`}
                                    onClick={() => setSelected(r.id)}
                                    disabled={running}
                                >
                                    <span className="lesson-card__title">{r.title || "Untitled"}</span>
                                    {meta && <span className="lesson-card__meta">{meta}</span>}
                                    {made && <span className="lesson-card__tag">Has a lesson</span>}
                                </button>
                            );
                        })}
                    </div>
                )}
            </section>

            <div className="lessons__actions make-lesson__actions">
                {existing && !running && (
                    <button className="lessons-btn" type="button" onClick={() => onOpenSet(existing.id, null)}>
                        Open its lesson
                    </button>
                )}
                <button
                    className="lessons-btn is-primary"
                    type="button"
                    disabled={!chosen || running || showSetup}
                    onClick={() => chosen && start(chosen)}
                >
                    <SparkleIcon size={15} /> {existing ? "Make again" : "Make lesson"}
                </button>
            </div>
            {existing && !running && (
                <p className="lesson-card__meta">
                    Making it again replaces that lesson, with its progress, exam attempts and certificate.
                </p>
            )}
        </div>
    );
}
