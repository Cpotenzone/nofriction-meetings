// Chat with your recordings (docs/TOPICS_AND_CHAT.md): the scope, the
// suggested questions and how an answer's Markdown and [n] citations are
// split for rendering. Pure (no Tauri, no React), tested with `npm test`.

export type ScopeKind = "all" | "notebook" | "topic" | "meeting";

/** What a question may draw on; `value` is the Notebook, the topic key or the recording id. */
export interface Scope {
    kind: ScopeKind;
    value?: string | null;
}

export const ALL_SCOPE: Scope = { kind: "all" };

/** `chat_scope_summary`: what a scope covers (for the picker and the suggestions). */
export interface ScopeSummary {
    label: string;
    count: number;
    recent_titles: string[];
    topics: string[];
    notebooks: string[];
    /** "meeting" | "class" | "personal" present in the scope */
    kinds: string[];
}

/** The scope a new chat opens with: the recording open in Recordings, else All. */
export function defaultScope(selectedMeetingId: string | null | undefined): Scope {
    return selectedMeetingId ? { kind: "meeting", value: selectedMeetingId } : ALL_SCOPE;
}

export function sameScope(a: Scope, b: Scope): boolean {
    if (a.kind !== b.kind) return false;
    if (a.kind === "all") return true;
    return (a.value ?? "").trim().toLowerCase() === (b.value ?? "").trim().toLowerCase();
}

/** The scope's label as the backend names it ("Notebook · BIO 101"); `name` resolves a topic key or recording id. */
export function scopeLabel(scope: Scope, name?: string | null): string {
    switch (scope.kind) {
        case "all":
            return "All recordings";
        case "notebook":
            return `Notebook · ${name ?? scope.value ?? ""}`;
        case "topic":
            return `Topic · ${name ?? scope.value ?? ""}`;
        case "meeting":
            return `Recording · ${name ?? "this recording"}`;
    }
}

function clipTitle(t: string, max = 40): string {
    const one = t.replace(/\s+/g, " ").trim();
    return one.length <= max ? one : `${one.slice(0, max - 1).trimEnd()}…`;
}

/**
 * Up to four questions for an empty chat, from the scope's titles, types
 * and topics. No AI call; every question reads as something the scope
 * can answer.
 */
export function suggestedQuestions(scope: Scope, summary: ScopeSummary | null): string[] {
    const out: string[] = [];
    const add = (q: string) => {
        if (out.length < 4 && !out.includes(q)) out.push(q);
    };
    if (!summary || summary.count === 0) {
        return scope.kind === "all" ? ["Summarize my week", "What did we decide recently?", "What's still open?"] : [];
    }
    const hasClass = summary.kinds.includes("class");
    const hasMeeting = summary.kinds.includes("meeting");
    const topics = summary.topics.filter(Boolean);
    const title = summary.recent_titles[0];
    if (scope.kind === "meeting") {
        add("Summarize this recording");
        if (topics[0]) add(`What was said about ${topics[0]}?`);
        if (hasClass) add("What's likely to be on the test?");
        else add("What was decided, and what's still open?");
        add("What should I follow up on?");
        return out;
    }
    if (scope.kind === "notebook") {
        const nb = summary.notebooks[0] ?? scope.value ?? "this notebook";
        if (hasClass) add(`What's on the test for ${nb}?`);
        else add(`What did we decide in ${nb}?`);
        if (topics[0]) add(`What was said about ${topics[0]}?`);
        add(`Summarize the latest ${nb} recording`);
        add("What's still open?");
        return out;
    }
    if (scope.kind === "topic") {
        const t = topics[0] ?? scope.value ?? "this topic";
        add(`What did we decide about ${t}?`);
        add(`Summarize everything about ${t}`);
        add(`What questions are still open about ${t}?`);
        if (title) add(`What was said about ${t} in ${clipTitle(title)}?`);
        return out;
    }
    add("Summarize my week");
    if (topics[0]) add(`What did we decide about ${topics[0]}?`);
    if (hasClass) {
        const nb = summary.notebooks[0];
        add(nb ? `What's on the test for ${nb}?` : "What's likely to be on the test?");
    }
    if (hasMeeting) add("What action items are still open?");
    if (title) add(`What happened in ${clipTitle(title)}?`);
    add("What should I follow up on?");
    return out;
}

// ── Answer rendering ────────────────────────────────────────────────────

export type Inline =
    | { t: "text"; v: string }
    | { t: "bold"; v: string }
    | { t: "code"; v: string }
    | { t: "cite"; n: number };

export type Block =
    | { t: "p"; inl: Inline[] }
    | { t: "h"; level: number; inl: Inline[] }
    | { t: "ul"; items: Inline[][] }
    | { t: "ol"; items: Inline[][] }
    | { t: "pre"; v: string };

/** Inline Markdown subset: **bold**, `code` and [n] citations. Anything else is text. */
export function parseInline(text: string): Inline[] {
    const out: Inline[] = [];
    const re = /(\*\*([^*]+)\*\*)|(`([^`]+)`)|(\[(\d{1,3}(?:\s*,\s*\d{1,3})*)\])/g;
    let last = 0;
    let m: RegExpExecArray | null;
    const pushText = (v: string) => {
        if (!v) return;
        const prev = out[out.length - 1];
        if (prev && prev.t === "text") prev.v += v;
        else out.push({ t: "text", v });
    };
    while ((m = re.exec(text)) !== null) {
        pushText(text.slice(last, m.index));
        if (m[2] !== undefined) out.push({ t: "bold", v: m[2] });
        else if (m[4] !== undefined) out.push({ t: "code", v: m[4] });
        else if (m[6] !== undefined) {
            for (const n of m[6].split(",")) out.push({ t: "cite", n: Number(n.trim()) });
        }
        last = m.index + m[0].length;
    }
    pushText(text.slice(last));
    return out;
}

/**
 * The block structure of an answer: paragraphs, headings, bullet and
 * numbered lists, fenced code. Output is data for React to render as text;
 * no HTML is ever produced.
 */
export function parseAnswer(markdown: string): Block[] {
    const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
    const blocks: Block[] = [];
    let para: string[] = [];
    let list: { t: "ul" | "ol"; items: Inline[][] } | null = null;
    let pre: string[] | null = null;
    const flushPara = () => {
        if (para.length) blocks.push({ t: "p", inl: parseInline(para.join(" ")) });
        para = [];
    };
    const flushList = () => {
        if (list) blocks.push(list);
        list = null;
    };
    for (const raw of lines) {
        if (pre) {
            if (raw.trim().startsWith("```")) {
                blocks.push({ t: "pre", v: pre.join("\n") });
                pre = null;
            } else pre.push(raw);
            continue;
        }
        const line = raw.trimEnd();
        if (line.trim().startsWith("```")) {
            flushPara();
            flushList();
            pre = [];
            continue;
        }
        if (!line.trim()) {
            flushPara();
            flushList();
            continue;
        }
        const h = /^(#{1,6})\s+(.*)$/.exec(line.trim());
        if (h) {
            flushPara();
            flushList();
            blocks.push({ t: "h", level: Math.max(3, h[1].length), inl: parseInline(h[2]) });
            continue;
        }
        const ul = /^\s*[-*•]\s+(.*)$/.exec(line);
        const ol = /^\s*\d+[.)]\s+(.*)$/.exec(line);
        if (ul || ol) {
            flushPara();
            const t = ul ? "ul" : "ol";
            const item = parseInline((ul ?? ol)![1]);
            if (list && list.t === t) list.items.push(item);
            else {
                flushList();
                list = { t, items: [item] };
            }
            continue;
        }
        if (list) {
            // A wrapped list item continues the last item
            const lastItem = list.items[list.items.length - 1];
            lastItem.push({ t: "text", v: " " });
            lastItem.push(...parseInline(line.trim()));
            continue;
        }
        para.push(line.trim());
    }
    flushPara();
    flushList();
    if (pre) blocks.push({ t: "pre", v: (pre as string[]).join("\n") });
    return blocks;
}

/** The citation numbers an answer uses, in order of first use. */
export function citedNumbers(markdown: string): number[] {
    const out: number[] = [];
    for (const b of parseAnswer(markdown)) {
        const inls = b.t === "ul" || b.t === "ol" ? b.items.flat() : b.t === "pre" ? [] : b.inl;
        for (const i of inls) if (i.t === "cite" && i.n > 0 && !out.includes(i.n)) out.push(i.n);
    }
    return out;
}

/** "12:34" from ms. */
export function clockOf(ms: number | null | undefined): string {
    if (ms == null || ms < 0) return "";
    const s = Math.floor(ms / 1000);
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = s % 60;
    return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}` : `${m}:${String(sec).padStart(2, "0")}`;
}
