// Lessons — PREVIEW-ONLY client logic (docs/LESSONS.md §7/§16, MEDIUM-6).
//
// Nothing here decides pass/fail or writes a stored score. The authoritative
// grader, mastery math, and exam scoring are in Rust (`lessons::grade`,
// `lessons::mastery`, `lessons::exam`) and reached through Tauri commands. This
// module provides live UI hints only: a local numeric parse/compare for an
// instant "looks right" cue, a mastery preview for a progress bar before the
// server round-trip, and resume-state helpers. All of it is replaced by the
// server result the moment a command returns.

export interface NumericTolerance {
    kind: "exact" | "abs" | "rel";
    value: number;
}

// A small units table mirroring the Rust UNITS table (§9). Preview only — the
// Rust grader is authoritative. value_in_base = given*factor + offset.
interface UnitDef {
    dim: string;
    factor: number;
    offset: number;
}
// Mirrors the authoritative Rust `grade::UNITS` table 1:1 so the preview hint
// never shows a false "unreadable" for a unit the Rust grader accepts (NIT-8).
const UNITS: Record<string, UnitDef> = {
    // mass (base g)
    g: { dim: "mass", factor: 1, offset: 0 },
    grams: { dim: "mass", factor: 1, offset: 0 },
    gram: { dim: "mass", factor: 1, offset: 0 },
    mg: { dim: "mass", factor: 1e-3, offset: 0 },
    "µg": { dim: "mass", factor: 1e-6, offset: 0 },
    ug: { dim: "mass", factor: 1e-6, offset: 0 },
    kg: { dim: "mass", factor: 1e3, offset: 0 },
    // volume (base L)
    l: { dim: "volume", factor: 1, offset: 0 },
    liters: { dim: "volume", factor: 1, offset: 0 },
    liter: { dim: "volume", factor: 1, offset: 0 },
    ml: { dim: "volume", factor: 1e-3, offset: 0 },
    dl: { dim: "volume", factor: 0.1, offset: 0 },
    "cm³": { dim: "volume", factor: 1e-3, offset: 0 },
    cm3: { dim: "volume", factor: 1e-3, offset: 0 },
    cc: { dim: "volume", factor: 1e-3, offset: 0 },
    // amount (base mol)
    mol: { dim: "amount", factor: 1, offset: 0 },
    mole: { dim: "amount", factor: 1, offset: 0 },
    moles: { dim: "amount", factor: 1, offset: 0 },
    mmol: { dim: "amount", factor: 1e-3, offset: 0 },
    // pressure (base atm)
    atm: { dim: "pressure", factor: 1, offset: 0 },
    kpa: { dim: "pressure", factor: 1 / 101.325, offset: 0 },
    mmhg: { dim: "pressure", factor: 1 / 760, offset: 0 },
    torr: { dim: "pressure", factor: 1 / 760, offset: 0 },
    pa: { dim: "pressure", factor: 1 / 101325, offset: 0 },
    // temperature (base K) — °C uses an additive offset, not a factor
    k: { dim: "temperature", factor: 1, offset: 0 },
    "°c": { dim: "temperature", factor: 1, offset: 273.15 },
    c: { dim: "temperature", factor: 1, offset: 273.15 },
    "°k": { dim: "temperature", factor: 1, offset: 0 },
};

export type PreviewGrade = "correct" | "wrong" | "unreadable";

// Parse a user-entered number, accepting plain decimals and scientific
// notation (6.022e23, 6.022 x 10^23), with an optional trailing unit token.
function parseGiven(raw: string): { value: number; unit: string | null } | null {
    const s = raw.trim().toLowerCase();
    if (s === "") return null;
    // Normalize "x 10^n" / "× 10^n" to e-notation.
    const normalized = s.replace(/\s*[x×]\s*10\s*\^\s*/i, "e");
    // The number is matched first (greedy, incl. scientific notation); the
    // optional trailing unit token may contain letters, the µ/° symbols, and
    // digits (e.g. cm3) — matching the tokens in the Rust UNITS table.
    const m = normalized.match(/^([+-]?\d*\.?\d+(?:e[+-]?\d+)?)\s*([a-zµ°][a-zµ°0-9³]*)?$/i);
    if (!m) return null;
    const value = Number(m[1]);
    if (!Number.isFinite(value)) return null;
    return { value, unit: m[2] ? m[2] : null };
}

// A live, non-authoritative numeric cue. Mirrors the Rust check_numeric policy:
// convert a compatible unit, mark incompatible units wrong, treat an unknown
// unit as unreadable, and compare within the item's tolerance.
export function previewNumeric(
    given: string,
    answer: number,
    itemUnit: string | null,
    tolerance: NumericTolerance,
): PreviewGrade {
    const parsed = parseGiven(given);
    if (!parsed) return "unreadable";
    let magnitude = parsed.value;
    if (parsed.unit) {
        const gu = UNITS[parsed.unit];
        if (!gu) return "unreadable";
        if (itemUnit) {
            const iu = UNITS[itemUnit.trim().toLowerCase()];
            if (!iu) return "unreadable";
            if (iu.dim !== gu.dim) return "wrong";
            const base = parsed.value * gu.factor + gu.offset;
            magnitude = (base - iu.offset) / iu.factor;
        }
    }
    return withinTolerance(magnitude, answer, tolerance) ? "correct" : "wrong";
}

function withinTolerance(given: number, answer: number, tol: NumericTolerance): boolean {
    switch (tol.kind) {
        case "abs":
            return Math.abs(given - answer) <= tol.value;
        case "rel":
            return Math.abs(given - answer) <= tol.value * Math.abs(answer);
        case "exact":
        default: {
            const eps = Math.max(1e-9, Math.abs(answer) * 1e-9);
            return Math.abs(given - answer) <= eps;
        }
    }
}

// Preview mastery EMA (mirrors Rust constants). Not stored; the server value
// overwrites it after each graded attempt.
export const PREVIEW_ALPHA = 0.4;
export function previewMastery(prevScore: number, attempts: number, correct: boolean, firstTry: boolean): number {
    const itemScore = correct ? (firstTry ? 100 : 70) : 0;
    if (attempts === 0) return itemScore;
    return PREVIEW_ALPHA * itemScore + (1 - PREVIEW_ALPHA) * prevScore;
}

// Resume: the next unseen explainer index, for a "pick up where you left off"
// cue in the player. Pure UI state.
export function resumeExplainerIndex(explainerIds: string[], seen: string[]): number {
    const seenSet = new Set(seen);
    const idx = explainerIds.findIndex((id) => !seenSet.has(id));
    return idx === -1 ? Math.max(0, explainerIds.length - 1) : idx;
}

// Exam-unlock predicate preview (the real gate is enforced server-side in
// lessons_start_exam). All checkpoints passed → eligible.
export function examUnlockedPreview(checkpointsPassed: number, totalLessons: number): boolean {
    return totalLessons > 0 && checkpointsPassed >= totalLessons;
}
