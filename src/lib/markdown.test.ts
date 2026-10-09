// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import { parseInline, parseMarkdown, slug } from "./markdown.ts";

test("headings get GitHub-style ids and inline marks parse", () => {
    assert.equal(slug("Delete or strike something"), "delete-or-strike-something");
    assert.equal(slug("What is it, how long, and notebook"), "what-is-it-how-long-and-notebook");
    assert.deepEqual(parseInline("Tap **Record** and `cmd`, see [Privacy](#privacy)."), [
        { t: "text", v: "Tap " },
        { t: "bold", v: "Record" },
        { t: "text", v: " and " },
        { t: "code", v: "cmd" },
        { t: "text", v: ", see " },
        { t: "link", v: "Privacy", href: "#privacy" },
        { t: "text", v: "." },
    ]);
});

test("blocks: paragraphs wrap, lists continue, tables and quotes parse", () => {
    const src = [
        "# Guide",
        "",
        "One line",
        "two lines.",
        "",
        "- first",
        "  wrapped",
        "- second",
        "",
        "1. a",
        "2. b",
        "",
        "> Tell people.",
        "",
        "| A | B |",
        "|---|---|",
        "| 1 | **2** |",
        "",
        "---",
        "",
        "```",
        "code",
        "```",
    ].join("\n");
    const b = parseMarkdown(src);
    assert.deepEqual(b.map((x) => x.t), ["h", "p", "ul", "ol", "quote", "table", "hr", "pre"]);
    assert.equal((b[0] as { id: string }).id, "guide");
    assert.deepEqual((b[1] as { inl: { v: string }[] }).inl.map((i) => i.v).join(""), "One line two lines.");
    const ul = b[2] as { items: { v: string }[][] };
    assert.equal(ul.items[0].map((i) => i.v).join(""), "first wrapped");
    assert.equal(ul.items.length, 2);
    const table = b[5] as { head: unknown[]; rows: { t: string }[][][] };
    assert.equal(table.head.length, 2);
    assert.equal(table.rows[0][1][0].t, "bold");
});
