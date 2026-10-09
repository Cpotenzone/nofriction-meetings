// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import { cleanLabels, dayLabel, groupByDay } from "./topicsLogic.ts";

const now = new Date(2026, 9, 8, 15, 0, 0);
const at = (day: number, hour = 10) => new Date(2026, 9, day, hour).toISOString();
const meetings = [
    { id: "m1", started_at: at(8, 11) },
    { id: "m2", started_at: at(8, 9) },
    { id: "m3", started_at: at(7) },
    { id: "m4", started_at: at(1) },
];

test("group by day keeps list order with day headers", () => {
    const g = groupByDay(meetings, now);
    assert.deepEqual(g.map((x) => x.label), ["Today", "Yesterday", "Thu, Oct 1, 2026"]);
    assert.deepEqual(g[0].meetings.map((m) => m.id), ["m1", "m2"]);
    assert.equal(dayLabel("not a date", now), "Unknown date");
    assert.deepEqual(groupByDay([], now), []);
});

test("labels clean up", () => {
    assert.deepEqual(cleanLabels(["  Q4   roadmap ", "", "q4 roadmap", "Hiring"]), ["Q4 roadmap", "Hiring"]);
});
