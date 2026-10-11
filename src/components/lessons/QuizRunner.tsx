// A shared quiz runner for checkpoints and the exam (docs/LESSONS.md §8.3,
// §10). It collects answers and, after submission, reveals the full worked
// solution per item (grading is authoritative in Rust; this never decides
// pass/fail). Keyboard operable, with a live preview cue for numeric answers.

import { useState } from "react";
import type { QuizItem, SubmittedAnswer, GradedItem } from "../../lib/lessons";
import { previewNumeric } from "../../lib/lessonLogic";

interface QuizRunnerProps {
    items: QuizItem[];
    onSubmit: (answers: SubmittedAnswer[]) => Promise<GradedItem[]>;
    submitLabel: string;
}

export function QuizRunner({ items, onSubmit, submitLabel }: QuizRunnerProps) {
    const [choices, setChoices] = useState<Record<string, number>>({});
    const [numeric, setNumeric] = useState<Record<string, string>>({});
    const [coeffs, setCoeffs] = useState<Record<string, number[]>>({});
    const [graded, setGraded] = useState<Record<string, GradedItem> | null>(null);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const submit = async () => {
        setBusy(true);
        setError(null);
        try {
            const answers: SubmittedAnswer[] = items.map((it) => ({
                itemId: it.id,
                choiceIndex: choices[it.id],
                numeric: numeric[it.id],
                coefficients: coeffs[it.id],
                firstTry: true,
            }));
            const result = await onSubmit(answers);
            const byId: Record<string, GradedItem> = {};
            result.forEach((g) => (byId[g.itemId] = g));
            setGraded(byId);
        } catch (e) {
            setError(String(e));
        } finally {
            setBusy(false);
        }
    };

    return (
        <div>
            {error && <p className="quiz-feedback is-wrong">{error}</p>}
            {items.map((it) => (
                <div className="quiz-item" key={it.id}>
                    <p className="quiz-item__prompt">{it.prompt}</p>
                    {it.kind === "mc" && it.choices && (
                        <div role="radiogroup" aria-label={it.prompt}>
                            {it.choices.map((c, ci) => (
                                <button
                                    key={ci}
                                    type="button"
                                    role="radio"
                                    aria-checked={choices[it.id] === ci}
                                    disabled={!!graded}
                                    className={`quiz-choice ${choices[it.id] === ci ? "is-selected" : ""}`}
                                    onClick={() => setChoices((m) => ({ ...m, [it.id]: ci }))}
                                >
                                    {c}
                                </button>
                            ))}
                        </div>
                    )}
                    {it.kind === "numeric" && (
                        <div>
                            <input
                                type="text"
                                inputMode="decimal"
                                aria-label={`answer for ${it.prompt}`}
                                disabled={!!graded}
                                value={numeric[it.id] || ""}
                                onChange={(e) => setNumeric((m) => ({ ...m, [it.id]: e.target.value }))}
                                placeholder={it.unit ? `value in ${it.unit}` : "value"}
                                className="quiz-choice"
                                style={{ maxWidth: 220 }}
                            />
                            {!graded && numeric[it.id] && it.answer != null && it.tolerance && (
                                <span className="explainer__a11y">
                                    {" "}
                                    {hintLabel(previewNumeric(numeric[it.id], it.answer, it.unit ?? null, it.tolerance))}
                                </span>
                            )}
                        </div>
                    )}
                    {it.kind === "balance" && (
                        <BalanceInput item={it} disabled={!!graded} onChange={(c) => setCoeffs((m) => ({ ...m, [it.id]: c }))} />
                    )}
                    {it.kind === "multistep" && (
                        <p className="explainer__a11y">
                            Multi-step item — answer each step; grading is combined server-side.
                        </p>
                    )}
                    {graded && graded[it.id] && (
                        <div>
                            <p className={`quiz-feedback ${graded[it.id].correct ? "is-correct" : "is-wrong"}`}>
                                {graded[it.id].correct ? "Correct" : "Not quite"}
                            </p>
                            {(it.solution?.length || it.explanation) && (
                                <div className="quiz-solution">
                                    {it.solution?.map((s, si) => (
                                        <div key={si}>
                                            {s.text}
                                            {s.expression ? ` — ${s.expression}` : ""}
                                            {s.result ? ` = ${s.result}${s.unit ? " " + s.unit : ""}` : ""}
                                        </div>
                                    ))}
                                    {it.explanation && <div>{it.explanation}</div>}
                                </div>
                            )}
                        </div>
                    )}
                </div>
            ))}
            {!graded && (
                <button className="lessons-btn is-primary" type="button" onClick={submit} disabled={busy}>
                    {busy ? "Grading…" : submitLabel}
                </button>
            )}
        </div>
    );
}

function hintLabel(grade: "correct" | "wrong" | "unreadable"): string {
    if (grade === "correct") return "(looks right)";
    if (grade === "unreadable") return "(couldn't read that)";
    return "";
}

function BalanceInput({
    item,
    disabled,
    onChange,
}: {
    item: QuizItem;
    disabled: boolean;
    onChange: (coeffs: number[]) => void;
}) {
    const eq = (item.equation as any) || { reactants: [], products: [] };
    const species = [...(eq.reactants || []), ...(eq.products || [])];
    const [vals, setVals] = useState<number[]>(species.map(() => 1));
    return (
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap", alignItems: "center" }}>
            {species.map((s: any, i: number) => (
                <span key={i} style={{ display: "inline-flex", alignItems: "center", gap: 3 }}>
                    <input
                        type="number"
                        min={1}
                        max={12}
                        disabled={disabled}
                        value={vals[i]}
                        aria-label={`coefficient for ${s.formula}`}
                        className="quiz-choice"
                        style={{ width: 56 }}
                        onChange={(e) => {
                            const next = [...vals];
                            next[i] = Math.max(1, Number(e.target.value) || 1);
                            setVals(next);
                            onChange(next);
                        }}
                    />
                    <span>{s.formula}</span>
                    {i === (eq.reactants || []).length - 1 && <span aria-hidden>→</span>}
                </span>
            ))}
        </div>
    );
}
