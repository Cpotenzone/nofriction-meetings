// Chat: a conversation that answers from the user's recordings
// (docs/TOPICS_AND_CHAT.md). Scope at the top, past chats on the side,
// answers in Markdown with [n] citation chips that open the recording in
// Rewind at that moment. Retrieval is local; the one AI request goes
// through the backend's AI client (Pro and consent are checked there).

import { useCallback, useEffect, useRef, useState } from "react";
import { chatApi, type ChatMessage, type ChatThread, type Citation } from "../../lib/chat";
import { defaultScope, sameScope, scopeLabel, suggestedQuestions, type Scope, type ScopeSummary } from "../../lib/chatLogic";
import { aiErrorClass, friendlyAiError, isNoProviderError } from "../../lib/ai";
import { requestRecordingSeek } from "../../lib/navigation";
import { AiSetupNotice, useAiStatus } from "../AiSetupNotice";
import { useUndoDelete } from "../UndoToast";
import { BrainIcon, TrashIcon } from "../icons";
import { ChatMarkdown } from "./ChatMarkdown";
import { ScopePicker } from "./ScopePicker";
import "./RecordingsChat.css";

interface Props {
    /** The recording open in Recordings: the default scope of a new chat */
    selectedMeetingId: string | null;
    /** Open a recording (a citation chip adds the moment) */
    onOpenRecording: (meetingId: string) => void;
}

function aiFailure(e: unknown): string {
    switch (aiErrorClass(e)) {
        case "pro_required":
            return "Chat is part of noFriction Pro.";
        case "consent_required":
            return "Chat needs your permission to send recording excerpts to your AI endpoint. Ask again and choose Allow.";
        default:
            return friendlyAiError(e);
    }
}

export function RecordingsChat({ selectedMeetingId, onOpenRecording }: Props) {
    const [scope, setScope] = useState<Scope>(() => defaultScope(selectedMeetingId));
    const [summary, setSummary] = useState<ScopeSummary | null>(null);
    const [threads, setThreads] = useState<ChatThread[]>([]);
    const [thread, setThread] = useState<ChatThread | null>(null);
    const [messages, setMessages] = useState<ChatMessage[]>([]);
    const [input, setInput] = useState("");
    const [thinking, setThinking] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [needsAi, setNeedsAi] = useState(false);
    const [showThreads, setShowThreads] = useState(false);
    const endRef = useRef<HTMLDivElement>(null);
    const textareaRef = useRef<HTMLTextAreaElement>(null);
    const { configured } = useAiStatus();
    const showSetup = configured === false || needsAi;
    // Deleting a chat: gone at once, Undo for a few seconds (no confirm dialog)
    const undoDelete = useUndoDelete((e) => setError(String(e)));

    useEffect(() => {
        if (configured) setNeedsAi(false);
    }, [configured]);

    // A different recording was opened: a fresh chat follows it
    useEffect(() => {
        if (!thread && messages.length === 0) setScope((s) => {
            const d = defaultScope(selectedMeetingId);
            return sameScope(s, d) ? s : d;
        });
    }, [selectedMeetingId]); // eslint-disable-line react-hooks/exhaustive-deps

    const loadThreads = useCallback(() => {
        chatApi.threads().then(setThreads).catch(() => {});
    }, []);
    useEffect(loadThreads, [loadThreads]);

    useEffect(() => {
        let live = true;
        setSummary(null);
        chatApi.summary(scope).then((s) => live && setSummary(s)).catch(() => live && setSummary(null));
        return () => { live = false; };
    }, [scope]);

    useEffect(() => {
        endRef.current?.scrollIntoView({ behavior: "smooth" });
    }, [messages, thinking]);

    useEffect(() => {
        const t = textareaRef.current;
        if (t) {
            t.style.height = "44px";
            t.style.height = Math.min(t.scrollHeight, 140) + "px";
        }
    }, [input]);

    const send = async (text: string) => {
        const q = text.trim();
        if (!q || thinking) return;
        setError(null);
        setInput("");
        setThinking(true);
        try {
            const turn = await chatApi.ask(thread?.id ?? null, scope, q);
            setThread(turn.thread);
            setMessages((prev) => [...prev, turn.user, turn.assistant]);
            loadThreads();
        } catch (e) {
            if (isNoProviderError(e)) setNeedsAi(true);
            else setError(aiFailure(e));
            setInput(q);
        } finally {
            setThinking(false);
        }
    };

    const openThread = async (t: ChatThread) => {
        try {
            const d = await chatApi.thread(t.id);
            setThread(d.thread);
            setMessages(d.messages);
            setScope(d.thread.scope);
            setShowThreads(false);
            setError(null);
        } catch (e) {
            setError(String(e));
        }
    };

    const newChat = () => {
        setThread(null);
        setMessages([]);
        setError(null);
        setScope(defaultScope(selectedMeetingId));
        textareaRef.current?.focus();
    };

    const removeThread = (t: ChatThread) => {
        setThreads((prev) => prev.filter((x) => x.id !== t.id));
        if (thread?.id === t.id) newChat();
        undoDelete.start(
            `Deleted the chat "${t.title}"`,
            async () => {
                await chatApi.remove(t.id);
                loadThreads();
            },
            () => loadThreads(),
        );
    };

    const cite = (c: Citation) => {
        requestRecordingSeek(c.meeting_id, c.timestamp_ms ?? 0);
        onOpenRecording(c.meeting_id);
    };

    const suggestions = suggestedQuestions(scope, summary);
    const scopeText = summary?.label ?? scopeLabel(scope);
    const count = summary ? ` · ${summary.count} recording${summary.count === 1 ? "" : "s"}` : "";

    return (
        <div className="rc" data-testid="recordings-chat">
            <div className="rc-header">
                <div>
                    <h2>Chat</h2>
                    <span className="rc-header__status">Answers come from {scopeText}{count}, searched on this Mac</span>
                </div>
                <div className="rc-header__actions">
                    <button className="rc-btn" onClick={() => setShowThreads((v) => !v)} aria-expanded={showThreads}>
                        Chats{threads.length ? ` (${threads.length})` : ""}
                    </button>
                    <button className="rc-btn" onClick={newChat} disabled={!thread && messages.length === 0 && !input}>
                        New chat
                    </button>
                </div>
            </div>

            <ScopePicker scope={scope} onChange={setScope} disabled={thinking} selectedMeetingId={selectedMeetingId} />

            {showSetup && (
                <div className="rc-notice"><AiSetupNotice feature="Chat" /></div>
            )}

            <div className="rc-body">
                {showThreads && (
                    <aside className="rc-threads" aria-label="Past chats">
                        {threads.length === 0 ? (
                            <p className="rc-muted">No chats yet.</p>
                        ) : (
                            threads.map((t) => (
                                <div key={t.id} className={`rc-thread ${thread?.id === t.id ? "is-on" : ""}`}>
                                    <button className="rc-thread__open" onClick={() => openThread(t)} title={t.title}>
                                        <span className="rc-thread__title">{t.title}</span>
                                        <span className="rc-thread__meta">
                                            {scopeLabel(t.scope)} · {new Date(t.updated_at).toLocaleDateString()}
                                            {t.flag ? " · answers removed" : ""}
                                        </span>
                                    </button>
                                    <button className="rc-thread__delete" onClick={() => removeThread(t)} title="Delete chat" aria-label="Delete chat">
                                        <TrashIcon size={13} />
                                    </button>
                                </div>
                            ))
                        )}
                    </aside>
                )}

                <div className="rc-messages">
                    {thread?.flag && (
                        <div className="rc-flag" role="status">{thread.flag}</div>
                    )}
                    {messages.length === 0 && !thinking ? (
                        <div className="rc-empty">
                            <span className="rc-empty__icon"><BrainIcon size={40} strokeWidth={1.5} /></span>
                            <span className="rc-empty__title">Ask your recordings</span>
                            <span className="rc-empty__subtitle">
                                Answers come from your transcripts, notes and marked moments in <strong>{scopeText}</strong>,
                                found on this Mac. Only the best excerpts go to your AI, and every answer cites where it came from.
                            </span>
                            {suggestions.length > 0 && (
                                <div className="rc-suggestions">
                                    {suggestions.map((s) => (
                                        <button key={s} className="rc-suggestion" onClick={() => send(s)} disabled={thinking || showSetup}>
                                            {s}
                                        </button>
                                    ))}
                                </div>
                            )}
                        </div>
                    ) : (
                        <>
                            {messages.map((m) => (
                                <div key={m.id} className={`rc-msg ${m.role}`}>
                                    <span className="rc-msg__label">
                                        {m.role === "user" ? "You" : `From ${m.scope_label ?? "your recordings"}`}
                                    </span>
                                    <div className="rc-msg__bubble">
                                        {m.role === "user" ? (
                                            <p className="rc-user-text">{m.content}</p>
                                        ) : (
                                            <ChatMarkdown content={m.content} citations={m.citations} onCite={cite} />
                                        )}
                                    </div>
                                    {m.role === "assistant" && m.citations.length > 0 && (
                                        <SourcesList citations={m.citations} onCite={cite} />
                                    )}
                                </div>
                            ))}
                            {thinking && (
                                <div className="rc-thinking" role="status">
                                    <div className="rc-thinking__dots"><span /><span /><span /></div>
                                    <span className="rc-thinking__label">Searching your recordings, then asking your AI…</span>
                                </div>
                            )}
                            <div ref={endRef} />
                        </>
                    )}
                    {error && <div className="rc-error" role="alert">{error}</div>}
                </div>
            </div>
            {undoDelete.toast}

            <div className="rc-input">
                <textarea
                    ref={textareaRef}
                    className="rc-textarea"
                    placeholder={`Ask about ${scopeText}…`}
                    value={input}
                    onChange={(e) => setInput(e.target.value)}
                    onKeyDown={(e) => {
                        if (e.key === "Enter" && !e.shiftKey) {
                            e.preventDefault();
                            void send(input);
                        }
                    }}
                    rows={1}
                    disabled={thinking || showSetup}
                    aria-label="Your question"
                />
                <button
                    className={`rc-send ${thinking ? "is-sending" : ""}`}
                    onClick={() => void send(input)}
                    disabled={!input.trim() || thinking || showSetup}
                    title="Send"
                    aria-label="Send"
                >
                    ▶
                </button>
            </div>
        </div>
    );
}

function SourcesList({ citations, onCite }: { citations: Citation[]; onCite: (c: Citation) => void }) {
    const [open, setOpen] = useState(false);
    return (
        <div className="rc-sources">
            <button className="rc-sources__toggle" onClick={() => setOpen((v) => !v)} aria-expanded={open}>
                {open ? "▾" : "▸"} {citations.length} source{citations.length === 1 ? "" : "s"}
            </button>
            {open && (
                <div className="rc-sources__list">
                    {citations.map((c) => (
                        <button key={c.n} className="rc-source" onClick={() => onCite(c)} title="Open this moment in the recording">
                            <span className="rc-source__n">{c.n}</span>
                            <span className="rc-source__title">{c.title}</span>
                            <span className="rc-source__when">
                                {c.source === "notes" ? "notes" : c.source === "marker" ? "marked moment" : ""}
                                {c.timestamp_ms != null ? ` ${clock(c.timestamp_ms)}` : ""}
                            </span>
                            <span className="rc-source__excerpt">{c.excerpt}</span>
                        </button>
                    ))}
                </div>
            )}
        </div>
    );
}

function clock(ms: number): string {
    const s = Math.floor(ms / 1000);
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = s % 60;
    return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}` : `${m}:${String(sec).padStart(2, "0")}`;
}
