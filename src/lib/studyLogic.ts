// Moment markers and study tools: pure logic, tested in studyLogic.test.ts
// (`npm test`). No Tauri imports here, so node's test runner can load it.
// docs/STUDY_TOOLS.md

// ── Markers ─────────────────────────────────────────────────────────────

export type MarkerKind = "important" | "question" | "test";

export const MARKER_KINDS: MarkerKind[] = ["important", "question", "test"];

export const MARKER_META: Record<MarkerKind, { symbol: string; label: string; hint: string }> = {
    important: { symbol: "★", label: "Important", hint: "Something that matters" },
    question: { symbol: "?", label: "Question", hint: "Confused, or something to ask" },
    test: { symbol: "✎", label: "On the test", hint: "Said to be on the exam" },
};

export interface Marker {
    id: string;
    meeting_id: string;
    /** Wall clock, RFC 3339 */
    ts: string;
    kind: MarkerKind;
    note: string | null;
    created_at: string;
    /** ms from the meeting start (the Recordings timeline's clock) */
    offset_ms: number;
}

export type MarkerFilter = "all" | MarkerKind;

export function isMarkerKind(k: unknown): k is MarkerKind {
    return k === "important" || k === "question" || k === "test";
}

/** The markers to list for a filter, in time order. */
export function filterMarkers(markers: Marker[], filter: MarkerFilter): Marker[] {
    return markers
        .filter((m) => filter === "all" || m.kind === filter)
        .slice()
        .sort((a, b) => a.offset_ms - b.offset_ms || a.created_at.localeCompare(b.created_at));
}

export function countByKind(markers: Marker[]): Record<MarkerKind, number> {
    const out: Record<MarkerKind, number> = { important: 0, question: 0, test: 0 };
    for (const m of markers) if (isMarkerKind(m.kind)) out[m.kind]++;
    return out;
}

/** 754000 → "12:34", 3725000 → "1:02:05" (same as the backend's clock). */
export function clock(ms: number): string {
    const s = Math.max(0, Math.floor(ms / 1000));
    const pad = (n: number) => String(n).padStart(2, "0");
    return s >= 3600
        ? `${Math.floor(s / 3600)}:${pad(Math.floor((s % 3600) / 60))}:${pad(s % 60)}`
        : `${Math.floor(s / 60)}:${pad(s % 60)}`;
}

/**
 * Where markers sit in the transcript: after the line being spoken when
 * the mark was made (the last line that starts at or before it), or before
 * the first line when the mark came first.
 */
export function placeMarkers(
    lines: { id: string; timestamp_ms: number }[],
    markers: Marker[],
): { before: Marker[]; after: Map<string, Marker[]> } {
    const sorted = lines.slice().sort((a, b) => a.timestamp_ms - b.timestamp_ms);
    const before: Marker[] = [];
    const after = new Map<string, Marker[]>();
    for (const m of filterMarkers(markers, "all")) {
        let host: string | null = null;
        for (const l of sorted) {
            if (l.timestamp_ms <= m.offset_ms) host = l.id;
            else break;
        }
        if (host === null) before.push(m);
        else after.set(host, [...(after.get(host) ?? []), m]);
    }
    return { before, after };
}

/** Markers inside any of the spans (ms, both ends included): what a
 *  time-range Delete removes when it commits. */
export function markersInSpans(markers: Marker[], spans: [number, number][]): Set<string> {
    return new Set(markers.filter((m) => spans.some(([a, b]) => m.offset_ms >= a && m.offset_ms <= b)).map((m) => m.id));
}

// ── Randomness (injectable so tests are deterministic) ──────────────────

export type Rng = () => number;

/** Small seeded generator (mulberry32) for tests and repeatable shuffles. */
export function seededRng(seed: number): Rng {
    let a = seed >>> 0;
    return () => {
        a = (a + 0x6d2b79f5) >>> 0;
        let t = a;
        t = Math.imul(t ^ (t >>> 15), t | 1);
        t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
}

/** Fisher–Yates; returns a new array. */
export function shuffled<T>(items: T[], rng: Rng = Math.random): T[] {
    const a = items.slice();
    for (let i = a.length - 1; i > 0; i--) {
        const j = Math.floor(rng() * (i + 1));
        [a[i], a[j]] = [a[j], a[i]];
    }
    return a;
}

// ── Flashcards ──────────────────────────────────────────────────────────

export interface Card {
    front: string;
    back: string;
}

/**
 * A study session over cards 0..n-1. Each round goes through `order`;
 * "Again" cards come back in the next round until every card is known.
 */
export interface DeckState {
    /** Card indexes for this round */
    order: number[];
    /** Position in `order` */
    pos: number;
    /** Showing the back */
    flipped: boolean;
    /** Known, in the order they were marked */
    known: number[];
    /** Marked "Again" this round */
    again: number[];
    round: number;
}

export function newDeck(n: number): DeckState {
    return { order: Array.from({ length: Math.max(0, n) }, (_, i) => i), pos: 0, flipped: false, known: [], again: [], round: 1 };
}

export function deckDone(s: DeckState): boolean {
    return s.pos >= s.order.length;
}

export function currentCard(s: DeckState): number | null {
    return deckDone(s) ? null : s.order[s.pos];
}

export function flip(s: DeckState): DeckState {
    return deckDone(s) ? s : { ...s, flipped: !s.flipped };
}

/** Known / Again on the current card, then the next card (or round). */
export function markCard(s: DeckState, result: "known" | "again"): DeckState {
    const card = currentCard(s);
    if (card === null) return s;
    const known = result === "known" ? [...s.known.filter((k) => k !== card), card] : s.known.filter((k) => k !== card);
    const again = result === "again" ? [...s.again, card] : s.again;
    const pos = s.pos + 1;
    if (pos < s.order.length) return { ...s, pos, flipped: false, known, again };
    if (again.length > 0) {
        // Next round: only the cards still to learn
        return { order: again, pos: 0, flipped: false, known, again: [], round: s.round + 1 };
    }
    return { ...s, pos, flipped: false, known, again };
}

/** Shuffle the cards not yet seen this round. */
export function shuffleDeck(s: DeckState, rng: Rng = Math.random): DeckState {
    if (deckDone(s)) return s;
    const seen = s.order.slice(0, s.pos);
    return { ...s, order: [...seen, ...shuffled(s.order.slice(s.pos), rng)], flipped: false };
}

export function deckProgress(s: DeckState, total: number): { known: number; left: number; total: number } {
    return { known: s.known.length, left: Math.max(0, total - s.known.length), total };
}

// ── Quiz ────────────────────────────────────────────────────────────────

export interface QuizQuestion {
    question: string;
    choices: string[];
    /** 0-based index of the correct choice */
    answer: number;
    explanation: string;
    /** Transcript time the answer comes from (ms from the meeting start) */
    at_ms: number | null;
}

export interface QuizState {
    pos: number;
    /** The choice picked for each question (null: not answered yet) */
    picked: (number | null)[];
    finished: boolean;
}

export function newQuiz(n: number): QuizState {
    return { pos: 0, picked: Array.from({ length: Math.max(0, n) }, () => null), finished: n <= 0 };
}

/** Answer the current question (once; a second pick is ignored). */
export function pickAnswer(s: QuizState, qs: QuizQuestion[], choice: number): QuizState {
    const q = qs[s.pos];
    if (s.finished || !q || s.picked[s.pos] !== null || choice < 0 || choice >= q.choices.length) return s;
    const picked = s.picked.slice();
    picked[s.pos] = choice;
    return { ...s, picked };
}

export function isCorrect(q: QuizQuestion, choice: number | null): boolean {
    return choice !== null && choice === q.answer;
}

/** To the next question once this one is answered; the last one finishes. */
export function nextQuestion(s: QuizState, qs: QuizQuestion[]): QuizState {
    if (s.finished || s.picked[s.pos] === null) return s;
    if (s.pos + 1 >= qs.length) return { ...s, finished: true };
    return { ...s, pos: s.pos + 1 };
}

export function quizScore(s: QuizState, qs: QuizQuestion[]): { correct: number; answered: number; total: number } {
    let correct = 0;
    let answered = 0;
    qs.forEach((q, i) => {
        const p = s.picked[i] ?? null;
        if (p !== null) {
            answered++;
            if (isCorrect(q, p)) correct++;
        }
    });
    return { correct, answered, total: qs.length };
}

/** Questions answered wrong, to retry. */
export function missedQuestions(s: QuizState, qs: QuizQuestion[]): number[] {
    return qs.map((q, i) => (isCorrect(q, s.picked[i] ?? null) ? -1 : i)).filter((i) => i >= 0);
}

// ── Stored guide (as get_study_guide returns it) ────────────────────────

export type StudyPart = "summary" | "terms" | "flashcards" | "quiz" | "questions";

export const STUDY_PARTS: { kind: StudyPart; label: string }[] = [
    { kind: "summary", label: "Summary" },
    { kind: "terms", label: "Key terms" },
    { kind: "flashcards", label: "Flashcards" },
    { kind: "quiz", label: "Practice quiz" },
    { kind: "questions", label: "Questions to ask" },
];

/** Cards from stored flashcards data; anything malformed is skipped. */
export function cardsOf(data: unknown): Card[] {
    const cards = (data as { cards?: unknown })?.cards;
    if (!Array.isArray(cards)) return [];
    return cards.filter(
        (c): c is Card => !!c && typeof (c as Card).front === "string" && typeof (c as Card).back === "string",
    );
}

/** Quiz questions from stored quiz data; anything malformed is skipped. */
export function quizOf(data: unknown): QuizQuestion[] {
    const qs = (data as { questions?: unknown })?.questions;
    if (!Array.isArray(qs)) return [];
    return qs.filter((q): q is QuizQuestion => {
        const x = q as QuizQuestion;
        return (
            !!x &&
            typeof x.question === "string" &&
            Array.isArray(x.choices) &&
            x.choices.length >= 2 &&
            x.choices.every((c) => typeof c === "string") &&
            Number.isInteger(x.answer) &&
            x.answer >= 0 &&
            x.answer < x.choices.length
        );
    }).map((x) => ({ ...x, explanation: typeof x.explanation === "string" ? x.explanation : "", at_ms: typeof x.at_ms === "number" ? x.at_ms : null }));
}
