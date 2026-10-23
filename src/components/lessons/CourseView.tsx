// A course surface (docs/LESSONS.md §10): the lesson list, the mastery
// dashboard with an exam-readiness meter, the exam flow (gated on all six
// checkpoints), and the printable certificate. All local and offline.

import { useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import type { LessonSet, Dashboard as DashboardT, StartedExam, Certificate as CertT } from "../../lib/lessons";
import { dashboard as fetchDashboard, startExam, submitExam, exportCertificate, getLesson, resetProgress } from "../../lib/lessons";
import { GaugeIcon, TrophyIcon, CertificateIcon, LockIcon, CheckIcon } from "../icons";
import { QuizRunner } from "./QuizRunner";
import { LessonPlayer } from "./LessonPlayer";
import { TrainerPanel } from "./TrainerPanel";

type Screen = "overview" | { lessonId: string } | "exam" | "certificate";

export function CourseView({ set, figureDir, onBack }: { set: LessonSet; figureDir?: string; onBack: () => void }) {
    const [screen, setScreen] = useState<Screen>("overview");
    const [dash, setDash] = useState<DashboardT | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [lessonTitles, setLessonTitles] = useState<Record<string, { index: number; title: string; passed?: boolean }>>({});

    const conceptName = (id: string) => set.concepts.find((c) => c.id === id)?.name ?? id;

    const refresh = () => {
        fetchDashboard(set.id).then(setDash).catch((e) => setError(String(e)));
    };
    useEffect(refresh, [set.id]);

    const startOver = async () => {
        const ok = await ask(
            "This clears your progress, mastery, exam attempts and certificate for this course.",
            { title: "Start this course over?", okLabel: "Start over", cancelLabel: "Cancel", kind: "warning" },
        ).catch(() => false);
        if (!ok) return;
        try {
            await resetProgress(set.id);
            refresh();
        } catch (e) {
            setError(String(e));
        }
    };

    useEffect(() => {
        let alive = true;
        Promise.all(set.lessons.map((id) => getLesson(set.id, id).catch(() => null))).then((ls) => {
            if (!alive) return;
            const map: Record<string, { index: number; title: string }> = {};
            ls.forEach((l) => {
                if (l) map[l.id] = { index: l.index, title: l.title };
            });
            setLessonTitles(map);
        });
        return () => {
            alive = false;
        };
    }, [set.id]);

    if (typeof screen === "object") {
        return (
            <LessonPlayer
                set={set}
                lessonId={screen.lessonId}
                figureDir={figureDir}
                onBack={() => {
                    setScreen("overview");
                    refresh();
                }}
                onCheckpoint={refresh}
            />
        );
    }

    if (screen === "exam") {
        return <ExamScreen set={set} onDone={() => { setScreen("overview"); refresh(); }} onCertificate={() => setScreen("certificate")} />;
    }

    if (screen === "certificate") {
        return <CertificateScreen setId={set.id} onBack={() => setScreen("overview")} />;
    }

    return (
        <div>
            <div className="lesson-player__nav">
                <button className="lessons-btn" type="button" onClick={onBack}>
                    ← All courses
                </button>
                <h2 className="lessons__title">{set.title}</h2>
            </div>
            {set.provenance === "generated" && (
                <p className="lesson-card__meta make-lesson__source">
                    Made from your recording{set.subtitle ? ` “${set.subtitle}”` : ""} with your AI. A Delete or Strike in
                    that recording deletes this course; make it again from Lessons → Make lesson.
                </p>
            )}
            {error && <p className="quiz-feedback is-wrong">{error}</p>}

            <section className="lesson-player__section">
                <h3>Lessons</h3>
                {set.lessons.map((id) => {
                    const meta = lessonTitles[id];
                    return (
                        <button
                            key={id}
                            type="button"
                            className="lesson-card"
                            style={{ marginBottom: 8 }}
                            onClick={() => setScreen({ lessonId: id })}
                        >
                            <span className="lesson-card__title">
                                {meta ? `Lesson ${meta.index}: ${meta.title}` : id}
                            </span>
                        </button>
                    );
                })}
            </section>

            {dash && (
                <section className="lesson-player__section">
                    <h3>
                        <GaugeIcon size={15} /> Mastery dashboard
                    </h3>
                    <div className="readiness-meter">
                        <div className="readiness-meter__label">
                            Exam readiness: {dash.readinessPct.toFixed(0)}% — {dash.readinessLabel}
                        </div>
                        <div className="bar-track">
                            <div className="bar-fill" style={{ width: `${dash.readinessPct}%` }} />
                        </div>
                    </div>
                    {dash.concepts.map((c) => (
                        <div className="mastery-row" key={c.conceptId}>
                            <span className="mastery-row__name">
                                {c.name}
                                {c.mastered && (
                                    <>
                                        {" "}
                                        <CheckIcon size={13} className="mastery-row__check" />
                                        <span className="lessons-sr-only"> mastered</span>
                                    </>
                                )}
                            </span>
                            <div className="bar-track" style={{ width: 140 }}>
                                <div className={`bar-fill ${c.mastered ? "is-match" : ""}`} style={{ width: `${c.score}%` }} />
                            </div>
                        </div>
                    ))}
                    <p className="lesson-card__meta">
                        {dash.checkpointsPassed} of {dash.totalLessons} checkpoints passed.
                    </p>
                </section>
            )}

            <TrainerPanel setId={set.id} conceptName={conceptName} canTrain={set.provenance !== "generated"} />

            <section className="lesson-player__section">
                <h3>
                    <TrophyIcon size={15} /> Final exam
                </h3>
                {dash?.examUnlocked ? (
                    <button className="lessons-btn is-primary" type="button" onClick={() => setScreen("exam")}>
                        Start the {set.exam.questionCount}-question exam
                    </button>
                ) : (
                    <p className="lesson-card__meta">
                        <LockIcon size={14} /> Pass every lesson checkpoint (80%+) to unlock the exam.
                    </p>
                )}
                <button className="lessons-btn" type="button" style={{ marginLeft: 8 }} onClick={() => setScreen("certificate")}>
                    <CertificateIcon size={14} /> View certificate
                </button>
            </section>

            <section className="lesson-player__section">
                <h3>Start over</h3>
                <p className="lesson-card__meta">
                    Reset your progress, mastery, exam attempts and certificate for this course.
                </p>
                <button className="lessons-btn" type="button" onClick={startOver}>
                    Start this course over
                </button>
            </section>
        </div>
    );
}

function ExamScreen({ set, onDone, onCertificate }: { set: LessonSet; onDone: () => void; onCertificate: () => void }) {
    const [started, setStarted] = useState<StartedExam | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [result, setResult] = useState<{ pct: number; passed: boolean } | null>(null);

    useEffect(() => {
        startExam(set.id)
            .then(setStarted)
            .catch((e) => setError(String(e)));
    }, [set.id]);

    if (error) return <div className="lessons-state is-error">{error} <button className="lessons-btn" onClick={onDone}>Back</button></div>;
    if (!started) return <div className="lessons-state">Preparing your exam…</div>;

    return (
        <div>
            <div className="lesson-player__nav">
                <button className="lessons-btn" type="button" onClick={onDone}>
                    ← Back
                </button>
                <h2 className="lessons__title">Final exam — {set.title}</h2>
            </div>
            <div className="lessons-sr-only" role="status" aria-live="polite">
                {result ? `You scored ${result.pct.toFixed(0)}%. ${result.passed ? "You passed!" : "80% is needed to pass."}` : ""}
            </div>
            {result ? (
                <div className="lesson-player__section">
                    <p className={result.passed ? "quiz-feedback is-correct" : "quiz-feedback is-wrong"}>
                        You scored {result.pct.toFixed(0)}%. {result.passed ? "You passed!" : "80% is needed to pass — try again."}
                    </p>
                    {result.passed && (
                        <button className="lessons-btn is-primary" type="button" onClick={onCertificate}>
                            View your certificate
                        </button>
                    )}
                    <button className="lessons-btn" type="button" style={{ marginLeft: 8 }} onClick={onDone}>
                        Back to course
                    </button>
                </div>
            ) : (
                <QuizRunner
                    items={started.items}
                    submitLabel="Submit exam"
                    onSubmit={async (answers) => {
                        const r = await submitExam(set.id, started.attemptId, answers);
                        setResult({ pct: r.scorePct, passed: r.passed });
                        return r.graded;
                    }}
                />
            )}
        </div>
    );
}

function CertificateScreen({ setId, onBack }: { setId: string; onBack: () => void }) {
    const [cert, setCert] = useState<CertT | null>(null);
    const [loaded, setLoaded] = useState(false);
    useEffect(() => {
        exportCertificate(setId)
            .then((c) => {
                setCert(c);
                setLoaded(true);
            })
            .catch(() => setLoaded(true));
    }, [setId]);

    if (!loaded) return <div className="lessons-state">Loading…</div>;
    if (!cert)
        return (
            <div className="lessons-state">
                <p>No certificate yet — pass the final exam to earn one.</p>
                <button className="lessons-btn" type="button" onClick={onBack}>
                    Back
                </button>
            </div>
        );

    return (
        <div>
            <div className="lesson-player__nav">
                <button className="lessons-btn" type="button" onClick={onBack}>
                    ← Back
                </button>
                <button className="lessons-btn" type="button" onClick={() => window.print()}>
                    Print / save as PDF
                </button>
            </div>
            <div className="certificate">
                <TrophyIcon size={44} className="generate-preview__hero" strokeWidth={1.5} />
                <p>Certificate of completion</p>
                <div className="certificate__title">{cert.courseTitle}</div>
                {cert.level && <p>{cert.level}</p>}
                <p>Score: {cert.scorePct.toFixed(0)}%</p>
                <p>Issued {new Date(cert.issuedAt).toLocaleDateString()}</p>
                <p className="lesson-card__meta">
                    {cert.provenance === "seed" ? "Seed content" : cert.provenance === "imported" ? "Imported lesson" : "Generated from your recording"}
                </p>
                <p className="certificate__hash">Local verification hash: {cert.resultHash}</p>
            </div>
        </div>
    );
}
