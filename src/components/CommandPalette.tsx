// noFriction Meetings - Command Palette
// macOS-native ⌘K command palette with fuzzy search

import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";

// Command types
interface Command {
    id: string;
    label: string;
    shortcut?: string;
    category: "action" | "navigation" | "meeting" | "ai";
    icon?: string;
    action: () => void | Promise<void>;
    keywords?: string[];
}

interface Meeting {
    id: string;
    title: string;
    start_time: string;
    duration_seconds: number;
}

interface CommandPaletteProps {
    isOpen: boolean;
    onClose: () => void;
    onNavigate?: (tab: string) => void;
    onStartRecording?: () => void;
    onStopRecording?: () => void;
    isRecording?: boolean;
    currentMeetingId?: string | null;
}

export function CommandPalette({
    isOpen,
    onClose,
    onNavigate,
    onStartRecording,
    onStopRecording,
    isRecording,
    currentMeetingId,
}: CommandPaletteProps) {
    const [query, setQuery] = useState("");
    const [selectedIndex, setSelectedIndex] = useState(0);
    const [recentMeetings, setRecentMeetings] = useState<Meeting[]>([]);
    const inputRef = useRef<HTMLInputElement>(null);
    const listRef = useRef<HTMLDivElement>(null);

    // Load recent meetings on open
    useEffect(() => {
        if (isOpen) {
            setQuery("");
            setSelectedIndex(0);
            inputRef.current?.focus();
            loadRecentMeetings();
        }
    }, [isOpen]);

    const loadRecentMeetings = async () => {
        try {
            const meetings = await invoke<Meeting[]>("list_meetings", { limit: 5 });
            setRecentMeetings(meetings);
        } catch (err) {
            console.error("Failed to load meetings:", err);
        }
    };

    // Define all commands
    const commands = useMemo<Command[]>(() => {
        const baseCommands: Command[] = [
            // Actions
            {
                id: "record",
                label: isRecording ? "Stop Recording" : "Start Recording",
                shortcut: "⌘R",
                category: "action",
                icon: isRecording ? "⏹" : "⏺",
                action: () => {
                    if (isRecording) {
                        onStopRecording?.();
                    } else {
                        onStartRecording?.();
                    }
                    onClose();
                },
                keywords: ["record", "start", "stop", "capture", "mic"],
            },
            {
                id: "summarize",
                label: "Summarize Meeting",
                shortcut: "⌘⇧S",
                category: "ai",
                icon: "📝",
                action: async () => {
                    if (currentMeetingId) {
                        // Trigger AI summary
                        onNavigate?.("ai");
                    }
                    onClose();
                },
                keywords: ["summary", "ai", "analyze", "overview"],
            },
            {
                id: "action-items",
                label: "Extract Action Items",
                shortcut: "⌘⇧A",
                category: "ai",
                icon: "✅",
                action: () => {
                    onNavigate?.("ai");
                    onClose();
                },
                keywords: ["action", "items", "tasks", "todo", "extract"],
            },
            {
                id: "export",
                label: "Export Meeting",
                shortcut: "⌘⇧E",
                category: "action",
                icon: "📤",
                action: async () => {
                    try {
                        await invoke("export_data");
                        onClose();
                    } catch (err) {
                        console.error("Export failed:", err);
                    }
                },
                keywords: ["export", "save", "download", "share"],
            },

            // Navigation
            {
                id: "nav-live",
                label: "Go to Live View",
                shortcut: "⌘1",
                category: "navigation",
                icon: "📝",
                action: () => {
                    onNavigate?.("live");
                    onClose();
                },
                keywords: ["live", "transcript", "current"],
            },
            {
                id: "nav-rewind",
                label: "Go to Rewind",
                shortcut: "⌘2",
                category: "navigation",
                icon: "🎬",
                action: () => {
                    onNavigate?.("rewind");
                    onClose();
                },
                keywords: ["rewind", "timeline", "playback", "history"],
            },
            {
                id: "nav-settings",
                label: "Open Settings",
                shortcut: "⌘,",
                category: "navigation",
                icon: "⚙️",
                action: () => {
                    onNavigate?.("settings");
                    onClose();
                },
                keywords: ["settings", "preferences", "config", "options"],
            },
            {
                id: "nav-prompts",
                label: "Open Prompt Library",
                shortcut: "⌘⇧P",
                category: "navigation",
                icon: "🎯",
                action: () => {
                    onNavigate?.("prompts");
                    onClose();
                },
                keywords: ["prompts", "ai", "library", "templates"],
            },

            // AI Actions
            {
                id: "ask-ai",
                label: "Ask AI a Question",
                category: "ai",
                icon: "🤖",
                action: () => {
                    onNavigate?.("ai");
                    onClose();
                },
                keywords: ["ask", "question", "ai", "chat", "query"],
            },
            {
                id: "analyze-frames",
                label: "Analyze Screen Captures",
                category: "ai",
                icon: "🖼️",
                action: async () => {
                    try {
                        await invoke("analyze_pending_frames", { limit: 10 });
                        onClose();
                    } catch (err) {
                        console.error("Analysis failed:", err);
                    }
                },
                keywords: ["analyze", "frames", "screenshots", "vlm", "vision"],
            },

            // Utility
            {
                id: "clear-cache",
                label: "Clear Cache",
                category: "action",
                icon: "🗑️",
                action: async () => {
                    try {
                        await invoke("clear_cache");
                        onClose();
                    } catch (err) {
                        console.error("Clear failed:", err);
                    }
                },
                keywords: ["clear", "cache", "clean", "reset"],
            },
            {
                id: "refresh-models",
                label: "Refresh AI Models",
                category: "action",
                icon: "🔄",
                action: async () => {
                    try {
                        await invoke("refresh_model_availability");
                        onClose();
                    } catch (err) {
                        console.error("Refresh failed:", err);
                    }
                },
                keywords: ["refresh", "models", "ollama", "ai"],
            },
        ];

        // Add meeting commands
        const meetingCommands: Command[] = recentMeetings.map((meeting) => ({
            id: `meeting-${meeting.id}`,
            label: meeting.title || "Untitled Meeting",
            category: "meeting" as const,
            icon: "📅",
            action: () => {
                // Navigate to meeting
                onNavigate?.("rewind");
                onClose();
            },
            keywords: [meeting.title?.toLowerCase() || "", formatTimeAgo(meeting.start_time)],
        }));

        return [...baseCommands, ...meetingCommands];
    }, [isRecording, currentMeetingId, recentMeetings, onNavigate, onStartRecording, onStopRecording, onClose]);

    // Fuzzy search filter
    const filteredCommands = useMemo(() => {
        if (!query.trim()) {
            // Show categorized when no query
            return commands;
        }

        const lowerQuery = query.toLowerCase();
        return commands
            .filter((cmd) => {
                const labelMatch = cmd.label.toLowerCase().includes(lowerQuery);
                const keywordMatch = cmd.keywords?.some((kw) => kw.includes(lowerQuery));
                return labelMatch || keywordMatch;
            })
            .slice(0, 10);
    }, [commands, query]);

    // Keyboard navigation
    const handleKeyDown = useCallback(
        (e: React.KeyboardEvent) => {
            switch (e.key) {
                case "ArrowDown":
                    e.preventDefault();
                    setSelectedIndex((i) => Math.min(i + 1, filteredCommands.length - 1));
                    break;
                case "ArrowUp":
                    e.preventDefault();
                    setSelectedIndex((i) => Math.max(i - 1, 0));
                    break;
                case "Enter":
                    e.preventDefault();
                    if (filteredCommands[selectedIndex]) {
                        filteredCommands[selectedIndex].action();
                    }
                    break;
                case "Escape":
                    e.preventDefault();
                    onClose();
                    break;
            }
        },
        [filteredCommands, selectedIndex, onClose]
    );

    // Scroll selected into view
    useEffect(() => {
        const listEl = listRef.current;
        if (listEl) {
            const selected = listEl.querySelector(`[data-index="${selectedIndex}"]`);
            if (selected) {
                selected.scrollIntoView({ block: "nearest" });
            }
        }
    }, [selectedIndex]);

    // Reset selection when query changes
    useEffect(() => {
        setSelectedIndex(0);
    }, [query]);

    if (!isOpen) return null;

    // Group commands by category
    const groupedCommands = filteredCommands.reduce(
        (acc, cmd) => {
            if (!acc[cmd.category]) acc[cmd.category] = [];
            acc[cmd.category].push(cmd);
            return acc;
        },
        {} as Record<string, Command[]>
    );

    const categoryLabels: Record<string, string> = {
        action: "Actions",
        navigation: "Navigation",
        meeting: "Recent Meetings",
        ai: "AI",
    };

    let flatIndex = 0;

    return (
        <div className="command-palette-overlay" onClick={onClose}>
            <div
                className="command-palette"
                onClick={(e) => e.stopPropagation()}
                onKeyDown={handleKeyDown}
            >
                {/* Search Input */}
                <div className="command-palette-input-wrapper">
                    <span className="command-palette-icon">⌘</span>
                    <input
                        ref={inputRef}
                        type="text"
                        className="command-palette-input"
                        placeholder="Type a command or search..."
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                        autoFocus
                    />
                    <kbd className="command-palette-hint">ESC</kbd>
                </div>

                {/* Command List */}
                <div className="command-palette-list" ref={listRef}>
                    {Object.entries(groupedCommands).map(([category, cmds]) => (
                        <div key={category} className="command-palette-group">
                            <div className="command-palette-group-label">
                                {categoryLabels[category] || category}
                            </div>
                            {cmds.map((cmd) => {
                                const index = flatIndex++;
                                return (
                                    <div
                                        key={cmd.id}
                                        data-index={index}
                                        className={`command-palette-item ${index === selectedIndex ? "selected" : ""}`}
                                        onClick={() => cmd.action()}
                                        onMouseEnter={() => setSelectedIndex(index)}
                                    >
                                        <span className="command-palette-item-icon">{cmd.icon}</span>
                                        <span className="command-palette-item-label">{cmd.label}</span>
                                        {cmd.shortcut && (
                                            <kbd className="command-palette-item-shortcut">{cmd.shortcut}</kbd>
                                        )}
                                    </div>
                                );
                            })}
                        </div>
                    ))}

                    {filteredCommands.length === 0 && (
                        <div className="command-palette-empty">
                            No commands found for "{query}"
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}

// Helper function
function formatTimeAgo(dateString: string): string {
    const date = new Date(dateString);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays === 1) return "Yesterday";
    if (diffDays < 7) return `${diffDays}d ago`;
    return date.toLocaleDateString();
}

// Global keyboard hook for ⌘K
export function useCommandPalette() {
    const [isOpen, setIsOpen] = useState(false);

    useEffect(() => {
        const handleKeyDown = (e: KeyboardEvent) => {
            // ⌘K to open
            if ((e.metaKey || e.ctrlKey) && e.key === "k") {
                e.preventDefault();
                setIsOpen(true);
            }
        };

        window.addEventListener("keydown", handleKeyDown);
        return () => window.removeEventListener("keydown", handleKeyDown);
    }, []);

    return {
        isOpen,
        open: () => setIsOpen(true),
        close: () => setIsOpen(false),
    };
}
