
import { useState, useEffect, useRef } from 'react';
import { Send, User, X, Loader2, FileText, CheckSquare, Brain, Search, BookOpen } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';

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

interface Message {
    id: string;
    role: 'user' | 'assistant' | 'system';
    content: string;
    timestamp: number;
    context?: ContextItem[];
}

interface CopilotPanelProps {
    meetingId?: string;
    onClose?: () => void;
}

// TheBrain models
const MODELS = [
    { id: 'qwen3:8b', name: 'Qwen3 8B' },
    { id: 'qwen3:14b', name: 'Qwen3 14B' },
    { id: 'qwen3-vl:8b', name: 'Qwen3 VL' },
    { id: 'qwen2.5-coder:7b', name: 'Coder 7B' },
];

export function CopilotPanel({ meetingId: _meetingId, onClose }: CopilotPanelProps) {
    const [messages, setMessages] = useState<Message[]>([
        {
            id: 'welcome',
            role: 'assistant',
            content: 'TheBrain online with RAG. I can search your history for context.',
            timestamp: Date.now()
        }
    ]);
    const [input, setInput] = useState('');
    const [isLoading, setIsLoading] = useState(false);
    const [connected, setConnected] = useState(false);
    const [selectedModel, setSelectedModel] = useState('qwen3:8b');
    const [ragEnabled, setRagEnabled] = useState(true);
    const [expandedContext, setExpandedContext] = useState<string | null>(null);
    const messagesEndRef = useRef<HTMLDivElement>(null);

    const scrollToBottom = () => {
        messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    };

    useEffect(() => {
        scrollToBottom();
    }, [messages]);

    useEffect(() => {
        checkConnection();
    }, []);

    const checkConnection = async () => {
        try {
            const result = await invoke<boolean>('check_thebrain');
            setConnected(result);
        } catch {
            setConnected(false);
        }
    };

    const handleSend = async (text: string = input) => {
        if (!text.trim() || isLoading) return;

        const userMsg: Message = {
            id: Date.now().toString(),
            role: 'user',
            content: text,
            timestamp: Date.now()
        };

        setMessages(prev => [...prev, userMsg]);
        setInput('');
        setIsLoading(true);

        try {
            let responseContent: string;
            let contextItems: ContextItem[] = [];

            if (ragEnabled) {
                const ragResponse = await invoke<RagChatResponse>('thebrain_rag_chat_with_memory', {
                    message: text,
                    model: selectedModel,
                    topK: 5,
                });
                responseContent = ragResponse.response;
                contextItems = ragResponse.context_used;
            } else {
                responseContent = await invoke<string>('thebrain_chat', {
                    message: text,
                    model: selectedModel
                });
            }

            const aiMsg: Message = {
                id: (Date.now() + 1).toString(),
                role: 'assistant',
                content: responseContent,
                timestamp: Date.now(),
                context: contextItems
            };
            setMessages(prev => [...prev, aiMsg]);
        } catch (error) {
            console.error('TheBrain Chat Error:', error);
            const errorMsg: Message = {
                id: (Date.now() + 1).toString(),
                role: 'system',
                content: `Error: ${error instanceof Error ? error.message : String(error)}`,
                timestamp: Date.now()
            };
            setMessages(prev => [...prev, errorMsg]);
        } finally {
            setIsLoading(false);
        }
    };

    const handlePreset = (preset: 'summarize' | 'action_items' | 'history') => {
        switch (preset) {
            case 'summarize':
                handleSend("Generate a concise summary of the meeting so far.");
                break;
            case 'action_items':
                handleSend("Identify all action items and tasks discussed.");
                break;
            case 'history':
                handleSend("What topics have I discussed in recent meetings? Give me an overview.");
                break;
        }
    };

    return (
        <div className="flex flex-col h-full bg-[#0B0C10] border-l border-[#1F2937] w-80 animate-slide-in-right">
            {/* Header */}
            <div className="flex items-center justify-between p-4 border-b border-[#1F2937] bg-[#111318]">
                <div className="flex items-center space-x-2">
                    <Brain className="w-4 h-4 text-amber-500" />
                    <h3 className="text-sm font-semibold tracking-wider uppercase text-gray-200">
                        TheBrain
                    </h3>
                </div>
                <div className="flex items-center space-x-2">
                    <span className={`w-2 h-2 rounded-full ${ragEnabled ? 'bg-emerald-500' : connected ? 'bg-amber-500' : 'bg-red-500'}`}></span>
                    {onClose && (
                        <button onClick={onClose} className="text-gray-500 hover:text-white transition-colors">
                            <X className="w-4 h-4" />
                        </button>
                    )}
                </div>
            </div>

            {/* Model Selector + RAG Toggle */}
            <div className="p-2 border-b border-[#1F2937] bg-[#0B0C10] flex items-center gap-2">
                <select
                    value={selectedModel}
                    onChange={(e) => setSelectedModel(e.target.value)}
                    className="flex-1 bg-[#1F2937] border border-[#374151] rounded px-2 py-1 text-xs text-gray-300"
                >
                    {MODELS.map(m => (
                        <option key={m.id} value={m.id}>{m.name}</option>
                    ))}
                </select>
                <label className="flex items-center gap-1 text-xs text-gray-400 cursor-pointer">
                    <input
                        type="checkbox"
                        checked={ragEnabled}
                        onChange={(e) => setRagEnabled(e.target.checked)}
                        className="w-3 h-3 accent-emerald-500"
                    />
                    <BookOpen className="w-3 h-3" />
                </label>
            </div>

            {/* Quick Actions */}
            <div className="p-3 grid grid-cols-3 gap-2 border-b border-[#1F2937] bg-[#0B0C10]">
                <button
                    onClick={() => handlePreset('summarize')}
                    disabled={isLoading}
                    className="flex items-center justify-center space-x-1 px-2 py-2 bg-[#1F2937]/50 hover:bg-[#1F2937] border border-[#374151] rounded text-xs text-gray-300 transition-all hover:text-white hover:border-amber-500/50"
                >
                    <FileText className="w-3 h-3" />
                    <span>Summary</span>
                </button>
                <button
                    onClick={() => handlePreset('action_items')}
                    disabled={isLoading}
                    className="flex items-center justify-center space-x-1 px-2 py-2 bg-[#1F2937]/50 hover:bg-[#1F2937] border border-[#374151] rounded text-xs text-gray-300 transition-all hover:text-white hover:border-amber-500/50"
                >
                    <CheckSquare className="w-3 h-3" />
                    <span>Tasks</span>
                </button>
                <button
                    onClick={() => handlePreset('history')}
                    disabled={isLoading}
                    className="flex items-center justify-center space-x-1 px-2 py-2 bg-emerald-900/30 hover:bg-emerald-900/50 border border-emerald-700/50 rounded text-xs text-emerald-400 transition-all hover:text-emerald-300"
                >
                    <Search className="w-3 h-3" />
                    <span>History</span>
                </button>
            </div>

            {/* Messages Area */}
            <div className="flex-1 overflow-y-auto p-4 space-y-4 font-mono text-sm relative">
                {/* Background Grid */}
                <div className="absolute inset-0 pointer-events-none opacity-[0.02]"
                    style={{
                        backgroundImage: `linear-gradient(#374151 1px, transparent 1px), linear-gradient(90deg, #374151 1px, transparent 1px)`,
                        backgroundSize: '20px 20px'
                    }}
                />

                {messages.map((msg) => (
                    <div
                        key={msg.id}
                        className={`flex flex-col ${msg.role === 'user' ? 'items-end' : 'items-start'} relative z-10 animate-fade-in`}
                    >
                        <div className={`flex items-start max-w-[90%] space-x-2 ${msg.role === 'user' ? 'flex-row-reverse space-x-reverse' : ''}`}>
                            <div className={`w-6 h-6 rounded flex items-center justify-center flex-shrink-0 mt-0.5 ${msg.role === 'user' ? 'bg-[#374151]' : msg.role === 'system' ? 'bg-red-900/20 text-red-500' : 'bg-amber-900/20 text-amber-500'
                                }`}>
                                {msg.role === 'user' ? <User className="w-3.5 h-3.5" /> : msg.role === 'system' ? <X className="w-3.5 h-3.5" /> : <Brain className="w-3.5 h-3.5" />}
                            </div>

                            <div className={`p-2.5 rounded text-xs leading-relaxed border ${msg.role === 'user'
                                ? 'bg-[#1F2937] border-[#374151] text-gray-100'
                                : msg.role === 'system'
                                    ? 'bg-red-950/10 border-red-900/30 text-red-400'
                                    : 'bg-[#111318] border-[#1F2937] text-gray-300'
                                }`}>
                                <div className="whitespace-pre-wrap">{msg.content}</div>

                                {/* Context cards for RAG responses */}
                                {msg.context && msg.context.length > 0 && (
                                    <div className="mt-2 pt-2 border-t border-[#374151]">
                                        <button
                                            onClick={() => setExpandedContext(expandedContext === msg.id ? null : msg.id)}
                                            className="text-emerald-400 text-[10px] flex items-center gap-1 hover:text-emerald-300"
                                        >
                                            <BookOpen className="w-2.5 h-2.5" />
                                            {msg.context.length} sources {expandedContext === msg.id ? '▼' : '▶'}
                                        </button>
                                        {expandedContext === msg.id && (
                                            <div className="mt-2 space-y-1">
                                                {msg.context.map((ctx) => (
                                                    <div key={ctx.id} className="bg-emerald-950/20 border border-emerald-900/30 rounded p-1.5 text-[10px]">
                                                        <div className="flex justify-between text-emerald-400">
                                                            <span>{Math.round(ctx.score * 100)}%</span>
                                                            {ctx.timestamp && <span>{ctx.timestamp.split('T')[0]}</span>}
                                                        </div>
                                                        <p className="text-gray-400 mt-0.5 line-clamp-2">{ctx.summary}</p>
                                                    </div>
                                                ))}
                                            </div>
                                        )}
                                    </div>
                                )}
                            </div>
                        </div>
                        <span className="text-[10px] text-gray-600 mt-1 px-1">
                            {new Date(msg.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                        </span>
                    </div>
                ))}

                {isLoading && (
                    <div className="flex items-start space-x-2">
                        <div className="w-6 h-6 rounded bg-amber-900/20 text-amber-500 flex items-center justify-center">
                            <Brain className="w-3.5 h-3.5" />
                        </div>
                        <div className="flex items-center space-x-1 p-2">
                            {ragEnabled && <span className="text-emerald-400 text-[10px] mr-1">Searching...</span>}
                            <div className="w-1.5 h-1.5 bg-amber-500/50 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                            <div className="w-1.5 h-1.5 bg-amber-500/50 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                            <div className="w-1.5 h-1.5 bg-amber-500/50 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
                        </div>
                    </div>
                )}
                <div ref={messagesEndRef} />
            </div>

            {/* Input Area */}
            <div className="p-3 border-t border-[#1F2937] bg-[#111318]">
                <form
                    onSubmit={(e) => {
                        e.preventDefault();
                        handleSend();
                    }}
                    className="relative"
                >
                    <input
                        type="text"
                        value={input}
                        onChange={(e) => setInput(e.target.value)}
                        placeholder={ragEnabled ? "Ask with history context..." : "Ask TheBrain..."}
                        className="w-full bg-[#0B0C10] border border-[#374151] rounded py-2 pl-3 pr-10 text-xs text-white placeholder-gray-600 focus:outline-none focus:border-amber-500/50 focus:ring-1 focus:ring-amber-500/20 transition-all"
                        disabled={isLoading}
                    />
                    <button
                        type="submit"
                        disabled={!input.trim() || isLoading}
                        className="absolute right-1.5 top-1.5 p-1 text-gray-500 hover:text-amber-500 disabled:opacity-50 disabled:hover:text-gray-500 transition-colors"
                    >
                        {isLoading ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Send className="w-3.5 h-3.5" />}
                    </button>
                </form>
                <div className="mt-2 flex justify-between items-center text-[10px] text-gray-600 px-1">
                    <span className="flex items-center">
                        <span className={`w-1.5 h-1.5 rounded-full ${ragEnabled ? 'bg-emerald-500' : connected ? 'bg-amber-500' : 'bg-red-500'} mr-1.5`}></span>
                        {ragEnabled ? 'RAG ACTIVE' : connected ? 'THEBRAIN ONLINE' : 'DISCONNECTED'}
                    </span>
                    <span>{selectedModel.toUpperCase()}</span>
                </div>
            </div>
        </div>
    );
}
