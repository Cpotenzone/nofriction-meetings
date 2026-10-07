// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    countsLabel,
    displayLink,
    formatOffset,
    isOpenableUrl,
    linksMarkdown,
    referenceUrl,
    shortPath,
    type MeetingLink,
} from "./meetingLinks.ts";

function link(over: Partial<MeetingLink>): MeetingLink {
    return {
        key: "example.com",
        url: "https://example.com",
        host: "example.com",
        path: "",
        title: null,
        note: null,
        sources: ["said"],
        said_count: 1,
        screen_count: 0,
        first_ms: 0,
        first_at: null,
        first_source: "said",
        reference_id: null,
        created_at: null,
        ...over,
    };
}

test("only http and https links can be opened", () => {
    for (const ok of [
        "https://example.com",
        "http://example.org/a?b=1",
        "HTTPS://EXAMPLE.COM/X",
        "https://localhost:3000/",
        "https://192.168.1.20:8443/slides",
    ]) {
        assert.equal(isOpenableUrl(ok), true, ok);
    }
    for (const bad of [
        "javascript:alert(1)",
        "JavaScript:alert(1)",
        "javascript://example.com/%0aalert(1)",
        "file:///etc/passwd",
        "data:text/html,<script>alert(1)</script>",
        "vbscript:msgbox(1)",
        "mailto:jane@example.com",
        "ftp://example.com/file",
        "about:blank",
        "example.com",
        " https://example.com",
        "https://example.com\n",
        "https://exa mple.com",
        "https://user:pw@example.com/",
        "https://",
        "https://example",
        "https://example.com:99999",
        "https://-bad-.com",
        "https://exämple.com",
        "",
    ]) {
        assert.equal(isOpenableUrl(bad), false, JSON.stringify(bad));
    }
});

test("typed reference addresses get https:// and other schemes are refused", () => {
    assert.equal(referenceUrl("example.com/syllabus"), "https://example.com/syllabus");
    assert.equal(referenceUrl("  http://example.edu/a "), "http://example.edu/a");
    assert.equal(referenceUrl("localhost:3000"), "https://localhost:3000");
    for (const bad of ["javascript:alert(1)", "file:///a.pdf", "mailto:a@b.co", "not a link", "", "ftp://x.org"]) {
        assert.equal(referenceUrl(bad), null, bad);
    }
});

test("display: domain plus a shortened path", () => {
    assert.equal(shortPath(""), "");
    assert.equal(shortPath("/math"), "/math");
    assert.equal(
        shortPath("/courses/biology-101/modules/week-3/lecture-notes-and-slides"),
        "/courses/…/lecture-notes-and-slides",
    );
    assert.equal(shortPath("/a/b/c?x=1&y=2&z=3&w=4&v=5&u=6&t=7", 20), "/a/…/c?…");
    const long = shortPath("/" + "x".repeat(80));
    assert.equal(long.length, 36);
    assert.ok(long.endsWith("…"));
    assert.equal(displayLink({ host: "khanacademy.org", path: "/math" }), "khanacademy.org/math");
});

test("times match the Recordings view (m:ss from the start)", () => {
    assert.equal(formatOffset(0), "0:00");
    assert.equal(formatOffset(65_400), "1:05");
    assert.equal(formatOffset(3_725_000), "62:05");
    assert.equal(formatOffset(-5), "0:00");
});

test("counts", () => {
    assert.equal(countsLabel({ said_count: 2, screen_count: 0 }), "said 2×");
    assert.equal(countsLabel({ said_count: 1, screen_count: 5 }), "said 1× · on screen 5×");
    assert.equal(countsLabel({ said_count: 0, screen_count: 0 }), "");
});

test("copy all as Markdown", () => {
    const md = linksMarkdown("Biology [101]", [
        link({
            key: "khanacademy.org/science",
            url: "https://www.khanacademy.org/science",
            host: "khanacademy.org",
            path: "/science",
            title: "Khan [unit] 1",
            note: "Ch. 1\nand 2",
            sources: ["added", "screen"],
            said_count: 0,
            screen_count: 3,
            first_ms: 30_000,
        }),
        link({
            key: "en.wikipedia.org/wiki/Cell_(biology)",
            url: "https://en.wikipedia.org/wiki/Cell_(biology)",
            host: "en.wikipedia.org",
            path: "/wiki/Cell_(biology)",
            said_count: 2,
            first_ms: 65_000,
        }),
        link({ url: "javascript:alert(1)", host: "", path: "", title: "evil", first_ms: null, said_count: 0 }),
    ]);
    assert.equal(
        md,
        [
            "## Links — Biology \\[101\\]",
            "",
            "- [Khan \\[unit\\] 1](https://www.khanacademy.org/science) — added · on screen 3× · first at 0:30 — Ch. 1 and 2",
            "- [en.wikipedia.org/wiki/Cell\\_(biology)](https://en.wikipedia.org/wiki/Cell_%28biology%29) — said 2× · first at 1:05",
            "- evil",
            "",
        ].join("\n"),
    );
    assert.equal(linksMarkdown("", []), "## Links — Meeting\n\n_No links._\n");
});
