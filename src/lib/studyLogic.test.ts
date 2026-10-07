// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    cardsOf,
    clock,
    countByKind,
    currentCard,
    deckDone,
    deckProgress,
    filterMarkers,
    flip,
    isCorrect,
    markCard,
    markersInSpans,
    missedQuestions,
    newDeck,
    newQuiz,
    nextQuestion,
    pickAnswer,
    placeMarkers,
    quizOf,
    quizScore,
    seededRng,
    shuffleDeck,
    shuffled,
    type Marker,
    type QuizQuestion,
} from "./studyLogic.ts";

const mk = (id: string, offset_ms: number, kind: Marker["kind"] = "important", note: string | null = null): Marker => ({
    id,
    meeting_id: "m1",
    ts: new Date(Date.UTC(2026, 9, 6, 10, 0, 0) + offset_ms).toISOString(),
    kind,
    note,
    created_at: "2026-10-06T10:00:00Z",
    offset_ms,
});

// ── Markers ─────────────────────────────────────────────────────────────

test("filter markers by kind, in time order", () => {
    const ms = [mk("c", 300, "test"), mk("a", 100, "question"), mk("b", 200, "test"), mk("d", 50)];
    assert.deepEqual(filterMarkers(ms, "all").map((m) => m.id), ["d", "a", "b", "c"]);
    assert.deepEqual(filterMarkers(ms, "test").map((m) => m.id), ["b", "c"], "everything marked for the test");
    assert.deepEqual(filterMarkers(ms, "question").map((m) => m.id), ["a"]);
    assert.deepEqual(countByKind(ms), { important: 1, question: 1, test: 2 });
    assert.deepEqual(ms.map((m) => m.id), ["c", "a", "b", "d"], "input not reordered");
});

test("markers sit after the line being spoken", () => {
    const lines = [
        { id: "l1", timestamp_ms: 1_000 },
        { id: "l2", timestamp_ms: 10_000 },
        { id: "l3", timestamp_ms: 20_000 },
    ];
    const placed = placeMarkers(lines, [mk("early", 500), mk("x", 12_000), mk("y", 10_000, "test"), mk("late", 99_000)]);
    assert.deepEqual(placed.before.map((m) => m.id), ["early"]);
    assert.deepEqual(placed.after.get("l2")?.map((m) => m.id), ["y", "x"]);
    assert.deepEqual(placed.after.get("l3")?.map((m) => m.id), ["late"]);
    assert.equal(placed.after.get("l1"), undefined);
    // No lines yet: all before
    assert.deepEqual(placeMarkers([], [mk("a", 5)]).before.map((m) => m.id), ["a"]);
});

test("markers inside a deleted time range are found (ends included)", () => {
    const ms = [mk("a", 999), mk("b", 1_000), mk("c", 5_000), mk("d", 5_001), mk("e", 9_000)];
    assert.deepEqual([...markersInSpans(ms, [[1_000, 5_000], [8_000, 9_000]])].sort(), ["b", "c", "e"]);
    assert.equal(markersInSpans(ms, []).size, 0);
});

test("clock matches the backend", () => {
    assert.equal(clock(0), "0:00");
    assert.equal(clock(754_000), "12:34");
    assert.equal(clock(3_725_000), "1:02:05");
    assert.equal(clock(-5), "0:00");
});

// ── Flashcards ──────────────────────────────────────────────────────────

test("flip, known and again: again cards come back until all are known", () => {
    let d = newDeck(3);
    assert.equal(currentCard(d), 0);
    assert.equal(flip(d).flipped, true);
    assert.equal(flip(flip(d)).flipped, false);
    d = markCard(flip(d), "known"); // 0 known
    assert.equal(d.flipped, false, "next card shows its front");
    d = markCard(d, "again"); // 1 again
    d = markCard(d, "known"); // 2 known
    assert.equal(d.round, 2);
    assert.deepEqual(d.order, [1]);
    assert.equal(currentCard(d), 1);
    assert.deepEqual(deckProgress(d, 3), { known: 2, left: 1, total: 3 });
    d = markCard(d, "again");
    assert.equal(d.round, 3, "still not known: another round");
    d = markCard(d, "known");
    assert.ok(deckDone(d));
    assert.deepEqual(deckProgress(d, 3), { known: 3, left: 0, total: 3 });
    assert.equal(currentCard(d), null);
    assert.deepEqual(markCard(d, "known"), d, "nothing after the end");
    assert.ok(deckDone(newDeck(0)));
});

test("shuffle reorders only the cards not yet seen, deterministically with a seed", () => {
    let d = newDeck(10);
    d = markCard(d, "known");
    d = markCard(d, "again");
    const s = shuffleDeck(flip(d), seededRng(42));
    assert.deepEqual(s.order.slice(0, 2), [0, 1], "seen cards stay put");
    assert.deepEqual(s.order.slice().sort((a, b) => a - b), [...Array(10).keys()], "same cards");
    assert.equal(s.flipped, false);
    assert.deepEqual(s, shuffleDeck(flip(d), seededRng(42)));
    assert.notDeepEqual(s.order.slice(2), d.order.slice(2));
    const arr = [1, 2, 3, 4, 5];
    assert.deepEqual(shuffled(arr, seededRng(1)).sort(), arr);
    assert.deepEqual(arr, [1, 2, 3, 4, 5], "input untouched");
});

// ── Quiz ────────────────────────────────────────────────────────────────

const qs: QuizQuestion[] = [
    { question: "Q1", choices: ["a", "b", "c", "d"], answer: 2, explanation: "c", at_ms: 1_000 },
    { question: "Q2", choices: ["a", "b"], answer: 0, explanation: "a", at_ms: null },
    { question: "Q3", choices: ["x", "y", "z"], answer: 1, explanation: "y", at_ms: 3_000 },
];

test("answer once, see right or wrong, move on, score at the end", () => {
    let s = newQuiz(qs.length);
    assert.deepEqual(nextQuestion(s, qs), s, "can't skip an unanswered question");
    s = pickAnswer(s, qs, 2);
    assert.ok(isCorrect(qs[0], s.picked[0]));
    assert.deepEqual(pickAnswer(s, qs, 0), s, "the first answer stands");
    s = nextQuestion(s, qs);
    s = pickAnswer(s, qs, 1);
    assert.ok(!isCorrect(qs[1], s.picked[1]));
    assert.deepEqual(pickAnswer(newQuiz(3), qs, 9), newQuiz(3), "out-of-range choice ignored");
    s = nextQuestion(s, qs);
    s = pickAnswer(s, qs, 1);
    assert.ok(!s.finished);
    s = nextQuestion(s, qs);
    assert.ok(s.finished);
    assert.deepEqual(quizScore(s, qs), { correct: 2, answered: 3, total: 3 });
    assert.deepEqual(missedQuestions(s, qs), [1]);
    assert.ok(newQuiz(0).finished);
});

test("stored data is checked before the UI uses it", () => {
    assert.deepEqual(cardsOf({ cards: [{ front: "f", back: "b" }, { front: 1, back: "x" }, null] }), [{ front: "f", back: "b" }]);
    assert.deepEqual(cardsOf(null), []);
    assert.deepEqual(cardsOf({ cards: "nope" }), []);
    const q = quizOf({
        questions: [
            { question: "ok", choices: ["a", "b"], answer: 1, explanation: "e", at_ms: 5 },
            { question: "bad index", choices: ["a", "b"], answer: 2 },
            { question: "one choice", choices: ["a"], answer: 0 },
            { question: "no explanation", choices: ["a", "b"], answer: 0 },
        ],
    });
    assert.deepEqual(q.map((x) => x.question), ["ok", "no explanation"]);
    assert.equal(q[1].explanation, "");
    assert.equal(q[1].at_ms, null);
});
