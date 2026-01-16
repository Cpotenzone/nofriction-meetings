// noFriction Meetings - AI Chat Component
// Chat interface for meeting analysis with Ollama

import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface AIPreset {
    id: string;
    name: string;
    description: string;
    model: string;
    system_prompt: string;
    temperature: number;
}

interface OllamaModel {
    name: string;
    size: string;
    modified_at: string;
}

interface ChatMessage {
    id: string;
    role: "user" | "assistant" | "system";
    content: string;
    timestamp: Date;
}

interface AIChatProps {
    meetingId: string | null;
}

export function AIChat({ meetingId }: AIChatProps) {
    const [messages, setMessages] = useState<ChatMessage[]>([]);
    const [input, setInput] = useState("");
    const [isLoading, setIsLoading] = useState(false);
    const [ollamaAvailable, setOllamaAvailable] = useState<boolean | null>(null);
    const [models, setModels] = useState<OllamaModel[]>([]);
    const [presets, setPresets] = useState<AIPreset[]>([]);
    const [selectedPreset, setSelectedPreset] = useState<string>("qa");

    const chatRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLTextAreaElement>(null);

    // Check Ollama status and load presets
    useEffect(() => {
        checkOllama();
        loadPresets();
    }, []);

    // Auto-scroll to bottom on new messages
    useEffect(() => {
        if (chatRef.current) {
            chatRef.current.scrollTop = chatRef.current.scrollHeight;
        }
    }, [messages]);

    const checkOllama = async () => {
        try {
            const available = await invoke<boolean>("check_ollama");
            setOllamaAvailable(available);

            if (available) {
                const modelList = await invoke<OllamaModel[]>("get_ollama_models");
                setModels(modelList);
            }
        } catch (err) {
            console.error("Failed to check Ollama:", err);
            setOllamaAvailable(false);
        }
    };

    const loadPresets = async () => {
        try {
            const presetList = await invoke<AIPreset[]>("get_ai_presets");
            setPresets(presetList);
        } catch (err) {
            console.error("Failed to load presets:", err);
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
            const response = await invoke<string>("ai_chat", {
                presetId: selectedPreset,
                message: userMessage.content,
                meetingId: meetingId,
            });

            const assistantMessage: ChatMessage = {
                id: `assistant-${Date.now()}`,
                role: "assistant",
                content: response,
                timestamp: new Date(),
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
    }, [input, isLoading, selectedPreset, meetingId]);

    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            sendMessage();
        }
    };

    const quickAction = async (action: "summarize" | "action_items") => {
        if (!meetingId) {
            setMessages((prev) => [...prev, {
                id: `system-${Date.now()}`,
                role: "system",
                content: "Please select a meeting first.",
                timestamp: new Date(),
            }]);
            return;
        }

        setIsLoading(true);
        const actionLabel = action === "summarize" ? "Summarizing meeting..." : "Extracting action items...";

        setMessages((prev) => [...prev, {
            id: `user-${Date.now()}`,
            role: "user",
            content: actionLabel,
            timestamp: new Date(),
        }]);

        try {
            const response = await invoke<string>(
                action === "summarize" ? "summarize_meeting" : "extract_action_items",
                { meetingId }
            );

            setMessages((prev) => [...prev, {
                id: `assistant-${Date.now()}`,
                role: "assistant",
                content: response,
                timestamp: new Date(),
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

    if (ollamaAvailable === false) {
        return (
            <div className="ai-chat-unavailable">
                <div className="unavailable-content">
                    <span className="unavailable-icon">🤖</span>
                    <h3>Ollama Not Available</h3>
                    <p>Install and run Ollama to enable AI features.</p>
                    <a
                        href="https://ollama.ai"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="ollama-link"
                    >
                        Download Ollama →
                    </a>
                    <button className="retry-button" onClick={checkOllama}>
                        Retry Connection
                    </button>
                </div>
            </div>
        );
    }

    return (
        <div className="ai-chat">
            {/* Header with preset selector */}
            <div className="ai-chat-header">
                <div className="preset-selector">
                    <label>AI Mode:</label>
                    <select
                        value={selectedPreset}
                        onChange={(e) => setSelectedPreset(e.target.value)}
                    >
                        {presets.map((p) => (
                            <option key={p.id} value={p.id}>
                                {p.name}
                            </option>
                        ))}
                    </select>
                </div>
                {models.length > 0 && (
                    <div className="model-info">
                        <span className="model-badge">
                            🧠 {models[0].name}
                        </span>
                    </div>
                )}
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
                    ✅ Action Items
                </button>
            </div>

            {/* Messages */}
            <div className="ai-chat-messages scrollable" ref={chatRef}>
                {messages.length === 0 ? (
                    <div className="chat-empty">
                        <span className="chat-empty-icon">💬</span>
                        <p>Ask questions about your meeting</p>
                        {meetingId ? (
                            <p className="chat-hint">Meeting context is loaded. Try asking "What was discussed?"</p>
                        ) : (
                            <p className="chat-hint">Select a meeting to enable context-aware responses.</p>
                        )}
                    </div>
                ) : (
                    messages.map((msg) => (
                        <div key={msg.id} className={`chat-message ${msg.role}`}>
                            <div className="message-avatar">
                                {msg.role === "user" ? "👤" : msg.role === "assistant" ? "🤖" : "⚠️"}
                            </div>
                            <div className="message-content">
                                <pre>{msg.content}</pre>
                            </div>
                        </div>
                    ))
                )}
                {isLoading && (
                    <div className="chat-message assistant loading">
                        <div className="message-avatar">🤖</div>
                        <div className="message-content">
                            <div className="typing-indicator">
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
                    placeholder="Ask about the meeting..."
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
