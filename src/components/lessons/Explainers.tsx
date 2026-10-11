// Interactive explainers (docs/LESSONS.md §6). Each renders from its validated
// structured `data` payload — never model HTML. Correctness rules are
// authoritative in Rust; these components present the data and give a live,
// non-authoritative cue. Every interactive explainer carries a text-equivalent
// (the `a11yState` the author supplied) and honors prefers-reduced-motion via
// CSS. The convertAssetSrc below turns a bundled figure path into a local
// Tauri asset URL — never a remote fetch.

import { useMemo, useState } from "react";
import type { ReactElement } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { Explainer } from "../../lib/lessons";

export function ExplainerView({ explainer, figureDir }: { explainer: Explainer; figureDir?: string }) {
    const d = explainer.data as Record<string, any>;
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
                {balanced ? "Balanced — every element matches." : "Not balanced yet."}{" "}
                Answer: {answer.join(", ")}.
            </p>
        </div>
    );
}

function MoleRatioExplainer({ data }: { data: Record<string, any> }) {
    const given = (data.given as any) || {};
    const want = (data.want as any) || {};
    const final = (data.finalAnswer as any) || {};
    const [revealed, setRevealed] = useState(false);
    return (
        <div>
            <p className="explainer__a11y">
                Given {given.amount} {given.unit} of {given.species}, find {want.unit} of {want.species}.
            </p>
            <button className="lessons-btn" type="button" onClick={() => setRevealed((v) => !v)}>
                {revealed ? "Hide answer" : "Reveal answer"}
            </button>
            {revealed && (
                <p className="quiz-solution">
                    {final.value?.toFixed ? final.value.toFixed(3) : final.value} {final.unit}
                </p>
            )}
        </div>
    );
}

function LimitingReagentExplainer({ data }: { data: Record<string, any> }) {
    const reagents: any[] = (data.reagents as any[]) || [];
    const consumption = reagents.map((rg) => ({
        species: rg.species,
        moles: rg.available / rg.molarMass / (rg.coeff || 1),
    }));
    const min = Math.min(...consumption.map((c) => c.moles));
    return (
        <div>
            {consumption.map((c) => (
                <div className="explainer-row" key={c.species}>
                    <span className="explainer-row__label">{c.species}</span>
                    <div className="bar-track" style={{ flex: 1 }}>
                        <div
                            className={`bar-fill ${c.moles === min ? "" : "is-match"}`}
                            style={{ width: `${(min / c.moles) * 100}%` }}
                        />
                    </div>
                </div>
            ))}
            <p className="explainer__a11y">
                The bar that empties first (
                {consumption.find((c) => c.moles === min)?.species}) is the limiting reagent.
            </p>
        </div>
    );
}

function DimensionalAnalysisExplainer({ data }: { data: Record<string, any> }) {
    const start = (data.start as any) || {};
    const tiles: any[] = (data.tiles as any[]) || [];
    const target = (data.target as any) || {};
    let product = start.value || 0;
    tiles.forEach((t) => (product *= t.num / t.den));
    return (
        <div>
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap", alignItems: "center" }}>
                <span className="lesson-card__tag">{start.value} {start.unit}</span>
                {tiles.map((t, i) => (
                    <span key={i} className="lesson-card__tag">× {t.num} / {t.den}</span>
                ))}
                <span aria-hidden>=</span>
                <span className="lesson-card__badge">{product.toFixed(3)} {target.unit}</span>
            </div>
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
                        <text x={a.x * W} y={a.y * H + 4} textAnchor="middle" fontSize="12" fill="#050505">
                            {a.element}
                        </text>
                    </g>
                ))}
            </svg>
            <ul className="explainer__a11y">
                {atoms.map((a) => (
                    <li key={a.id}>{a.element}</li>
                ))}
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
