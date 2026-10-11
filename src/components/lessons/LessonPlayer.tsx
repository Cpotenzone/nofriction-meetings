// The lesson player (docs/LESSONS.md §6/§8/§10.1a): explainers, worked
// examples, the misconceptions cards, and the checkpoint. A sub-80% checkpoint
// does not block navigation but marks the lesson "needs review" and routes
// remediation to the missed concepts' explainers (handled by the parent).

import { useEffect, useState } from "react";
import type { Lesson, LessonSet } from "../../lib/lessons";
import { getLesson, submitCheckpoint } from "../../lib/lessons";
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
    const [result, setResult] = useState<{ pct: number; passed: boolean } | null>(null);

    useEffect(() => {
        let alive = true;
        setLesson(null);
        setResult(null);
        getLesson(set.id, lessonId)
            .then((l) => alive && setLesson(l))
            .catch((e) => alive && setError(String(e)));
        return () => {
            alive = false;
        };
    }, [set.id, lessonId]);

    if (error) return <div className="lessons-state is-error">Couldn't open this lesson: {error}</div>;
    if (!lesson) return <div className="lessons-state">Loading the lesson…</div>;

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
                    <div key={ex.id} style={{ marginBottom: 16 }}>
                        <strong>{ex.title}</strong>
                        <p className="lesson-figure__caption">{ex.caption}</p>
                        <ExplainerView explainer={ex} figureDir={figureDir} />
                    </div>
                ))}
            </section>

            <section className="lesson-player__section">
                <h3>Worked examples</h3>
                {lesson.workedExamples.map((w) => (
                    <WorkedExampleView key={w.id} prompt={w.prompt} steps={w.steps} final={w.finalAnswer} />
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
                    <p className={result.passed ? "quiz-feedback is-correct" : "quiz-feedback is-wrong"}>
                        You scored {result.pct.toFixed(0)}%.{" "}
                        {result.passed ? "Checkpoint passed." : "Below 80% — this lesson needs review before the exam unlocks."}
                    </p>
                ) : (
                    <QuizRunner
                        items={lesson.checkpoint.items.slice(0, lesson.checkpoint.presentCount)}
                        submitLabel="Submit checkpoint"
                        onSubmit={async (answers) => {
                            const r = await submitCheckpoint(set.id, lesson.id, answers);
                            setResult({ pct: r.scorePct, passed: r.passed });
                            onCheckpoint(lesson.id, r.passed, r.scorePct);
                            return r.graded;
                        }}
                    />
                )}
            </section>
        </div>
    );
}

function WorkedExampleView({
    prompt,
    steps,
    final,
}: {
    prompt: string;
    steps: { text: string; expression?: string | null; result?: string | null; unit?: string | null }[];
    final: { value: number; unit?: string | null };
}) {
    const [shown, setShown] = useState(0);
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
                <button className="lessons-btn" type="button" onClick={() => setShown((n) => n + 1)}>
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
