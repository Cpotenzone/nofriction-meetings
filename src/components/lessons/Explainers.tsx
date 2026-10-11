// Interactive explainers (docs/LESSONS.md §6). Each renders from its validated
// structured `data` payload — never model HTML. Correctness rules are
// authoritative in Rust; these components present the data and give a live,
// non-authoritative cue. Every interactive explainer carries a text-equivalent
// (the `a11yState` the author supplied) and honors prefers-reduced-motion via
// CSS. The convertAssetSrc below turns a bundled figure path into a local
// Tauri asset URL — never a remote fetch.

import { useEffect, useMemo, useState } from "react";
import type { ReactElement } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { Explainer } from "../../lib/lessons";

export function ExplainerView({
    explainer,
    figureDir,
    onSeen,
}: {
    explainer: Explainer;
    figureDir?: string;
    onSeen?: () => void;
}) {
    const d = explainer.data as Record<string, any>;
    // Rendering an explainer counts as the learner having reached it, so record
    // it for resume/progress (fire-and-forget via the parent's handler).
    useEffect(() => {
        onSeen?.();
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [explainer.id]);
    let body: ReactElement;
    switch (explainer.kind) {
        case "figure":
            body = <FigureExplainer data={d} figureDir={figureDir} />;
            break;
        case "equation_balancer":
            body = <BalancerExplainer data={d} />;
            break;
        case "mole_ratio":
            body = <MoleRatioExplainer data={d} />;
            break;
        case "limiting_reagent":
            body = <LimitingReagentExplainer data={d} />;
            break;
        case "dimensional_analysis":
            body = <DimensionalAnalysisExplainer data={d} />;
            break;
        case "molecule_diagram":
            body = <MoleculeDiagramExplainer data={d} />;
            break;
        case "titration_curve":
            body = <TitrationExplainer data={d} />;
            break;
        case "ideal_gas_pv":
            body = <IdealGasExplainer data={d} />;
            break;
        default:
            body = <p className="explainer__a11y">{explainer.caption}</p>;
    }
    return (
        <div className="explainer">
            {body}
            {explainer.a11yState && (
                <p className="explainer__a11y" role="note">
                    {explainer.a11yState}
                </p>
            )}
        </div>
    );
}

function FigureExplainer({ data, figureDir }: { data: Record<string, any>; figureDir?: string }) {
    const alt = (data.alt as string) || "Lesson figure";
    const figureId = data.figureId as string;
    const [failed, setFailed] = useState(false);
    // A present lesson's figures live on disk under the catalog dir; they are
    // served via the local Tauri asset protocol, never fetched remotely.
    const src = figureDir ? convertFileSrc(`${figureDir}/figures/${figureId}.png`) : "";
    if (failed || !src) {
        // Graceful degradation to the alt text (a transiently failed decode
        // never breaks the lesson — §6).
        return <p className="lesson-figure__caption">{alt}</p>;
    }
    return (
        <figure style={{ margin: 0 }}>
            <img className="lesson-figure" src={src} alt={alt} onError={() => setFailed(true)} />
            <figcaption className="lesson-figure__caption">{alt}</figcaption>
        </figure>
    );
}

// Parse "H2O" → {H:2, O:1} for the per-element balance meter (preview only).
function atomCounts(formula: string): Record<string, number> {
    const out: Record<string, number> = {};
    const re = /([A-Z][a-z]?)(\d*)/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(formula))) {
        if (!m[1]) continue;
        out[m[1]] = (out[m[1]] || 0) + (m[2] ? Number(m[2]) : 1);
    }
    return out;
}

function BalancerExplainer({ data }: { data: Record<string, any> }) {
    const eq = (data.equation as any) || { reactants: [], products: [] };
    const reactants: { formula: string }[] = eq.reactants || [];
    const products: { formula: string }[] = eq.products || [];
    const species = [...reactants, ...products];
    const answer: number[] = (data.answerCoefficients as number[]) || species.map(() => 1);
    const [coeffs, setCoeffs] = useState<number[]>(species.map(() => 1));

    const elements = useMemo(() => {
        const set = new Set<string>();
        species.forEach((s) => Object.keys(atomCounts(s.formula)).forEach((e) => set.add(e)));
        return [...set];
    }, [data]);

    const sideCount = (el: string, from: number, to: number) =>
        species.slice(from, to).reduce((acc, s, i) => acc + (atomCounts(s.formula)[el] || 0) * coeffs[from + i], 0);

    const r = reactants.length;
    const balanced = elements.every((el) => sideCount(el, 0, r) === sideCount(el, r, species.length));

    return (
        <div>
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
                {species.map((s, i) => (
                    <label key={i} style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
                        <input
                            type="number"
                            min={1}
                            max={12}
                            value={coeffs[i]}
                            aria-label={`coefficient for ${s.formula}`}
                            onChange={(e) => {
                                const next = [...coeffs];
                                next[i] = Math.max(1, Number(e.target.value) || 1);
                                setCoeffs(next);
                            }}
                        />
                        <span>{s.formula}</span>
                        {i === r - 1 && <span aria-hidden>→</span>}
                        {i < species.length - 1 && i !== r - 1 && <span aria-hidden>+</span>}
                    </label>
                ))}
            </div>
            <div style={{ marginTop: 10 }}>
                {elements.map((el) => {
                    const left = sideCount(el, 0, r);
                    const right = sideCount(el, r, species.length);
                    const max = Math.max(left, right, 1);
                    return (
                        <div className="explainer-row" key={el}>
                            <span className="explainer-row__label">{el}: {left} vs {right}</span>
                            <div className="bar-track" style={{ flex: 1 }}>
                                <div
                                    className={`bar-fill ${left === right ? "is-match" : ""}`}
                                    style={{ width: `${(Math.min(left, right) / max) * 100}%` }}
                                />
                            </div>
                        </div>
                    );
                })}
            </div>
            <p className="explainer__a11y">
                {balanced
                    ? `Balanced — every element matches. The coefficients are ${answer.join(", ")}.`
                    : "Not balanced yet. Adjust the coefficients until every element's left and right counts match."}
            </p>
        </div>
    );
}

// A step-by-step reveal (docs/LESSONS.md §6): the learner uncovers the
// conversion one stage at a time — set up the given quantity, apply the mole
// ratio / conversion factor, then read the result — instead of a single
// answer toggle. The stages are the generic stoichiometry chain (not fabricated
// chemistry); the numeric final value comes straight from the authored,
// Rust-verified `finalAnswer`. The answer only appears on the final step, so
// the a11y text does not pre-reveal it.
function MoleRatioExplainer({ data }: { data: Record<string, any> }) {
    const given = (data.given as any) || {};
    const want = (data.want as any) || {};
    const final = (data.finalAnswer as any) || {};
    const finalText =
        final.value != null
            ? `${typeof final.value === "number" ? final.value.toFixed(3) : final.value} ${final.unit ?? ""}`.trim()
            : "";

    const stages = [
        {
            label: "Set up the given quantity",
            body: `Start from ${given.amount ?? "?"} ${given.unit ?? ""} of ${given.species ?? "?"}.`,
        },
        {
            label: "Apply the conversion through moles",
            body: `Convert ${given.species ?? "the reactant"} to moles, then use the balanced-equation mole ratio to reach ${want.species ?? "the target"}.`,
        },
        {
            label: `Read the result in ${want.unit ?? "the target unit"}`,
            body: finalText ? `${want.species ?? ""} = ${finalText}`.trim() : "Result.",
        },
    ];
    // -1 means nothing revealed yet.
    const [shown, setShown] = useState(0);
    const done = shown >= stages.length;

    return (
        <div>
            <p className="explainer__a11y">
                Given {given.amount} {given.unit} of {given.species}, find {want.unit} of {want.species}.
                {done ? "" : " Reveal each step to work it through."}
            </p>
            <ol className="quiz-steps">
                {stages.slice(0, shown).map((s, i) => (
                    <li className="quiz-step" key={i}>
                        <strong>{s.label}</strong>
                        <div className="quiz-solution">{s.body}</div>
                    </li>
                ))}
            </ol>
            {!done ? (
                <button className="lessons-btn" type="button" onClick={() => setShown((n) => n + 1)}>
                    {shown === 0 ? "Start the step reveal" : "Next step"}
                </button>
            ) : (
                <button className="lessons-btn" type="button" onClick={() => setShown(0)}>
                    Reset
                </button>
            )}
        </div>
    );
}

// A limiting-reagent SIMULATOR (docs/LESSONS.md §6): the learner drives the
// reaction forward with a slider ("extent of reaction") and watches each
// reactant bar drain in proportion to its stoichiometric need. The bar that
// empties first is the limiting reagent; the sim stops there. The limiting
// species is only named once the sim has reached the point where a reactant is
// exhausted, so the a11y text does not spoil the exercise up front (NIT-9b).
function LimitingReagentExplainer({ data }: { data: Record<string, any> }) {
    const reagents: any[] = (data.reagents as any[]) || [];
    // Starting moles of each reactant and how many moles are consumed per unit
    // of reaction extent (its coefficient). The maximum reaction extent is set
    // by whichever reactant's moles/coeff is smallest — that reactant limits.
    const species = reagents.map((rg) => {
        const startMoles = rg.available / rg.molarMass;
        const coeff = rg.coeff || 1;
        return { species: rg.species, startMoles, coeff, maxExtent: startMoles / coeff };
    });
    const limitExtent = species.length ? Math.min(...species.map((s) => s.maxExtent)) : 0;
    const limiting = species.find((s) => Math.abs(s.maxExtent - limitExtent) < 1e-9);

    // extent runs 0..limitExtent; the slider is normalized 0..100.
    const [pct, setPct] = useState(0);
    const extent = (pct / 100) * limitExtent;
    const atCompletion = pct >= 100;

    return (
        <div>
            {species.map((s) => {
                const consumed = Math.min(s.startMoles, extent * s.coeff);
                const remaining = Math.max(0, s.startMoles - consumed);
                const fillPct = s.startMoles > 0 ? (remaining / s.startMoles) * 100 : 0;
                const empty = remaining <= 1e-9;
                return (
                    <div className="explainer-row" key={s.species}>
                        <span className="explainer-row__label">
                            {s.species}: {remaining.toFixed(2)} mol
                        </span>
                        <div className="bar-track" style={{ flex: 1 }}>
                            <div
                                className={`bar-fill ${empty ? "is-depleted" : ""}`}
                                style={{ width: `${fillPct}%` }}
                            />
                        </div>
                    </div>
                );
            })}
            <div className="explainer-row">
                <span className="explainer-row__label">Run reaction</span>
                <input
                    type="range"
                    min={0}
                    max={100}
                    step={1}
                    value={pct}
                    aria-label="extent of reaction"
                    onChange={(e) => setPct(Number(e.target.value))}
                />
            </div>
            <p className="explainer__a11y">
                {atCompletion
                    ? `${limiting?.species ?? "one reactant"} empties first, so it is the limiting reagent; the others are left in excess.`
                    : "Drag to run the reaction forward and watch which reactant empties first."}
            </p>
        </div>
    );
}

// A dimensional-analysis "railroad" BUILDER (docs/LESSONS.md §6): the learner
// places conversion tiles onto the track one at a time (click to place / click
// to remove — keyboard-operable buttons, not a mouse-only drag, so it is a11y
// friendly). As each tile lands, the running product updates live and the unit
// carried in from the left cancels against the tile's denominator (shown with a
// strike-through). The learner builds the whole chain to reach the target unit;
// the final numeric value matches the authored, Rust-verified `value`.
function DimensionalAnalysisExplainer({ data }: { data: Record<string, any> }) {
    const start = (data.start as any) || {};
    const tiles: any[] = (data.tiles as any[]) || [];
    const target = (data.target as any) || {};

    // placed[i] = whether tile i (in authored order) is on the track. Tiles are
    // placed left-to-right; the next placeable tile is the first unplaced one.
    const [placedCount, setPlacedCount] = useState(0);
    const placed = tiles.slice(0, placedCount);

    const startValue = typeof start.value === "number" ? start.value : 0;
    let product = startValue;
    placed.forEach((t) => (product *= (t.num ?? 1) / (t.den ?? 1)));

    const complete = placedCount >= tiles.length;
    const carriedUnit = complete ? target.unit : "…";

    return (
        <div>
            <div className="railroad" role="group" aria-label="dimensional-analysis railroad">
                <span className="railroad__tile is-start">
                    <span className="railroad__num">{startValue}</span>
                    <span className="railroad__unit">{start.unit}</span>
                </span>
                {placed.map((t, i) => {
                    // The denominator of this tile cancels the unit carried in
                    // from the tile to its left (strike it through to show the
                    // cancellation mechanic).
                    return (
                        <span className="railroad__tile" key={i}>
                            <span aria-hidden className="railroad__times">×</span>
                            <span className="railroad__frac">
                                <span className="railroad__num">{t.num}</span>
                                <span className="railroad__bar" aria-hidden />
                                <span className="railroad__den is-cancelled">{t.den}</span>
                            </span>
                        </span>
                    );
                })}
                <span aria-hidden className="railroad__times">=</span>
                <span className="lesson-card__badge">
                    {product.toFixed(3)} {carriedUnit}
                </span>
            </div>

            {!complete && tiles[placedCount] && (
                <div className="railroad__tray">
                    <span className="explainer-row__label">Next factor</span>
                    <button
                        type="button"
                        className="lessons-btn"
                        onClick={() => setPlacedCount((n) => n + 1)}
                    >
                        Place × {tiles[placedCount].num} / {tiles[placedCount].den}
                    </button>
                </div>
            )}
            {placedCount > 0 && (
                <button
                    type="button"
                    className="lessons-btn"
                    style={{ marginTop: 6 }}
                    onClick={() => setPlacedCount((n) => Math.max(0, n - 1))}
                >
                    Remove last factor
                </button>
            )}
            <p className="explainer__a11y">
                {complete
                    ? `All units cancel to leave ${target.unit}: the result is ${product.toFixed(3)} ${target.unit ?? ""}.`
                    : `Place each conversion factor so its denominator cancels the unit carried in. ${placedCount} of ${tiles.length} placed.`}
            </p>
        </div>
    );
}

function MoleculeDiagramExplainer({ data }: { data: Record<string, any> }) {
    const atoms: any[] = (data.atoms as any[]) || [];
    const bonds: any[] = (data.bonds as any[]) || [];
    const W = 240;
    const H = 140;
    return (
        <div>
            <svg viewBox={`0 0 ${W} ${H}`} width={W} height={H} role="img" aria-label="Molecule diagram">
                {bonds.map((b, i) => {
                    const a = atoms.find((x) => x.id === b.a);
                    const c = atoms.find((x) => x.id === b.b);
                    if (!a || !c) return null;
                    return (
                        <line
                            key={i}
                            x1={a.x * W}
                            y1={a.y * H}
                            x2={c.x * W}
                            y2={c.y * H}
                            stroke="var(--text-secondary, #a3a3a3)"
                            strokeWidth={b.order === 2 ? 4 : b.order === 3 ? 6 : 2}
                        />
                    );
                })}
                {atoms.map((a) => (
                    <g key={a.id}>
                        <circle cx={a.x * W} cy={a.y * H} r={14} fill="var(--hazard-yellow, #facc15)" />
                        <text x={a.x * W} y={a.y * H + 4} textAnchor="middle" fontSize="12" fill="var(--bg-main, #050505)">
                            {a.element}
                        </text>
                    </g>
                ))}
            </svg>
            <ul className="explainer__a11y">
                {atoms.map((a) => (
                    <li key={a.id}>{a.element}</li>
                ))}
                {bonds.map((b, i) => {
                    const a = atoms.find((x) => x.id === b.a);
                    const c = atoms.find((x) => x.id === b.b);
                    if (!a || !c) return null;
                    const order = b.order === 3 ? "triple" : b.order === 2 ? "double" : "single";
                    return (
                        <li key={`bond-${i}`}>
                            {order} bond: {a.element}–{c.element}
                        </li>
                    );
                })}
            </ul>
        </div>
    );
}

function TitrationExplainer({ data }: { data: Record<string, any> }) {
    const points: any[] = (data.points as any[]) || [];
    const equiv = data.equivalenceVol as number;
    const [idx, setIdx] = useState(0);
    const W = 280;
    const H = 160;
    const maxV = Math.max(...points.map((p) => p.volAdded), 1);
    const path = points
        .map((p, i) => `${i === 0 ? "M" : "L"} ${(p.volAdded / maxV) * W} ${H - (p.pH / 14) * H}`)
        .join(" ");
    const cur = points[idx] || points[0];
    return (
        <div>
            <svg viewBox={`0 0 ${W} ${H}`} width={W} height={H} role="img" aria-label="Titration curve">
                <path d={path} fill="none" stroke="var(--hazard-yellow, #facc15)" strokeWidth={2} />
                {equiv != null && (
                    <line
                        x1={(equiv / maxV) * W}
                        y1={0}
                        x2={(equiv / maxV) * W}
                        y2={H}
                        stroke="var(--accent-success, #34d399)"
                        strokeDasharray="4 3"
                    />
                )}
                {cur && (
                    <circle cx={(cur.volAdded / maxV) * W} cy={H - (cur.pH / 14) * H} r={4} fill="var(--text-primary, #f5f5f5)" />
                )}
            </svg>
            <div className="explainer-row">
                <span className="explainer-row__label">Titrant</span>
                <input
                    type="range"
                    min={0}
                    max={points.length - 1}
                    value={idx}
                    aria-label="titrant volume"
                    onChange={(e) => setIdx(Number(e.target.value))}
                />
            </div>
            <p className="explainer__a11y">
                At {cur?.volAdded} mL the pH is {cur?.pH}. Equivalence at {equiv} mL.
            </p>
        </div>
    );
}

function IdealGasExplainer({ data }: { data: Record<string, any> }) {
    const n = (data.n as number) || 1;
    const T = (data.T as number) || 273.15;
    const R = (data.R as number) || 0.0821;
    const bounds = (data.bounds as any) || { pMin: 0.5, pMax: 2 };
    const [p, setP] = useState((bounds.pMin + bounds.pMax) / 2);
    const v = (n * R * T) / p;
    return (
        <div>
            <div className="explainer-row">
                <span className="explainer-row__label">Pressure (atm)</span>
                <input
                    type="range"
                    min={bounds.pMin}
                    max={bounds.pMax}
                    step={0.05}
                    value={p}
                    aria-label="pressure in atm"
                    onChange={(e) => setP(Number(e.target.value))}
                />
            </div>
            <p className="explainer__a11y">
                PV = nRT: at {p.toFixed(2)} atm, n = {n}, T = {T} K, the volume is {v.toFixed(2)} L.
            </p>
        </div>
    );
}
