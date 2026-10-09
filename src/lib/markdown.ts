// A small Markdown reader for the user guide (docs/USER_GUIDE.md): headings,
// paragraphs, lists, tables, quotes, code and inline bold / code / links.
// Pure (no React), tested with `npm test`. Output is data, rendered as
// text nodes by HelpWindow (never HTML).

export type Inline =
    | { t: "text"; v: string }
    | { t: "bold"; v: string }
    | { t: "code"; v: string }
    | { t: "link"; v: string; href: string };

export type Block =
    | { t: "h"; level: number; id: string; inl: Inline[] }
    | { t: "p"; inl: Inline[] }
    | { t: "ul"; items: Inline[][] }
    | { t: "ol"; items: Inline[][] }
    | { t: "quote"; inl: Inline[] }
    | { t: "pre"; v: string }
    | { t: "table"; head: Inline[][]; rows: Inline[][][] }
    | { t: "hr" };

/** GitHub-style heading id: lowercase, spaces to dashes, punctuation dropped. */
export function slug(text: string): string {
    return text
        .toLowerCase()
        .replace(/[^\p{L}\p{N}\s-]/gu, "")
        .trim()
        .replace(/\s+/g, "-");
}

const plain = (inl: Inline[]) => inl.map((i) => i.v).join("");

export function parseInline(text: string): Inline[] {
    const out: Inline[] = [];
    const re = /(\*\*([^*]+)\*\*)|(`([^`]+)`)|(\[([^\]]+)\]\(([^)\s]+)\))/g;
    let last = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text))) {
        if (m.index > last) out.push({ t: "text", v: text.slice(last, m.index) });
        if (m[1]) out.push({ t: "bold", v: m[2] });
        else if (m[3]) out.push({ t: "code", v: m[4] });
        else out.push({ t: "link", v: m[6], href: m[7] });
        last = m.index + m[0].length;
    }
    if (last < text.length) out.push({ t: "text", v: text.slice(last) });
    return out;
}

const cells = (line: string) =>
    line
        .trim()
        .replace(/^\|/, "")
        .replace(/\|$/, "")
        .split("|")
        .map((c) => parseInline(c.trim()));

export function parseMarkdown(src: string): Block[] {
    const lines = src.replace(/\r\n/g, "\n").split("\n");
    const blocks: Block[] = [];
    let i = 0;
    const para: string[] = [];
    const flush = () => {
        if (para.length) {
            blocks.push({ t: "p", inl: parseInline(para.join(" ")) });
            para.length = 0;
        }
    };
    while (i < lines.length) {
        const line = lines[i];
        const trimmed = line.trim();
        if (!trimmed) {
            flush();
            i++;
            continue;
        }
        if (trimmed.startsWith("```")) {
            flush();
            const buf: string[] = [];
            i++;
            while (i < lines.length && !lines[i].trim().startsWith("```")) buf.push(lines[i++]);
            i++;
            blocks.push({ t: "pre", v: buf.join("\n") });
            continue;
        }
        const h = /^(#{1,6})\s+(.*)$/.exec(trimmed);
        if (h) {
            flush();
            const inl = parseInline(h[2].trim());
            blocks.push({ t: "h", level: h[1].length, id: slug(plain(inl)), inl });
            i++;
            continue;
        }
        if (/^(-{3,}|\*{3,})$/.test(trimmed)) {
            flush();
            blocks.push({ t: "hr" });
            i++;
            continue;
        }
        if (trimmed.startsWith(">")) {
            flush();
            const buf: string[] = [];
            while (i < lines.length && lines[i].trim().startsWith(">")) buf.push(lines[i++].trim().replace(/^>\s?/, ""));
            blocks.push({ t: "quote", inl: parseInline(buf.join(" ")) });
            continue;
        }
        if (trimmed.startsWith("|")) {
            flush();
            const head = cells(trimmed);
            i++;
            if (i < lines.length && /^\|?\s*:?-+/.test(lines[i].trim())) i++;
            const rows: Inline[][][] = [];
            while (i < lines.length && lines[i].trim().startsWith("|")) rows.push(cells(lines[i++]));
            blocks.push({ t: "table", head, rows });
            continue;
        }
        const li = /^([-*]|\d+\.)\s+(.*)$/.exec(trimmed);
        if (li) {
            flush();
            const ordered = /^\d+\./.test(li[1]);
            const items: string[] = [];
            while (i < lines.length) {
                const t = lines[i].trim();
                const m = /^([-*]|\d+\.)\s+(.*)$/.exec(t);
                if (m && /^\d+\./.test(m[1]) === ordered) {
                    items.push(m[2]);
                    i++;
                } else if (t && lines[i].startsWith("  ") && items.length) {
                    // A wrapped line of the previous item
                    items[items.length - 1] += " " + t;
                    i++;
                } else break;
            }
            blocks.push({ t: ordered ? "ol" : "ul", items: items.map(parseInline) });
            continue;
        }
        para.push(trimmed);
        i++;
    }
    flush();
    return blocks;
}
