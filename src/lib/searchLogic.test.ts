// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import { groupHits, type SearchHit } from "./searchLogic.ts";

const hits: SearchHit[] = [
    { meeting_id: "m2", kind: "said", label: "we ship on the fourth", ms: 90_000 },
    { meeting_id: "m1", kind: "title", label: "Acme sync", ms: null },
    { meeting_id: "m1", kind: "person", label: "Dana Whitfield", ms: null },
    { meeting_id: "m1", kind: "topic", label: "Q4 launch", ms: null },
    { meeting_id: "m1", kind: "person", label: "dana whitfield", ms: null },
    { meeting_id: "m2", kind: "said", label: "launch slips a week", ms: 10_000 },
    { meeting_id: "m2", kind: "said", label: "third", ms: 200_000 },
    { meeting_id: "m2", kind: "said", label: "fourth", ms: 300_000 },
    { meeting_id: "m9", kind: "notebook", label: "BIO 101", ms: null },
];

test("hits group per recording in list order, lines earliest first and capped", () => {
    const g = groupHits(hits, ["m1", "m2"], 3);
    assert.deepEqual(g.map((x) => x.meetingId), ["m1", "m2", "m9"], "unknown recordings come last");
    assert.deepEqual(g[0].why, ["Dana Whitfield", "Q4 launch"], "a title match needs no reason; repeats fold");
    assert.deepEqual(g[0].lines, []);
    assert.deepEqual(
        g[1].lines.map((l) => l.ms),
        [10_000, 90_000, 200_000],
    );
    assert.equal(g[1].moreLines, 1);
    assert.deepEqual(g[2].why, ["BIO 101"]);
});

test("no hits, no groups", () => {
    assert.deepEqual(groupHits([], ["m1"]), []);
});
