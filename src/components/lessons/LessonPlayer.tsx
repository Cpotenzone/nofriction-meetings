// The lesson player (docs/LESSONS.md §6/§8/§10.1a): explainers, worked
// examples, the misconceptions cards, and the checkpoint. Progress is persisted
// as the learner reveals explainers / worked examples and the lesson resumes at
// the last-seen explainer on reopen (lessons_save_progress / lessons_get_
// progress + resumeExplainerIndex). A sub-80% checkpoint does not block
// navigation but marks the lesson "needs review" and routes remediation to the
// explainers for the concepts the learner actually missed (the panel rendered
// below the checkpoint links straight to each one).

import { useEffect, useMemo, useRef, useState } from "react";
import type { Lesson, LessonSet, GradedItem, LessonProgress } from "../../lib/lessons";
import { getLesson, submitCheckpoint, saveProgress, getProgress } from "../../lib/lessons";
import { resumeExplainerIndex } from "../../lib/lessonLogic";
import { ExplainerView } from "./Explainers";
import { QuizRunner } from "./QuizRunner";

interface LessonPlayerProps {
    set: LessonSet;
    lessonId: string;
    figureDir?: string;
    onBack: () => void;
    onCheckpoint: (lessonId: string, passed: boolean, pct: number) => void;
}

export function LessonPlayer({ set, lessonId, figureDir, onBack, onCheckpoint }: LessonPlayerProps) {
    const [lesson, setLesson] = useState<Lesson | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [result, setResult] = useState<{ pct: number; passed: boolean; missedConceptIds: string[] } | null>(null);
    // Persisted progress — which explainers / worked examples the learner has
    // revealed, and the last explainer they viewed (for resume).
    const progressRef = useRef<LessonProgress>({
        explainersSeen: [],
        workedExamplesSeen: [],
        checkpointBestPct: 0,
        checkpointPassed: false,
        lastViewedExplainerId: null,
    });
    const [resumeHint, setResumeHint] = useState<{ index: number; id: string } | null>(null);
    // Refs to each explainer block so remediation routing can scroll to one.
    const explainerRefs = useRef<Record<string, HTMLDivElement | null>>({});
    const conceptName = useMemo(() => {
        const map: Record<string, string> = {};
        set.concepts.forEach((c) => (map[c.id] = c.name));
        return (id: string) => map[id] ?? id;
    }, [set.concepts]);

    useEffect(() => {
        let alive = true;
        setLesson(null);
        setResult(null);
        setResumeHint(null);
        Promise.all([getLesson(set.id, lessonId), getProgress(set.id, lessonId).catch(() => null)])
            .then(([l, p]) => {
                if (!alive) return;
                setLesson(l);
                if (p) {
                    progressRef.current = p;
                    const idx = resumeExplainerIndex(
                        l.explainers.map((e) => e.id),
                        p.explainersSeen,
                    );
                    const resumeId = p.lastViewedExplainerId ?? l.explainers[idx]?.id ?? null;
                    if (resumeId && p.explainersSeen.length > 0 && p.explainersSeen.length < l.explainers.length) {
                        setResumeHint({ index: idx, id: resumeId });
                    }
                } else {
                    progressRef.current = {
                        explainersSeen: [],
                        workedExamplesSeen: [],
                        checkpointBestPct: 0,
                        checkpointPassed: false,
                        lastViewedExplainerId: null,
                    };
                }
            })
            .catch((e) => alive && setError(String(e)));
        return () => {
            alive = false;
        };
    }, [set.id, lessonId]);

    // Record that an explainer / worked example has been seen and persist. Keeps
    // the sets de-duplicated and fire-and-forget (a save failure never blocks
    // the learner — the authoritative state is the Rust store).
    const markExplainerSeen = (id: string) => {
        const p = progressRef.current;
        p.lastViewedExplainerId = id;
        if (!p.explainersSeen.includes(id)) p.explainersSeen.push(id);
        void saveProgress(set.id, lessonId, p).catch(() => {});
    };
    const markWorkedSeen = (id: string) => {
        const p = progressRef.current;
        if (!p.workedExamplesSeen.includes(id)) {
            p.workedExamplesSeen.push(id);
            void saveProgress(set.id, lessonId, p).catch(() => {});
        }
    };

    const scrollToExplainer = (id: string) => {
        explainerRefs.current[id]?.scrollIntoView({ behavior: "smooth", block: "start" });
    };

    if (error) return <div className="lessons-state is-error">Couldn't open this lesson: {error}</div>;
    if (!lesson) return <div className="lessons-state">Loading the lesson…</div>;

    // Explainers that teach a given concept — the remediation target set.
    const explainersForConcept = (conceptId: string) =>
        lesson.explainers.filter((e) => e.conceptIds.includes(conceptId));

    return (
        <div className="lesson-player">
            <div className="lesson-player__nav">
                <button className="lessons-btn" type="button" onClick={onBack}>
                    ← Back to course
                </button>
                <h2 className="lessons__title">
                    Lesson {lesson.index}: {lesson.title}
                </h2>
            </div>

            {resumeHint && (
                <section className="lesson-player__section">
                    <p className="lesson-card__meta">You were partway through this lesson.</p>
                    <button
                        className="lessons-btn"
                        type="button"
                        onClick={() => {
                            scrollToExplainer(resumeHint.id);
                            setResumeHint(null);
                        }}
                    >
                        Resume where you left off
                    </button>
                </section>
            )}

            {lesson.objectives.length > 0 && (
                <section className="lesson-player__section">
                    <h3>Learning objectives</h3>
                    <ul>
                        {lesson.objectives.map((o, i) => (
                            <li key={i}>{o.text}</li>
                        ))}
                    </ul>
                </section>
            )}

            <section className="lesson-player__section">
                <h3>Explainers</h3>
                {lesson.explainers.map((ex) => (
                    <div
                        key={ex.id}
                        style={{ marginBottom: 16 }}
                        ref={(el) => {
                            explainerRefs.current[ex.id] = el;
                        }}
                    >
                        <strong>{ex.title}</strong>
                        <p className="lesson-figure__caption">{ex.caption}</p>
                        <ExplainerView explainer={ex} figureDir={figureDir} onSeen={() => markExplainerSeen(ex.id)} />
                    </div>
                ))}
            </section>

            <section className="lesson-player__section">
                <h3>Worked examples</h3>
                {lesson.workedExamples.map((w) => (
                    <WorkedExampleView
                        key={w.id}
                        prompt={w.prompt}
                        steps={w.steps}
                        final={w.finalAnswer}
                        onReveal={() => markWorkedSeen(w.id)}
                    />
                ))}
            </section>

            <section className="lesson-player__section">
                <h3>Common mistakes</h3>
                {lesson.misconceptions.items.map((m, i) => (
                    <MisconceptionCard key={i} claim={m.claim} why={m.whyWrong} correct={m.correct} />
                ))}
            </section>

            <section className="lesson-player__section">
                <h3>Checkpoint</h3>
                {result ? (
                    <div>
                        <p className={result.passed ? "quiz-feedback is-correct" : "quiz-feedback is-wrong"}>
                            You scored {result.pct.toFixed(0)}%.{" "}
                            {result.passed
                                ? "Checkpoint passed."
                                : "Below 80% — this lesson needs review before the exam unlocks."}
                        </p>
                        {!result.passed && result.missedConceptIds.length > 0 && (
                            <div className="quiz-solution">
                                <p>Review these concepts, then retake the checkpoint:</p>
                                {result.missedConceptIds.map((cid) => {
                                    const target = explainersForConcept(cid)[0];
                                    return (
                                        <button
                                            key={cid}
                                            type="button"
                                            className="lessons-btn"
                                            style={{ marginRight: 8, marginBottom: 6 }}
                                            disabled={!target}
                                            onClick={() => target && scrollToExplainer(target.id)}
                                        >
                                            Review: {conceptName(cid)}
                                        </button>
                                    );
                                })}
                            </div>
                        )}
                        <button
                            className="lessons-btn"
                            type="button"
                            style={{ marginTop: 8 }}
                            onClick={() => setResult(null)}
                        >
                            Retake the checkpoint
                        </button>
                    </div>
                ) : (
                    <QuizRunner
                        items={lesson.checkpoint.items.slice(0, lesson.checkpoint.presentCount)}
                        submitLabel="Submit checkpoint"
                        onSubmit={async (answers) => {
                            const r = await submitCheckpoint(set.id, lesson.id, answers);
                            const missed = missedConcepts(lesson, r.graded);
                            setResult({ pct: r.scorePct, passed: r.passed, missedConceptIds: missed });
                            // Persist the checkpoint outcome alongside explainer progress.
                            const p = progressRef.current;
                            p.checkpointBestPct = Math.max(p.checkpointBestPct, r.scorePct);
                            p.checkpointPassed = p.checkpointPassed || r.passed;
                            void saveProgress(set.id, lesson.id, p).catch(() => {});
                            onCheckpoint(lesson.id, r.passed, r.scorePct);
                            return r.graded;
                        }}
                    />
                )}
            </section>
        </div>
    );
}

// The concepts a learner missed, derived from the graded checkpoint: for every
// item answered incorrectly, collect its conceptIds (de-duplicated, order
// preserved) so remediation can point at the right explainers.
function missedConcepts(lesson: Lesson, graded: GradedItem[]): string[] {
    const wrongItemIds = new Set(graded.filter((g) => !g.correct).map((g) => g.itemId));
    const out: string[] = [];
    for (const item of lesson.checkpoint.items) {
        if (!wrongItemIds.has(item.id)) continue;
        for (const cid of item.conceptIds) {
            if (!out.includes(cid)) out.push(cid);
        }
    }
    return out;
}

function WorkedExampleView({
    prompt,
    steps,
    final,
    onReveal,
}: {
    prompt: string;
    steps: { text: string; expression?: string | null; result?: string | null; unit?: string | null }[];
    final: { value: number; unit?: string | null };
    onReveal?: () => void;
}) {
    const [shown, setShown] = useState(0);
    const advance = () => {
        setShown((n) => {
            const next = n + 1;
            if (next >= steps.length) onReveal?.();
            return next;
        });
    };
    return (
        <div style={{ marginBottom: 12 }}>
            <p className="quiz-item__prompt">{prompt}</p>
            {steps.slice(0, shown).map((s, i) => (
                <div className="quiz-solution" key={i}>
                    {s.text}
                    {s.expression ? ` — ${s.expression}` : ""}
                    {s.result ? ` = ${s.result}${s.unit ? " " + s.unit : ""}` : ""}
                </div>
            ))}
            {shown < steps.length ? (
                <button className="lessons-btn" type="button" onClick={advance}>
                    Next step
                </button>
            ) : (
                <p className="lesson-card__badge">
                    Answer: {final.value} {final.unit}
                </p>
            )}
        </div>
    );
}

function MisconceptionCard({ claim, why, correct }: { claim: string; why: string; correct: string }) {
    const [open, setOpen] = useState(false);
    return (
        <div className="quiz-item">
            <button
                className="lessons-btn"
                type="button"
                aria-expanded={open}
                onClick={() => setOpen((v) => !v)}
                style={{ width: "100%", textAlign: "left" }}
            >
                <span className="misconception-mark" aria-hidden="true">
                    ✗
                </span>{" "}
                {claim}
            </button>
            {open && (
                <div className="quiz-solution">
                    <div className="quiz-feedback is-wrong">Why it's wrong: {why}</div>
                    <div className="quiz-feedback is-correct">Correct approach: {correct}</div>
                </div>
            )}
        </div>
    );
}
