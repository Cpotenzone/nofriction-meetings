// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    citedNumbers,
    clockOf,
    defaultScope,
    parseAnswer,
    parseInline,
    sameScope,
    scopeLabel,
    suggestedQuestions,
    type ScopeSummary,
} from "./chatLogic.ts";

const summary = (over: Partial<ScopeSummary> = {}): ScopeSummary => ({
    label: "All recordings",
    count: 3,
    recent_titles: ["Acme weekly sync", "Biology 101"],
    topics: ["Q4 roadmap", "Cell membranes"],
    notebooks: ["Acme", "BIO 101"],
    kinds: ["meeting", "class"],
    ...over,
});

test("the default scope is the open recording, else all", () => {
    assert.deepEqual(defaultScope("m1"), { kind: "meeting", value: "m1" });
    assert.deepEqual(defaultScope(null), { kind: "all" });
    assert.ok(sameScope({ kind: "notebook", value: "BIO 101" }, { kind: "notebook", value: "bio 101 " }));
    assert.ok(!sameScope({ kind: "all" }, { kind: "meeting", value: "m1" }));
    assert.ok(sameScope({ kind: "all", value: "x" }, { kind: "all" }));
});

test("scope labels match the backend's", () => {
    assert.equal(scopeLabel({ kind: "all" }), "All recordings");
    assert.equal(scopeLabel({ kind: "notebook", value: "BIO 101" }), "Notebook · BIO 101");
    assert.equal(scopeLabel({ kind: "topic", value: "q4 roadmap" }, "Q4 roadmap"), "Topic · Q4 roadmap");
    assert.equal(scopeLabel({ kind: "meeting", value: "m1" }, "Acme sync"), "Recording · Acme sync");
    assert.equal(scopeLabel({ kind: "meeting", value: "m1" }), "Recording · this recording");
});

test("suggested questions follow the scope and its contents, at most four", () => {
    const all = suggestedQuestions({ kind: "all" }, summary());
    assert.equal(all.length, 4);
    assert.equal(all[0], "Summarize my week");
    assert.ok(all.includes("What did we decide about Q4 roadmap?"));
    assert.ok(all.includes("What's on the test for Acme?"), "a class in the scope asks about the test");
    const nb = suggestedQuestions({ kind: "notebook", value: "BIO 101" }, summary({ notebooks: ["BIO 101"], kinds: ["class"], topics: ["Mitosis"] }));
    assert.equal(nb[0], "What's on the test for BIO 101?");
    assert.ok(nb.includes("What was said about Mitosis?"));
    const meet = suggestedQuestions({ kind: "notebook", value: "Acme" }, summary({ notebooks: ["Acme"], kinds: ["meeting"], topics: [] }));
    assert.equal(meet[0], "What did we decide in Acme?");
    const topic = suggestedQuestions({ kind: "topic", value: "q4 roadmap" }, summary({ topics: ["Q4 roadmap"] }));
    assert.equal(topic[0], "What did we decide about Q4 roadmap?");
    assert.ok(topic[3].includes("Acme weekly sync"));
    const one = suggestedQuestions({ kind: "meeting", value: "m1" }, summary({ count: 1, kinds: ["class"], topics: [] }));
    assert.deepEqual(one, ["Summarize this recording", "What's likely to be on the test?", "What should I follow up on?"]);
    assert.deepEqual(suggestedQuestions({ kind: "all" }, null), ["Summarize my week", "What did we decide recently?", "What's still open?"]);
    assert.deepEqual(suggestedQuestions({ kind: "notebook", value: "x" }, summary({ count: 0 })), []);
    const long = suggestedQuestions({ kind: "all" }, summary({ topics: [], kinds: ["personal"], recent_titles: ["A very long recording title that keeps going and going past the limit"] }));
    assert.ok(long.some((q) => q.endsWith("…?")), long.join(" | "));
});

test("inline markdown becomes text, bold, code and citations only", () => {
    assert.deepEqual(parseInline("Ship **Friday** [1], see `x` [2, 3] <b>no html</b>"), [
        { t: "text", v: "Ship " },
        { t: "bold", v: "Friday" },
        { t: "text", v: " " },
        { t: "cite", n: 1 },
        { t: "text", v: ", see " },
        { t: "code", v: "x" },
        { t: "text", v: " " },
        { t: "cite", n: 2 },
        { t: "cite", n: 3 },
        { t: "text", v: " <b>no html</b>" },
    ]);
    assert.deepEqual(parseInline("[x] [] [12]"), [{ t: "text", v: "[x] [] " }, { t: "cite", n: 12 }]);
});

test("answers split into paragraphs, headings, lists and code", () => {
    const md = "# Decision\nThey decided to **ship** [1].\n\n- Budget: 10k [2]\n- Owner: Bo\n  continued\n\n1. first\n2) second\n\n```\ncode [9]\n```\ntail";
    const b = parseAnswer(md);
    assert.deepEqual(b.map((x) => x.t), ["h", "p", "ul", "ol", "pre", "p"]);
    assert.equal((b[0] as { level: number }).level, 3, "headings never larger than h3");
    assert.equal((b[2] as { items: unknown[] }).items.length, 2);
    assert.deepEqual((b[4] as { v: string }).v, "code [9]");
    assert.deepEqual(citedNumbers(md), [1, 2], "code is never a citation");
    assert.deepEqual(citedNumbers("plain"), []);
    assert.deepEqual(parseAnswer(""), []);
    assert.deepEqual(parseAnswer("```\nopen"), [{ t: "pre", v: "open" }]);
});

test("clock", () => {
    assert.equal(clockOf(754_000), "12:34");
    assert.equal(clockOf(3_725_000), "1:02:05");
    assert.equal(clockOf(null), "");
});
