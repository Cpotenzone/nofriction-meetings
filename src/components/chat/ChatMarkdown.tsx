// An answer's Markdown rendered as React text nodes (never HTML), with
// [n] citations as chips that open the recording at that moment.

import { parseAnswer, type Block, type Inline } from "../../lib/chatLogic";
import { clockOf } from "../../lib/chatLogic";
import type { Citation } from "../../lib/chat";

interface Props {
    content: string;
    citations: Citation[];
    onCite: (c: Citation) => void;
}

function CiteChip({ n, citations, onCite }: { n: number; citations: Citation[]; onCite: (c: Citation) => void }) {
    const c = citations.find((x) => x.n === n);
    if (!c) return <span className="rc-cite rc-cite--dead" title="No source with this number">[{n}]</span>;
    const when = c.timestamp_ms == null ? (c.source === "notes" ? "notes" : "") : clockOf(c.timestamp_ms);
    return (
        <button
            type="button"
            className="rc-cite"
            title={`${c.title}${when ? ` · ${when}` : ""}\n${c.excerpt}`}
            onClick={() => onCite(c)}
        >
            {n}
        </button>
    );
}

function Inlines({ inl, citations, onCite }: { inl: Inline[]; citations: Citation[]; onCite: (c: Citation) => void }) {
    return (
        <>
            {inl.map((i, k) => {
                switch (i.t) {
                    case "text":
                        return <span key={k}>{i.v}</span>;
                    case "bold":
                        return <strong key={k}>{i.v}</strong>;
                    case "code":
                        return <code key={k}>{i.v}</code>;
                    case "cite":
                        return <CiteChip key={k} n={i.n} citations={citations} onCite={onCite} />;
                }
            })}
        </>
    );
}

function BlockView({ b, citations, onCite }: { b: Block; citations: Citation[]; onCite: (c: Citation) => void }) {
    switch (b.t) {
        case "p":
            return <p><Inlines inl={b.inl} citations={citations} onCite={onCite} /></p>;
        case "h":
            return b.level <= 3
                ? <h3><Inlines inl={b.inl} citations={citations} onCite={onCite} /></h3>
                : <h4><Inlines inl={b.inl} citations={citations} onCite={onCite} /></h4>;
        case "ul":
            return <ul>{b.items.map((it, k) => <li key={k}><Inlines inl={it} citations={citations} onCite={onCite} /></li>)}</ul>;
        case "ol":
            return <ol>{b.items.map((it, k) => <li key={k}><Inlines inl={it} citations={citations} onCite={onCite} /></li>)}</ol>;
        case "pre":
            return <pre>{b.v}</pre>;
    }
}

export function ChatMarkdown({ content, citations, onCite }: Props) {
    const blocks = parseAnswer(content);
    return (
        <div className="rc-md">
            {blocks.map((b, k) => <BlockView key={k} b={b} citations={citations} onCite={onCite} />)}
        </div>
    );
}
