// Practice quiz: pick an answer, see right or wrong with the explanation and
// a link to the moment in the lecture it comes from; score at the end.
// Question text is model output: rendered as text only.

import { useEffect, useMemo, useState } from "react";
import {
    clock,
    isCorrect,
    missedQuestions,
    newQuiz,
    nextQuestion,
    pickAnswer,
    quizScore,
    type QuizQuestion,
    type QuizState,
} from "../../lib/studyLogic";

export function QuizRunner({ questions, onJump }: { questions: QuizQuestion[]; onJump?: (ms: number) => void }) {
    const key = useMemo(() => questions.map((q) => q.question).join("\u0000"), [questions]);
    // Which questions this run asks (all, or the ones missed last time)
    const [set, setSet] = useState<number[]>(() => questions.map((_, i) => i));
    const qs = useMemo(() => set.map((i) => questions[i]).filter(Boolean), [set, questions]);
    const [state, setState] = useState<QuizState>(() => newQuiz(questions.length));
    useEffect(() => {
        setSet(questions.map((_, i) => i));
        setState(newQuiz(questions.length));
    }, [key, questions]);

    if (qs.length === 0) return null;
    const restart = (indexes: number[]) => {
        setSet(indexes);
        setState(newQuiz(indexes.length));
    };

    if (state.finished) {
        const s = quizScore(state, qs);
        const missed = missedQuestions(state, qs).map((i) => set[i]);
        return (
            <div className="study-quiz study-done" role="status">
                <strong>
                    {s.correct} of {s.total} right
                </strong>
                <span className="study-muted">{s.correct === s.total ? "Every answer right." : `${s.total - s.correct} to review.`}</span>
                <div className="study-deck__actions">
                    {missed.length > 0 && (
                        <button type="button" className="mn-btn primary" onClick={() => restart(missed)}>
                            Retry the {missed.length} missed
                        </button>
                    )}
                    <button type="button" className="mn-btn" onClick={() => restart(questions.map((_, i) => i))}>
                        Start over
                    </button>
                </div>
            </div>
        );
    }

    const q = qs[state.pos];
    const picked = state.picked[state.pos] ?? null;
    const answered = picked !== null;
    const right = isCorrect(q, picked);

    return (
        <div className="study-quiz">
            <div className="study-deck__meta">
                <span>
                    Question {state.pos + 1} of {qs.length}
                </span>
                <span className="study-muted">{quizScore(state, qs).correct} right so far</span>
            </div>
            <p className="study-quiz__q">{q.question}</p>
            <ol className="study-quiz__choices" type="A">
                {q.choices.map((c, i) => {
                    const cls = !answered ? "" : i === q.answer ? " is-right" : i === picked ? " is-wrong" : " is-dim";
                    return (
                        <li key={i}>
                            <button
                                type="button"
                                className={`study-choice${cls}`}
                                disabled={answered}
                                aria-pressed={picked === i}
                                onClick={() => setState((s) => pickAnswer(s, qs, i))}
                            >
                                <span className="study-choice__letter">{String.fromCharCode(65 + i)}</span>
                                <span>{c}</span>
                            </button>
                        </li>
                    );
                })}
            </ol>
            {answered && (
                <div className={`study-feedback${right ? " is-right" : " is-wrong"}`} role="status">
                    <strong>{right ? "Right." : `Not quite: the answer is ${String.fromCharCode(65 + q.answer)}.`}</strong>
                    {q.explanation && <span> {q.explanation}</span>}
                    {q.at_ms !== null && onJump && (
                        <button type="button" className="study-link" onClick={() => onJump(q.at_ms as number)}>
                            Jump to this moment ({clock(q.at_ms)})
                        </button>
                    )}
                </div>
            )}
            <div className="study-deck__actions">
                <button type="button" className="mn-btn primary" disabled={!answered} onClick={() => setState((s) => nextQuestion(s, qs))}>
                    {state.pos + 1 >= qs.length ? "See score" : "Next question"}
                </button>
            </div>
        </div>
    );
}
