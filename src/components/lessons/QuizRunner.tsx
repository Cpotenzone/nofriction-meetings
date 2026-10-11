// A shared quiz runner for checkpoints and the exam (docs/LESSONS.md §8.3,
// §10). It collects answers and, after submission, reveals the full worked
// solution per item (grading is authoritative in Rust; this never decides
// pass/fail). Keyboard operable, with a live preview cue for numeric answers.
//
// Two assessment-integrity properties are enforced here on the presentation
// side so they hold for every bank (checkpoints, exam, trainers, diagnostic),
// independent of how an authored bundle distributes its keys:
//   1. MC choices are shuffled per item at presentation and the chosen screen
//      position is mapped back to the AUTHORED index before submit, so always
//      clicking the first choice no longer scores 100%.
//   2. Multistep items render a real per-step input (mc / numeric / balance)
//      and populate SubmittedAnswer.multistep, so they are gradeable from the
//      UI instead of silently scoring 0.

import { useMemo, useState } from "react";
import type { QuizItem, SubmittedAnswer, GradedItem, MultistepStep } from "../../lib/lessons";
import { previewNumeric } from "../../lib/lessonLogic";

interface QuizRunnerProps {
    items: QuizItem[];
    onSubmit: (answers: SubmittedAnswer[]) => Promise<GradedItem[]>;
    submitLabel: string;
}

// A deterministic per-item shuffle: Fisher–Yates seeded from the item id so the
// order is stable across re-renders of the same item but differs between items
// (and between a retake that draws different ids). Returns the authored indices
// in presentation order.
function shuffledOrder(id: string, n: number): number[] {
    let seed = 0;
    for (let i = 0; i < id.length; i++) seed = (seed * 31 + id.charCodeAt(i)) >>> 0;
    const rng = () => {
        // xorshift32
        seed ^= seed << 13;
        seed ^= seed >>> 17;
        seed ^= seed << 5;
        seed >>>= 0;
        return seed / 0xffffffff;
    };
    const order = Array.from({ length: n }, (_, i) => i);
    for (let i = n - 1; i > 0; i--) {
        const j = Math.floor(rng() * (i + 1));
        [order[i], order[j]] = [order[j], order[i]];
    }
    return order;
}

export function QuizRunner({ items, onSubmit, submitLabel }: QuizRunnerProps) {
    // `choices` stores the AUTHORED index the learner selected (already mapped
    // back from the shuffled screen position).
    const [choices, setChoices] = useState<Record<string, number>>({});
    const [numeric, setNumeric] = useState<Record<string, string>>({});
    const [coeffs, setCoeffs] = useState<Record<string, number[]>>({});
    const [steps, setSteps] = useState<Record<string, StepAnswer[]>>({});
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
                multistep:
                    it.kind === "multistep"
                        ? (it.steps ?? []).map((_, si) => {
                              const a = steps[it.id]?.[si] ?? {};
                              return {
                                  choiceIndex: a.choiceIndex,
                                  numeric: a.numeric,
                                  coefficients: a.coefficients,
                              };
                          })
                        : undefined,
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

    const setStepAnswer = (itemId: string, stepIndex: number, patch: Partial<StepAnswer>) => {
        setSteps((m) => {
            const cur = m[itemId] ? [...m[itemId]] : [];
            cur[stepIndex] = { ...(cur[stepIndex] ?? {}), ...patch };
            return { ...m, [itemId]: cur };
        });
    };

    return (
        <div>
            {error && <p className="quiz-feedback is-wrong">{error}</p>}
            {items.map((it) => (
                <div className="quiz-item" key={it.id}>
                    <p className="quiz-item__prompt">{it.prompt}</p>
                    {it.kind === "mc" && it.choices && (
                        <ChoiceGroup
                            itemId={it.id}
                            prompt={it.prompt}
                            choices={it.choices}
                            disabled={!!graded}
                            selectedAuthored={choices[it.id]}
                            onSelect={(authored) => setChoices((m) => ({ ...m, [it.id]: authored }))}
                        />
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
                        <MultistepInput
                            itemId={it.id}
                            steps={it.steps ?? []}
                            disabled={!!graded}
                            answers={steps[it.id] ?? []}
                            onStep={(si, patch) => setStepAnswer(it.id, si, patch)}
                        />
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

// One learner answer for a single multistep step.
interface StepAnswer {
    choiceIndex?: number;
    numeric?: string;
    coefficients?: number[];
}

// A multiple-choice radiogroup whose choices are shuffled at presentation. The
// `onSelect` callback reports the AUTHORED index (the position in the original
// `choices` array), so grading against the stored answerIndex is unaffected
// while gaming "always pick the first option" no longer works.
function ChoiceGroup({
    itemId,
    prompt,
    choices,
    disabled,
    selectedAuthored,
    onSelect,
}: {
    itemId: string;
    prompt: string;
    choices: string[];
    disabled: boolean;
    selectedAuthored?: number;
    onSelect: (authoredIndex: number) => void;
}) {
    const order = useMemo(() => shuffledOrder(itemId, choices.length), [itemId, choices.length]);
    return (
        <div role="radiogroup" aria-label={prompt}>
            {order.map((authored) => (
                <button
                    key={authored}
                    type="button"
                    role="radio"
                    aria-checked={selectedAuthored === authored}
                    disabled={disabled}
                    className={`quiz-choice ${selectedAuthored === authored ? "is-selected" : ""}`}
                    onClick={() => onSelect(authored)}
                >
                    {choices[authored]}
                </button>
            ))}
        </div>
    );
}

// Renders each step of a multistep item with the right input for its kind and
// collects the answers so SubmittedAnswer.multistep is populated. Steps are
// shown in authored order (the chain is sequential by design); MC steps shuffle
// their choices like standalone MC items.
function MultistepInput({
    itemId,
    steps,
    disabled,
    answers,
    onStep,
}: {
    itemId: string;
    steps: MultistepStep[];
    disabled: boolean;
    answers: StepAnswer[];
    onStep: (stepIndex: number, patch: Partial<StepAnswer>) => void;
}) {
    if (steps.length === 0) {
        return <p className="explainer__a11y">This item has no steps to answer.</p>;
    }
    return (
        <ol className="quiz-steps">
            {steps.map((step, si) => (
                <li key={si} className="quiz-step">
                    <p className="quiz-item__prompt">{step.prompt}</p>
                    {step.kind === "mc" && step.choices && (
                        <ChoiceGroup
                            itemId={`${itemId}-s${si}`}
                            prompt={step.prompt}
                            choices={step.choices}
                            disabled={disabled}
                            selectedAuthored={answers[si]?.choiceIndex}
                            onSelect={(authored) => onStep(si, { choiceIndex: authored })}
                        />
                    )}
                    {step.kind === "numeric" && (
                        <input
                            type="text"
                            inputMode="decimal"
                            aria-label={`answer for ${step.prompt}`}
                            disabled={disabled}
                            value={answers[si]?.numeric ?? ""}
                            onChange={(e) => onStep(si, { numeric: e.target.value })}
                            placeholder={step.unit ? `value in ${step.unit}` : "value"}
                            className="quiz-choice"
                            style={{ maxWidth: 220 }}
                        />
                    )}
                    {step.kind === "balance" && (
                        <StepBalanceInput
                            step={step}
                            disabled={disabled}
                            onChange={(c) => onStep(si, { coefficients: c })}
                        />
                    )}
                </li>
            ))}
        </ol>
    );
}

function hintLabel(grade: "correct" | "wrong" | "unreadable"): string {
    if (grade === "correct") return "(looks right)";
    if (grade === "unreadable") return "(couldn't read that)";
    return "";
}

// A coefficient entry for an equation, shared shape used by both standalone
// balance items and multistep balance steps.
function CoefficientRow({
    equation,
    disabled,
    onChange,
}: {
    equation: unknown;
    disabled: boolean;
    onChange: (coeffs: number[]) => void;
}) {
    const eq = (equation as any) || { reactants: [], products: [] };
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

function BalanceInput({
    item,
    disabled,
    onChange,
}: {
    item: QuizItem;
    disabled: boolean;
    onChange: (coeffs: number[]) => void;
}) {
    return <CoefficientRow equation={item.equation} disabled={disabled} onChange={onChange} />;
}

function StepBalanceInput({
    step,
    disabled,
    onChange,
}: {
    step: MultistepStep;
    disabled: boolean;
    onChange: (coeffs: number[]) => void;
}) {
    return <CoefficientRow equation={step.equation} disabled={disabled} onChange={onChange} />;
}
