// Help: one document, the user guide (docs/USER_GUIDE.md), rendered as
// text. Opened from Help → noFriction Help and Settings → About.

import { useEffect, useMemo, useRef } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import guide from "../../docs/USER_GUIDE.md?raw";
import { parseMarkdown, type Block, type Inline } from "../lib/markdown";
import "./SettingsWindow.css";

interface HelpWindowProps {
    isOpen: boolean;
    onClose: () => void;
}

function Inlines({ inl, onAnchor }: { inl: Inline[]; onAnchor: (id: string) => void }) {
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
                    case "link":
                        return i.href.startsWith("#") ? (
                            <a key={k} href={i.href} onClick={(e) => { e.preventDefault(); onAnchor(i.href.slice(1)); }}>
                                {i.v}
                            </a>
                        ) : (
                            <a key={k} href={i.href} onClick={(e) => { e.preventDefault(); openUrl(i.href).catch(() => undefined); }}>
                                {i.v}
                            </a>
                        );
                }
            })}
        </>
    );
}

function BlockView({ b, onAnchor }: { b: Block; onAnchor: (id: string) => void }) {
    const inl = (x: Inline[]) => <Inlines inl={x} onAnchor={onAnchor} />;
    switch (b.t) {
        case "h": {
            const Tag = (b.level <= 1 ? "h1" : b.level === 2 ? "h2" : "h3") as "h1" | "h2" | "h3";
            return <Tag id={`help-${b.id}`}>{inl(b.inl)}</Tag>;
        }
        case "p":
            return <p>{inl(b.inl)}</p>;
        case "ul":
            return <ul>{b.items.map((it, k) => <li key={k}>{inl(it)}</li>)}</ul>;
        case "ol":
            return <ol>{b.items.map((it, k) => <li key={k}>{inl(it)}</li>)}</ol>;
        case "quote":
            return <blockquote>{inl(b.inl)}</blockquote>;
        case "pre":
            return <pre>{b.v}</pre>;
        case "hr":
            return <hr />;
        case "table":
            return (
                <table>
                    <thead>
                        <tr>{b.head.map((c, k) => <th key={k}>{inl(c)}</th>)}</tr>
                    </thead>
                    <tbody>
                        {b.rows.map((r, k) => (
                            <tr key={k}>{r.map((c, j) => <td key={j}>{inl(c)}</td>)}</tr>
                        ))}
                    </tbody>
                </table>
            );
    }
}

export function HelpWindow({ isOpen, onClose }: HelpWindowProps) {
    const blocks = useMemo(() => parseMarkdown(guide), []);
    const bodyRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (!isOpen) return;
        const onKey = (e: KeyboardEvent) => {
            if (e.key === "Escape") onClose();
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [isOpen, onClose]);

    if (!isOpen) return null;
    const jump = (id: string) => {
        const el = bodyRef.current?.querySelector(`#help-${CSS.escape(id)}`);
        el?.scrollIntoView({ behavior: "smooth", block: "start" });
    };
    return (
        <div className="win__scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
            <div className="win win--help" role="dialog" aria-modal="true" aria-label="Help">
                <button type="button" className="win__close" onClick={onClose} aria-label="Close" title="Close (Esc)">
                    ✕
                </button>
                <div className="help" ref={bodyRef}>
                    {blocks.map((b, k) => <BlockView key={k} b={b} onAnchor={jump} />)}
                </div>
            </div>
        </div>
    );
}
