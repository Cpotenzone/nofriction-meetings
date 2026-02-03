
import { useState, useEffect, useRef } from 'react';
import { Send, Bot, User, Sparkles, X, Loader2, FileText, CheckSquare } from 'lucide-react';
import { aiChat } from '../lib/tauri';

interface Message {
    id: string;
    role: 'user' | 'assistant' | 'system';
    content: string;
    timestamp: number;
}

interface CopilotPanelProps {
    meetingId?: string;
    onClose?: () => void;
}

export function CopilotPanel({ meetingId, onClose }: CopilotPanelProps) {
    const [messages, setMessages] = useState<Message[]>([
        {
            id: 'welcome',
            role: 'assistant',
            content: 'Systems online. I am ready to assist with meeting intelligence. What do you need?',
            timestamp: Date.now()
        }
    ]);
    const [input, setInput] = useState('');
    const [isLoading, setIsLoading] = useState(false);
    const messagesEndRef = useRef<HTMLDivElement>(null);

    const scrollToBottom = () => {
        messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    };

    useEffect(() => {
        scrollToBottom();
    }, [messages]);

    const handleSend = async (text: string = input, presetId: string = 'qa') => {
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
            // If it's a specific preset command (like "summarize"), we might use a different prompt
            // But aiChat takes (preset_id, message, meeting_id).
            // For general chat, we use 'qa' preset. 
            // For buttons, we might use specific presets.

            const response = await aiChat(presetId, text, meetingId);

            const aiMsg: Message = {
                id: (Date.now() + 1).toString(),
                role: 'assistant',
                content: response,
                timestamp: Date.now()
            };
            setMessages(prev => [...prev, aiMsg]);
        } catch (error) {
            console.error('AI Chat Error:', error);
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

    const handlePreset = (preset: 'summarize' | 'action_items') => {
        if (preset === 'summarize') {
            handleSend("Generate a concise summary of the meeting so far.", 'summarize');
        } else if (preset === 'action_items') {
            handleSend("Identify all action items and tasks discussed.", 'action_items');
        }
    };

    return (
        <div className="flex flex-col h-full bg-[#0B0C10] border-l border-[#1F2937] w-80 animate-slide-in-right">
            {/* Header */}
            <div className="flex items-center justify-between p-4 border-b border-[#1F2937] bg-[#111318]">
                <div className="flex items-center space-x-2">
                    <Sparkles className="w-4 h-4 text-emerald-500" />
                    <h3 className="text-sm font-semibold tracking-wider uppercase text-gray-200">
                        AI Co-Pilot
                    </h3>
                </div>
                {onClose && (
                    <button onClick={onClose} className="text-gray-500 hover:text-white transition-colors">
                        <X className="w-4 h-4" />
                    </button>
                )}
            </div>

            {/* Quick Actions */}
            <div className="p-3 grid grid-cols-2 gap-2 border-b border-[#1F2937] bg-[#0B0C10]">
                <button
                    onClick={() => handlePreset('summarize')}
                    disabled={isLoading}
                    className="flex items-center justify-center space-x-1.5 px-3 py-2 bg-[#1F2937]/50 hover:bg-[#1F2937] border border-[#374151] rounded text-xs text-gray-300 transition-all hover:text-white hover:border-emerald-500/50"
                >
                    <FileText className="w-3 h-3" />
                    <span>Summarize</span>
                </button>
                <button
                    onClick={() => handlePreset('action_items')}
                    disabled={isLoading}
                    className="flex items-center justify-center space-x-1.5 px-3 py-2 bg-[#1F2937]/50 hover:bg-[#1F2937] border border-[#374151] rounded text-xs text-gray-300 transition-all hover:text-white hover:border-emerald-500/50"
                >
                    <CheckSquare className="w-3 h-3" />
                    <span>Tasks</span>
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
                            <div className={`w-6 h-6 rounded flex items-center justify-center flex-shrink-0 mt-0.5 ${msg.role === 'user' ? 'bg-[#374151]' : msg.role === 'system' ? 'bg-red-900/20 text-red-500' : 'bg-emerald-900/20 text-emerald-500'
                                }`}>
                                {msg.role === 'user' ? <User className="w-3.5 h-3.5" /> : msg.role === 'system' ? <X className="w-3.5 h-3.5" /> : <Bot className="w-3.5 h-3.5" />}
                            </div>

                            <div className={`p-2.5 rounded text-xs leading-relaxed border ${msg.role === 'user'
                                ? 'bg-[#1F2937] border-[#374151] text-gray-100'
                                : msg.role === 'system'
                                    ? 'bg-red-950/10 border-red-900/30 text-red-400'
                                    : 'bg-[#111318] border-[#1F2937] text-gray-300'
                                }`}>
                                <div className="whitespace-pre-wrap">{msg.content}</div>
                            </div>
                        </div>
                        <span className="text-[10px] text-gray-600 mt-1 px-1">
                            {new Date(msg.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                        </span>
                    </div>
                ))}

                {isLoading && (
                    <div className="flex items-start space-x-2">
                        <div className="w-6 h-6 rounded bg-emerald-900/20 text-emerald-500 flex items-center justify-center">
                            <Bot className="w-3.5 h-3.5" />
                        </div>
                        <div className="flex items-center space-x-1 p-2">
                            <div className="w-1.5 h-1.5 bg-emerald-500/50 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                            <div className="w-1.5 h-1.5 bg-emerald-500/50 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                            <div className="w-1.5 h-1.5 bg-emerald-500/50 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
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
                        placeholder="Ask Co-pilot..."
                        className="w-full bg-[#0B0C10] border border-[#374151] rounded py-2 pl-3 pr-10 text-xs text-white placeholder-gray-600 focus:outline-none focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/20 transition-all"
                        disabled={isLoading}
                    />
                    <button
                        type="submit"
                        disabled={!input.trim() || isLoading}
                        className="absolute right-1.5 top-1.5 p-1 text-gray-500 hover:text-emerald-500 disabled:opacity-50 disabled:hover:text-gray-500 transition-colors"
                    >
                        {isLoading ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Send className="w-3.5 h-3.5" />}
                    </button>
                </form>
                <div className="mt-2 flex justify-between items-center text-[10px] text-gray-600 px-1">
                    <span className="flex items-center">
                        <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 mr-1.5"></span>
                        OLLAMA ON
                    </span>
                    <span>QWEN 2.5VL</span>
                </div>
            </div>
        </div>
    );
}
