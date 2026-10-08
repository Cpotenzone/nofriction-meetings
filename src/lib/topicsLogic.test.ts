// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    cleanLabels,
    dayLabel,
    filterByTopic,
    groupRecordings,
    parseGroupBy,
    topicsForRow,
    type TopicIndex,
} from "./topicsLogic.ts";

const idx: TopicIndex = {
    topics: [
        { key: "cell membrane", label: "Cell membranes", count: 2 },
        { key: "q4 roadmap", label: "Q4 roadmap", count: 1 },
    ],
    by_meeting: {
        m1: [{ key: "cell membrane", label: "Cell membranes" }, { key: "osmosis", label: "Osmosis" }, { key: "atp", label: "ATP" }],
        m2: [{ key: "q4 roadmap", label: "Q4 roadmap" }],
        m3: [{ key: "cell membrane", label: "Cell membranes" }],
    },
};

const now = new Date(2026, 9, 8, 15, 0, 0);
const at = (day: number, hour = 10) => new Date(2026, 9, day, hour).toISOString();
const meetings = [
    { id: "m1", started_at: at(8, 11), class_name: "BIO 101" },
    { id: "m2", started_at: at(8, 9), class_name: "Acme" },
    { id: "m3", started_at: at(7), class_name: "bio 101" },
    { id: "m4", started_at: at(1), class_name: null },
];

test("topic filter and row chips", () => {
    assert.deepEqual(filterByTopic(meetings, idx, "cell membrane").map((m) => m.id), ["m1", "m3"]);
    assert.equal(filterByTopic(meetings, idx, null).length, 4);
    assert.deepEqual(filterByTopic(meetings, idx, "nothing"), []);
    const row = topicsForRow(idx, "m1");
    assert.deepEqual(row.shown.map((t) => t.label), ["Cell membranes", "Osmosis"]);
    assert.equal(row.more, 1);
    assert.deepEqual(topicsForRow(idx, "m4"), { shown: [], more: 0 });
});

test("group by date keeps list order with day headers", () => {
    const g = groupRecordings(meetings, idx, "date", now);
    assert.deepEqual(g.map((x) => x.label), ["Today", "Yesterday", "Thu, Oct 1, 2026"]);
    assert.deepEqual(g[0].meetings.map((m) => m.id), ["m1", "m2"]);
    assert.equal(dayLabel("not a date", now), "Unknown date");
});

test("group by notebook folds case and puts the rest last", () => {
    const g = groupRecordings(meetings, idx, "notebook", now);
    assert.deepEqual(g.map((x) => x.label), ["Acme", "BIO 101", "No notebook"]);
    assert.deepEqual(g[1].meetings.map((m) => m.id), ["m1", "m3"], "first spelling shown, both grouped");
    assert.deepEqual(g[2].meetings.map((m) => m.id), ["m4"]);
});

test("group by topic lists a recording under each topic, biggest first", () => {
    const g = groupRecordings(meetings, idx, "topic", now);
    assert.deepEqual(g.map((x) => x.label), ["Cell membranes", "ATP", "Osmosis", "Q4 roadmap", "No topics"]);
    assert.deepEqual(g[0].meetings.map((m) => m.id), ["m1", "m3"]);
    assert.deepEqual(g[4].meetings.map((m) => m.id), ["m4"]);
    assert.equal(groupRecordings([], idx, "topic", now).length, 0);
});

test("group-by setting parses and labels clean up", () => {
    assert.equal(parseGroupBy("topic"), "topic");
    assert.equal(parseGroupBy("nope"), "date");
    assert.equal(parseGroupBy(null), "date");
    assert.deepEqual(cleanLabels(["  Q4   roadmap ", "", "q4 roadmap", "Hiring"]), ["Q4 roadmap", "Hiring"]);
});
