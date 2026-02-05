// noFriction Meetings - AI Chat Component
// Chat interface with TheBrain Cloud API and RAG (Retrieval Augmented Generation)

import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface TheBrainModel {
    id: string;
    loaded: boolean;
    size_gb?: number;
    preload: boolean;
}

interface ContextItem {
    id: string;
    score: number;
    summary: string;
    timestamp?: string;
    category?: string;
}

interface RagChatResponse {
    response: string;
    context_used: ContextItem[];
    model: string;
}

interface ChatMessage {
    id: string;
    role: "user" | "assistant" | "system";
    content: string;
    timestamp: Date;
    context?: ContextItem[]; // Context items used for this response
}

interface AIChatProps {
    meetingId: string | null;
}

// Available TheBrain models
const THEBRAIN_MODELS = [
    { id: "qwen3:8b", name: "Qwen3 8B", description: "General reasoning" },
    { id: "qwen3:14b", name: "Qwen3 14B", description: "Deep analysis" },
    { id: "qwen3-vl:8b", name: "Qwen3 VL 8B", description: "Vision + Language" },
    { id: "qwen2.5-coder:7b", name: "Qwen2.5 Coder", description: "Code generation" },
    { id: "qwen2.5vl:7b", name: "Qwen2.5 VL", description: "Alternate vision" },
];

export function AIChat({ meetingId }: AIChatProps) {
    const [messages, setMessages] = useState<ChatMessage[]>([]);
    const [input, setInput] = useState("");
    const [isLoading, setIsLoading] = useState(false);
    const [thebrainConnected, setThebrainConnected] = useState<boolean | null>(null);
    const [availableModels, setAvailableModels] = useState<TheBrainModel[]>([]);
    const [selectedModel, setSelectedModel] = useState<string>("qwen3:8b");
    const [ragEnabled, setRagEnabled] = useState(true); // Enable RAG by default
    const [showContext, setShowContext] = useState(false);
    const [captureCount, setCaptureCount] = useState(0);
    const [isCapturing, setIsCapturing] = useState(false);

    const chatRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLTextAreaElement>(null);

    // Check TheBrain status on mount
    useEffect(() => {
        checkTheBrain();
    }, []);

    // Auto-scroll to bottom on new messages
    useEffect(() => {
        if (chatRef.current) {
            chatRef.current.scrollTop = chatRef.current.scrollHeight;
        }
    }, [messages]);

    const checkTheBrain = async () => {
        try {
            const connected = await invoke<boolean>("check_thebrain");
            setThebrainConnected(connected);

            if (connected) {
                try {
                    const models = await invoke<TheBrainModel[]>("get_thebrain_models");
                    setAvailableModels(models);
                    // Select first loaded model if available
                    const loadedModel = models.find(m => m.loaded);
                    if (loadedModel) {
                        setSelectedModel(loadedModel.id);
                    }
                } catch (e) {
                    console.error("Failed to get models:", e);
                }
            }
        } catch (err) {
            console.error("Failed to check TheBrain:", err);
            setThebrainConnected(false);
        }
    };

    // Capture current screen content via accessibility API
    const captureScreen = async () => {
        if (isCapturing) return;
        setIsCapturing(true);
        try {
            const result = await invoke<{
                success: boolean;
                text_preview: string;
                word_count: number;
                source: string;
            }>("capture_accessibility_snapshot");

            setCaptureCount(prev => prev + 1);

            // Add system message about the capture
            const captureMsg: ChatMessage = {
                id: `capture-${Date.now()}`,
                role: "system",
                content: `📸 Captured ${result.word_count} words from screen. Preview: "${result.text_preview.slice(0, 100)}..."`,
                timestamp: new Date(),
            };
            setMessages(prev => [...prev, captureMsg]);
        } catch (err) {
            const errorMsg: ChatMessage = {
                id: `capture-error-${Date.now()}`,
                role: "system",
                content: `⚠️ Screen capture failed: ${err}`,
                timestamp: new Date(),
            };
            setMessages(prev => [...prev, errorMsg]);
        } finally {
            setIsCapturing(false);
        }
    };

    const sendMessage = useCallback(async () => {
        if (!input.trim() || isLoading) return;

        const userMessage: ChatMessage = {
            id: `user-${Date.now()}`,
            role: "user",
            content: input.trim(),
            timestamp: new Date(),
        };

        setMessages((prev) => [...prev, userMessage]);
        setInput("");
        setIsLoading(true);

        try {
            let responseContent: string;
            let contextItems: ContextItem[] = [];

            if (ragEnabled) {
                // Use RAG chat with memory - searches history and stores conversation
                const ragResponse = await invoke<RagChatResponse>("thebrain_rag_chat_with_memory", {
                    message: userMessage.content,
                    model: selectedModel,
                    topK: 5,
                });
                responseContent = ragResponse.response;
                contextItems = ragResponse.context_used;
            } else {
                // Use simple chat without RAG
                responseContent = await invoke<string>("thebrain_chat", {
                    message: userMessage.content,
                    model: selectedModel,
                });
            }

            const assistantMessage: ChatMessage = {
                id: `assistant-${Date.now()}`,
                role: "assistant",
                content: responseContent,
                timestamp: new Date(),
                context: contextItems,
            };

            setMessages((prev) => [...prev, assistantMessage]);
        } catch (err) {
            const errorMessage: ChatMessage = {
                id: `error-${Date.now()}`,
                role: "system",
                content: `Error: ${err}`,
                timestamp: new Date(),
            };
            setMessages((prev) => [...prev, errorMessage]);
        } finally {
            setIsLoading(false);
        }
    }, [input, isLoading, selectedModel, ragEnabled]);

    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            sendMessage();
        }
    };

    const quickAction = async (action: "summarize" | "action_items" | "history") => {
        if (action !== "history" && !meetingId) {
            setMessages((prev) => [...prev, {
                id: `system-${Date.now()}`,
                role: "system",
                content: "Please select a meeting first.",
                timestamp: new Date(),
            }]);
            return;
        }

        setIsLoading(true);
        let prompt: string;
        let userLabel: string;

        switch (action) {
            case "summarize":
                prompt = "Please provide a concise summary of the meeting content.";
                userLabel = "📝 Summarize meeting";
                break;
            case "action_items":
                prompt = "Please extract all action items and tasks from the meeting.";
                userLabel = "✅ Extract action items";
                break;
            case "history":
                prompt = "What did I discuss in my recent meetings? Give me a brief overview.";
                userLabel = "🔍 Search my history";
                break;
        }

        setMessages((prev) => [...prev, {
            id: `user-${Date.now()}`,
            role: "user",
            content: userLabel,
            timestamp: new Date(),
        }]);

        try {
            const ragResponse = await invoke<RagChatResponse>("thebrain_rag_chat_with_memory", {
                message: prompt,
                model: selectedModel,
                topK: 10,
            });

            setMessages((prev) => [...prev, {
                id: `assistant-${Date.now()}`,
                role: "assistant",
                content: ragResponse.response,
                timestamp: new Date(),
                context: ragResponse.context_used,
            }]);
        } catch (err) {
            setMessages((prev) => [...prev, {
                id: `error-${Date.now()}`,
                role: "system",
                content: `Error: ${err}`,
                timestamp: new Date(),
            }]);
        } finally {
            setIsLoading(false);
        }
    };

    if (thebrainConnected === false) {
        return (
            <div className="ai-chat-unavailable">
                <div className="unavailable-content">
                    <span className="unavailable-icon">🧠</span>
                    <h3>TheBrain Not Connected</h3>
                    <p>Please login to TheBrain in Settings → Knowledge Base.</p>
                    <button className="retry-button" onClick={checkTheBrain}>
                        Retry Connection
                    </button>
                </div>
            </div>
        );
    }

    return (
        <div className="ai-chat">
            {/* Header with model selector and RAG toggle */}
            <div className="ai-chat-header">
                <div className="model-selector">
                    <label>🧠 Model:</label>
                    <select
                        value={selectedModel}
                        onChange={(e) => setSelectedModel(e.target.value)}
                    >
                        {THEBRAIN_MODELS.map((m) => {
                            const isLoaded = availableModels.find(am => am.id === m.id)?.loaded;
                            return (
                                <option key={m.id} value={m.id}>
                                    {m.name} {isLoaded ? "✓" : ""}
                                </option>
                            );
                        })}
                    </select>
                </div>
                <div className="rag-toggle">
                    <label className="toggle-label">
                        <input
                            type="checkbox"
                            checked={ragEnabled}
                            onChange={(e) => setRagEnabled(e.target.checked)}
                        />
                        <span>📚 Use History</span>
                    </label>
                </div>
                <div className="connection-status">
                    <span className={`status-dot ${ragEnabled ? 'rag-active' : 'connected'}`}></span>
                    <span>{ragEnabled ? 'RAG Active' : 'TheBrain'}</span>
                </div>
                <button
                    className="capture-btn"
                    onClick={captureScreen}
                    disabled={isCapturing}
                    title="Capture screen text via accessibility API"
                >
                    {isCapturing ? '📸...' : '📸'} {captureCount > 0 && `(${captureCount})`}
                </button>
            </div>

            {/* Quick actions */}
            <div className="ai-quick-actions">
                <button
                    className="quick-action-btn"
                    onClick={() => quickAction("summarize")}
                    disabled={isLoading || !meetingId}
                >
                    📝 Summarize
                </button>
                <button
                    className="quick-action-btn"
                    onClick={() => quickAction("action_items")}
                    disabled={isLoading || !meetingId}
                >
                    ✅ Actions
                </button>
                <button
                    className="quick-action-btn history"
                    onClick={() => quickAction("history")}
                    disabled={isLoading}
                >
                    🔍 History
                </button>
            </div>

            {/* Messages */}
            <div className="ai-chat-messages scrollable" ref={chatRef}>
                {messages.length === 0 ? (
                    <div className="chat-empty">
                        <span className="chat-empty-icon">💬</span>
                        <p>Chat with TheBrain AI</p>
                        {ragEnabled ? (
                            <p className="chat-hint">📚 RAG enabled - I'll search your history for context</p>
                        ) : meetingId ? (
                            <p className="chat-hint">Meeting context loaded. Try "What was discussed?"</p>
                        ) : (
                            <p className="chat-hint">Ask any question - powered by TheBrain Cloud</p>
                        )}
                    </div>
                ) : (
                    messages.map((msg) => (
                        <div key={msg.id} className={`chat-message ${msg.role}`}>
                            <div className="message-avatar">
                                {msg.role === "user" ? "👤" : msg.role === "assistant" ? "🧠" : "⚠️"}
                            </div>
                            <div className="message-content">
                                <pre>{msg.content}</pre>
                                {/* Context cards for RAG responses */}
                                {msg.context && msg.context.length > 0 && (
                                    <div className="context-section">
                                        <button
                                            className="context-toggle"
                                            onClick={() => setShowContext(!showContext)}
                                        >
                                            📚 {msg.context.length} sources used {showContext ? '▼' : '▶'}
                                        </button>
                                        {showContext && (
                                            <div className="context-cards">
                                                {msg.context.map((ctx) => (
                                                    <div key={ctx.id} className="context-card">
                                                        <div className="context-header">
                                                            <span className="context-score">
                                                                {Math.round(ctx.score * 100)}% match
                                                            </span>
                                                            {ctx.timestamp && (
                                                                <span className="context-time">{ctx.timestamp}</span>
                                                            )}
                                                        </div>
                                                        <p className="context-summary">{ctx.summary}</p>
                                                    </div>
                                                ))}
                                            </div>
                                        )}
                                    </div>
                                )}
                            </div>
                        </div>
                    ))
                )}
                {isLoading && (
                    <div className="chat-message assistant loading">
                        <div className="message-avatar">🧠</div>
                        <div className="message-content">
                            <div className="typing-indicator">
                                {ragEnabled && <span className="rag-searching">Searching history...</span>}
                                <span></span>
                                <span></span>
                                <span></span>
                            </div>
                        </div>
                    </div>
                )}
            </div>

            {/* Input area */}
            <div className="ai-chat-input">
                <textarea
                    ref={inputRef}
                    value={input}
                    onChange={(e) => setInput(e.target.value)}
                    onKeyDown={handleKeyDown}
                    placeholder={ragEnabled ? "Ask about your meetings..." : "Ask TheBrain anything..."}
                    rows={2}
                    disabled={isLoading}
                />
                <button
                    className="send-button"
                    onClick={sendMessage}
                    disabled={isLoading || !input.trim()}
                >
                    {isLoading ? "..." : "→"}
                </button>
            </div>
        </div>
    );
}
