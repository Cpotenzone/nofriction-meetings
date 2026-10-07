import React, { useState, useRef, useEffect } from "react";
import { chatWithData, ChatHistoryMessage, ChatSource } from "../../../lib/tauri";
import "./DataChat.css";
import { BrainIcon } from "../../icons";
import { aiErrorClass, friendlyAiError, isNoProviderError } from "../../../lib/ai";
import { AiSetupNotice, useAiStatus } from "../../AiSetupNotice";

interface DisplayMessage {
    role: "user" | "assistant";
    content: string;
    sources?: ChatSource[];
}

export const DataChat: React.FC = () => {
    const [messages, setMessages] = useState<DisplayMessage[]>([]);
    const [input, setInput] = useState("");
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [expandedSources, setExpandedSources] = useState<Set<number>>(new Set());
    const messagesEndRef = useRef<HTMLDivElement>(null);
    const textareaRef = useRef<HTMLTextAreaElement>(null);
    const { configured } = useAiStatus();
    const [needsAi, setNeedsAi] = useState(false);
    const showSetup = configured === false || needsAi;
    // A provider was just connected in Settings: clear the earlier failure
    useEffect(() => {
        if (configured) setNeedsAi(false);
    }, [configured]);

    const scrollToBottom = () => {
        messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
    };

    useEffect(() => {
        scrollToBottom();
    }, [messages, isLoading]);

    // Auto-resize textarea
    useEffect(() => {
        if (textareaRef.current) {
            textareaRef.current.style.height = "44px";
            textareaRef.current.style.height = Math.min(textareaRef.current.scrollHeight, 120) + "px";
        }
    }, [input]);

    const handleSend = async () => {
        const trimmed = input.trim();
        if (!trimmed || isLoading) return;

        setError(null);
        const userMessage: DisplayMessage = { role: "user", content: trimmed };
        setMessages((prev) => [...prev, userMessage]);
        setInput("");

        // Build history from previous messages (exclude current)
        const history: ChatHistoryMessage[] = messages.map((m) => ({
            role: m.role,
            content: m.content,
        }));

        setIsLoading(true);
        try {
            const response = await chatWithData(trimmed, history);
            const assistantMessage: DisplayMessage = {
                role: "assistant",
                content: response.answer,
                sources: response.sources,
            };
            setMessages((prev) => [...prev, assistantMessage]);
        } catch (err) {
            console.error("Chat error:", friendlyAiError(err));
            if (isNoProviderError(err)) {
                setNeedsAi(true);
            } else if (aiErrorClass(err) === "pro_required") {
                setError("Chat is part of noFriction Pro.");
            } else if (aiErrorClass(err) === "consent_required") {
                setError("Chat needs your permission to send recording excerpts to your AI provider. Ask again and choose Allow.");
            } else {
                setError(friendlyAiError(err));
            }
        } finally {
            setIsLoading(false);
        }
    };

    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            handleSend();
        }
    };

    const handleSuggestion = (text: string) => {
        setInput(text);
        textareaRef.current?.focus();
    };

    const toggleSources = (index: number) => {
        setExpandedSources((prev) => {
            const next = new Set(prev);
            if (next.has(index)) next.delete(index);
            else next.add(index);
            return next;
        });
    };

    const clearChat = () => {
        setMessages([]);
        setError(null);
        setExpandedSources(new Set());
    };

    const getSourceBadgeClass = (source: string) => {
        if (source === "local") return "source-badge local";
        return "source-badge";
    };

    const formatTimestamp = (ts: string | null) => {
        if (!ts) return "";
        try {
            const d = new Date(ts);
            return d.toLocaleDateString(undefined, { month: "short", day: "numeric" }) +
                " " + d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
        } catch {
            return ts;
        }
    };

    const suggestions = [
        "What were the key points from my last recording?",
        "Summarize what I worked on today",
        "What action items are outstanding?",
        "Who did I meet with this week?",
        "What apps was I using during my last recording?",
    ];

    return (
        <div className="data-chat">
            {/* Header */}
            <div className="data-chat-header">
                <div>
                    <h2>
                        <span className="header-icon">💬</span>
                        CHAT WITH YOUR RECORDINGS
                    </h2>
                    <span className="header-status">
                        Your recordings, searched on this Mac
                    </span>
                </div>
                {messages.length > 0 && (
                    <button className="header-clear-btn" onClick={clearChat}>
                        Clear
                    </button>
                )}
            </div>

            {showSetup && (
                <div style={{ padding: "12px 20px 0" }}>
                    <AiSetupNotice feature="Chat" />
                </div>
            )}

            {/* Messages */}
            <div className="data-chat-messages">
                {messages.length === 0 && !isLoading ? (
                    <div className="data-chat-empty">
                        <span className="empty-icon"><BrainIcon size={40} strokeWidth={1.5} /></span>
                        <span className="empty-title">Talk to your data</span>
                        <span className="empty-subtitle">
                            Ask about your recordings, transcripts and screen activity. noFriction searches them on this
                            Mac and sends only the most relevant excerpts to your AI provider for the answer.
                        </span>
                        <div className="empty-suggestions">
                            {suggestions.map((s, i) => (
                                <button
                                    key={i}
                                    className="suggestion-chip"
                                    onClick={() => handleSuggestion(s)}
                                >
                                    {s}
                                </button>
                            ))}
                        </div>
                    </div>
                ) : (
                    <>
                        {messages.map((msg, i) => (
                            <div key={i} className={`chat-message ${msg.role}`}>
                                <span className="message-label">
                                    {msg.role === "user" ? "YOU" : "NOFRICTION AI"}
                                </span>
                                <div className="message-bubble">{msg.content}</div>

                                {/* Source citations */}
                                {msg.sources && msg.sources.length > 0 && (
                                    <div className="message-sources">
                                        <button
                                            className="sources-toggle"
                                            onClick={() => toggleSources(i)}
                                        >
                                            {expandedSources.has(i) ? "▾" : "▸"}{" "}
                                            {msg.sources.length} source{msg.sources.length !== 1 ? "s" : ""} referenced
                                        </button>
                                        {expandedSources.has(i) && (
                                            <div className="sources-list">
                                                {msg.sources.map((src, si) => (
                                                    <div key={si} className="source-item">
                                                        <span className={getSourceBadgeClass(src.source)}>
                                                            {src.source}
                                                        </span>
                                                        <span className="source-text">
                                                            {src.summary}
                                                        </span>
                                                        {src.timestamp && (
                                                            <span className="source-score">
                                                                {formatTimestamp(src.timestamp)}
                                                            </span>
                                                        )}
                                                        {src.score !== null && (
                                                            <span className="source-score">
                                                                {(src.score * 100).toFixed(0)}%
                                                            </span>
                                                        )}
                                                    </div>
                                                ))}
                                            </div>
                                        )}
                                    </div>
                                )}
                            </div>
                        ))}

                        {/* Typing indicator */}
                        {isLoading && (
                            <div className="typing-indicator">
                                <div className="typing-dots">
                                    <span />
                                    <span />
                                    <span />
                                </div>
                                <span className="typing-label">Searching & thinking...</span>
                            </div>
                        )}

                        {/* Error */}
                        {error && (
                            <div className="chat-error">
                                ⚠️ {error}
                            </div>
                        )}

                        <div ref={messagesEndRef} />
                    </>
                )}
            </div>

            {/* Input */}
            <div className="data-chat-input">
                <textarea
                    ref={textareaRef}
                    className="chat-textarea"
                    placeholder="Ask about your recordings, transcripts, or screen activity..."
                    value={input}
                    onChange={(e) => setInput(e.target.value)}
                    onKeyDown={handleKeyDown}
                    rows={1}
                    disabled={isLoading || showSetup}
                />
                <button
                    className={`chat-send-btn ${isLoading ? "sending" : ""}`}
                    onClick={handleSend}
                    disabled={!input.trim() || isLoading || showSetup}
                    title="Send message"
                >
                    ▶
                </button>
            </div>
        </div>
    );
};
