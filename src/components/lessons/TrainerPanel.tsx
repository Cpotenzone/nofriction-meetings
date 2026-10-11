// The spaced-repetition review + adaptive trainer surface (docs/LESSONS.md
// §8.1/§8.2, owner-decision D3). Two parts, both fully offline:
//
//   • Review due — the concepts whose SM-2-lite interval has elapsed
//     (lessons_review_due), weakest first, so the learner knows what to revisit.
//   • Adaptive trainer — pulls the next generated drill for the weakest /
//     most-overdue concept (lessons_next_trainer_item), scales difficulty by
//     mastery, grades the answer, and records the attempt (lessons_submit_
//     trainer_item) which advances mastery and the review interval in Rust.
//
// Grading of a trainer answer is a live preview cue only (previewNumeric);
// mastery and the schedule remain authoritative in Rust via submitTrainerItem.

import { useEffect, useState } from "react";
import type { DueConcept, TrainerItem } from "../../lib/lessons";
import { reviewDue, nextTrainerItem, submitTrainerItem } from "../../lib/lessons";
import { previewNumeric } from "../../lib/lessonLogic";
import { TargetIcon, RouteIcon } from "../icons";

export function TrainerPanel({ setId, conceptName }: { setId: string; conceptName: (id: string) => string }) {
    const [due, setDue] = useState<DueConcept[] | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [training, setTraining] = useState(false);

    const refreshDue = () => {
        reviewDue(setId)
            .then(setDue)
            .catch((e) => setError(String(e)));
    };
    useEffect(refreshDue, [setId]);

    if (training) {
        return (
            <TrainerSession
                setId={setId}
                conceptName={conceptName}
                onDone={() => {
                    setTraining(false);
                    refreshDue();
                }}
            />
        );
    }

    return (
        <section className="lesson-player__section">
            <h3>
                <TargetIcon size={15} /> Review and train
            </h3>
            {error && <p className="quiz-feedback is-wrong">{error}</p>}
            {due && due.length > 0 ? (
                <>
                    <p className="lesson-card__meta">These concepts are due for review (weakest first):</p>
                    {due.map((d) => (
                        <div className="mastery-row" key={d.conceptId}>
                            <span className="mastery-row__name">{conceptName(d.conceptId)}</span>
                            <div className="bar-track" style={{ width: 140 }}>
                                <div className="bar-fill" style={{ width: `${Math.max(0, Math.min(100, d.score))}%` }} />
                            </div>
                        </div>
                    ))}
                </>
            ) : (
                <p className="lesson-card__meta">Nothing is due for review right now. Train anytime to stay sharp.</p>
            )}
            <button className="lessons-btn is-primary" type="button" onClick={() => setTraining(true)}>
                <RouteIcon size={14} /> Start an adaptive training set
            </button>
        </section>
    );
}

// A short adaptive trainer run: fetch an item, answer it, grade + record, move
// on to the next (which adapts to the updated mastery). The learner stops when
// they like or when the trainer runs out of concepts to drill.
function TrainerSession({
    setId,
    conceptName,
    onDone,
}: {
    setId: string;
    conceptName: (id: string) => string;
    onDone: () => void;
}) {
    const [attempt, setAttempt] = useState(0);
    const [item, setItem] = useState<TrainerItem | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [value, setValue] = useState("");
    const [outcome, setOutcome] = useState<{ correct: boolean } | null>(null);
    const [exhausted, setExhausted] = useState(false);

    useEffect(() => {
        let alive = true;
        setLoading(true);
        setValue("");
        setOutcome(null);
        nextTrainerItem(setId, attempt)
            .then((it) => {
                if (!alive) return;
                if (!it || !it.prompt) {
                    setExhausted(true);
                    setItem(null);
                } else {
                    setItem(it);
                }
            })
            .catch((e) => alive && setError(String(e)))
            .finally(() => alive && setLoading(false));
        return () => {
            alive = false;
        };
    }, [setId, attempt]);

    const grade = async () => {
        if (!item) return;
        const g = previewNumeric(value, item.answer, item.unit || null, { kind: "rel", value: 0.01 });
        const correct = g === "correct";
        setOutcome({ correct });
        try {
            await submitTrainerItem(setId, item.conceptId, correct, true);
        } catch (e) {
            setError(String(e));
        }
    };

    return (
        <section className="lesson-player__section">
            <div className="lesson-player__nav">
                <button className="lessons-btn" type="button" onClick={onDone}>
                    ← Done training
                </button>
                <h3>
                    <TargetIcon size={15} /> Adaptive trainer
                </h3>
            </div>
            {error && <p className="quiz-feedback is-wrong">{error}</p>}
            {loading && <p className="lesson-card__meta">Finding the next drill…</p>}
            {exhausted && (
                <div>
                    <p className="quiz-feedback is-correct">Nicely done — no more drills queued right now.</p>
                    <button className="lessons-btn" type="button" onClick={onDone}>
                        Back to the course
                    </button>
                </div>
            )}
            {item && !loading && (
                <div className="quiz-item">
                    <p className="lesson-card__meta">{conceptName(item.conceptId)} · tier {item.tier}</p>
                    <p className="quiz-item__prompt">{item.prompt}</p>
                    <input
                        type="text"
                        inputMode="decimal"
                        aria-label={`answer for ${item.prompt}`}
                        disabled={!!outcome}
                        value={value}
                        onChange={(e) => setValue(e.target.value)}
                        placeholder={item.unit ? `value in ${item.unit}` : "value"}
                        className="quiz-choice"
                        style={{ maxWidth: 220 }}
                    />
                    {!outcome ? (
                        <button className="lessons-btn is-primary" type="button" onClick={grade} style={{ marginLeft: 8 }}>
                            Check
                        </button>
                    ) : (
                        <div>
                            <p className={outcome.correct ? "quiz-feedback is-correct" : "quiz-feedback is-wrong"}>
                                {outcome.correct ? "Correct" : `Not quite — the answer is ${item.answer} ${item.unit}`}
                            </p>
                            <button className="lessons-btn is-primary" type="button" onClick={() => setAttempt((n) => n + 1)}>
                                Next drill
                            </button>
                        </div>
                    )}
                </div>
            )}
        </section>
    );
}
