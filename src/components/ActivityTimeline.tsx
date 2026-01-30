// noFriction Meetings - Activity Timeline Component
// Displays structured timeline with topics, app switches, and activity events

import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

// Types matching Rust backend
interface TimelineEvent {
    event_id: string;
    meeting_id: string;
    ts: string;
    event_type: string;
    title: string;
    description: string | null;
    app_name: string | null;
    window_title: string | null;
    duration_ms: number | null;
    episode_id: string | null;
    state_id: string | null;
    topic: string | null;
    importance: number;
}

interface TopicCluster {
    topic_id: string;
    meeting_id: string;
    name: string;
    description: string | null;
    start_ts: string;
    end_ts: string | null;
    event_count: number;
    total_duration_ms: number;
}

interface ActivityTimelineProps {
    meetingId: string | null;
    onEventClick?: (event: TimelineEvent) => void;
}

// Topic colors and icons
const TOPIC_STYLES: Record<string, { color: string; icon: string; bg: string }> = {
    "Coding": { color: "#22c55e", icon: "💻", bg: "rgba(34, 197, 94, 0.15)" },
    "Documentation": { color: "#3b82f6", icon: "📄", bg: "rgba(59, 130, 246, 0.15)" },
    "Communication": { color: "#a855f7", icon: "💬", bg: "rgba(168, 85, 247, 0.15)" },
    "Research": { color: "#f59e0b", icon: "🔍", bg: "rgba(245, 158, 11, 0.15)" },
    "Terminal": { color: "#6b7280", icon: "⌨️", bg: "rgba(107, 114, 128, 0.15)" },
    "default": { color: "#94a3b8", icon: "📁", bg: "rgba(148, 163, 184, 0.15)" }
};

// Event type icons
const EVENT_ICONS: Record<string, string> = {
    "document_opened": "📂",
    "document_closed": "📁",
    "app_switch": "🔄",
    "content_edit": "✏️",
    "navigation": "🧭",
    "meeting_start": "🎬",
    "meeting_end": "🏁",
    "topic_change": "🏷️",
    "activity_gap": "☕"
};

export function ActivityTimeline({ meetingId, onEventClick }: ActivityTimelineProps) {
    const [events, setEvents] = useState<TimelineEvent[]>([]);
    const [topics, setTopics] = useState<TopicCluster[]>([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [selectedTopic, setSelectedTopic] = useState<string | null>(null);
    const [expandedEvents, setExpandedEvents] = useState<Set<string>>(new Set());

    // Load timeline data
    const loadTimeline = useCallback(async () => {
        if (!meetingId) {
            setEvents([]);
            setTopics([]);
            return;
        }

        setLoading(true);
        setError(null);

        try {
            const [eventsData, topicsData] = await Promise.all([
                invoke<TimelineEvent[]>("get_timeline_events", { meetingId: meetingId }),
                invoke<TopicCluster[]>("get_topic_clusters", { meetingId: meetingId })
            ]);

            setEvents(eventsData || []);
            setTopics(topicsData || []);
        } catch (err) {
            console.error("Failed to load timeline:", err);
            setError(String(err));
        } finally {
            setLoading(false);
        }
    }, [meetingId]);

    useEffect(() => {
        loadTimeline();
        // Refresh every 10 seconds during active meetings
        const interval = setInterval(loadTimeline, 10000);
        return () => clearInterval(interval);
    }, [loadTimeline]);

    // Format duration
    const formatDuration = (ms: number): string => {
        if (ms < 60000) return `${Math.round(ms / 1000)}s`;
        const minutes = Math.floor(ms / 60000);
        if (minutes < 60) return `${minutes}m`;
        const hours = Math.floor(minutes / 60);
        return `${hours}h ${minutes % 60}m`;
    };

    // Format timestamp
    const formatTime = (ts: string): string => {
        const date = new Date(ts);
        return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    };

    // Get topic style
    const getTopicStyle = (topic: string | null) => {
        return TOPIC_STYLES[topic || "default"] || TOPIC_STYLES["default"];
    };

    // Toggle event expansion
    const toggleEvent = (eventId: string) => {
        setExpandedEvents(prev => {
            const next = new Set(prev);
            if (next.has(eventId)) {
                next.delete(eventId);
            } else {
                next.add(eventId);
            }
            return next;
        });
    };

    // Filter events by topic
    const filteredEvents = selectedTopic
        ? events.filter(e => e.topic === selectedTopic)
        : events;

    // Calculate importance bar width
    const getImportanceWidth = (importance: number) => `${Math.round(importance * 100)}%`;

    if (!meetingId) {
        return (
            <div className="activity-timeline empty-state">
                <div className="empty-icon">📊</div>
                <p>Select a meeting to view activity timeline</p>
            </div>
        );
    }

    if (loading && events.length === 0) {
        return (
            <div className="activity-timeline loading">
                <div className="spinner"></div>
                <p>Loading timeline...</p>
            </div>
        );
    }

    if (error) {
        return (
            <div className="activity-timeline error">
                <p>⚠️ {error}</p>
                <button onClick={loadTimeline}>Retry</button>
            </div>
        );
    }

    return (
        <div className="activity-timeline">
            {/* Header with stats */}
            <div className="timeline-header">
                <h3>📊 Activity Timeline</h3>
                <span className="timeline-stats">
                    {events.length} events • {topics.length} topics
                </span>
            </div>

            {/* Topic Filter Pills */}
            {topics.length > 0 && (
                <div className="topic-filters">
                    <button
                        className={`topic-pill ${!selectedTopic ? 'active' : ''}`}
                        onClick={() => setSelectedTopic(null)}
                    >
                        All
                    </button>
                    {topics.map(topic => {
                        const style = getTopicStyle(topic.name);
                        return (
                            <button
                                key={topic.topic_id}
                                className={`topic-pill ${selectedTopic === topic.name ? 'active' : ''}`}
                                style={{
                                    backgroundColor: selectedTopic === topic.name ? style.bg : undefined,
                                    borderColor: style.color
                                }}
                                onClick={() => setSelectedTopic(topic.name)}
                            >
                                {style.icon} {topic.name}
                                <span className="topic-count">{topic.event_count}</span>
                            </button>
                        );
                    })}
                </div>
            )}

            {/* Topic Summary Cards */}
            {!selectedTopic && topics.length > 0 && (
                <div className="topic-summary">
                    {topics.map(topic => {
                        const style = getTopicStyle(topic.name);
                        return (
                            <div
                                key={topic.topic_id}
                                className="topic-card"
                                style={{ borderLeftColor: style.color, backgroundColor: style.bg }}
                                onClick={() => setSelectedTopic(topic.name)}
                            >
                                <div className="topic-icon">{style.icon}</div>
                                <div className="topic-info">
                                    <div className="topic-name">{topic.name}</div>
                                    <div className="topic-meta">
                                        {formatDuration(topic.total_duration_ms)} • {topic.event_count} events
                                    </div>
                                </div>
                            </div>
                        );
                    })}
                </div>
            )}

            {/* Events List */}
            <div className="events-list">
                {filteredEvents.length === 0 ? (
                    <div className="no-events">
                        <p>No activity recorded yet</p>
                    </div>
                ) : (
                    filteredEvents.map((event, index) => {
                        const style = getTopicStyle(event.topic);
                        const isExpanded = expandedEvents.has(event.event_id);
                        const icon = EVENT_ICONS[event.event_type] || "📌";

                        return (
                            <div
                                key={event.event_id}
                                className={`event-item ${isExpanded ? 'expanded' : ''}`}
                                style={{ borderLeftColor: style.color }}
                            >
                                {/* Timeline connector */}
                                {index < filteredEvents.length - 1 && (
                                    <div className="timeline-connector" style={{ backgroundColor: style.color }}></div>
                                )}

                                {/* Event dot */}
                                <div className="event-dot" style={{ backgroundColor: style.color }}>
                                    <span>{icon}</span>
                                </div>

                                {/* Event content */}
                                <div
                                    className="event-content"
                                    onClick={() => toggleEvent(event.event_id)}
                                >
                                    <div className="event-header">
                                        <span className="event-time">{formatTime(event.ts)}</span>
                                        <span className="event-title">{event.title}</span>
                                        {event.topic && (
                                            <span
                                                className="event-topic"
                                                style={{ backgroundColor: style.bg, color: style.color }}
                                            >
                                                {event.topic}
                                            </span>
                                        )}
                                    </div>

                                    {/* Importance bar */}
                                    <div className="importance-bar">
                                        <div
                                            className="importance-fill"
                                            style={{
                                                width: getImportanceWidth(event.importance),
                                                backgroundColor: style.color
                                            }}
                                        ></div>
                                    </div>

                                    {/* Expanded details */}
                                    {isExpanded && (
                                        <div className="event-details">
                                            {event.app_name && (
                                                <div className="detail-row">
                                                    <span className="label">App:</span>
                                                    <span className="value">{event.app_name}</span>
                                                </div>
                                            )}
                                            {event.window_title && (
                                                <div className="detail-row">
                                                    <span className="label">Window:</span>
                                                    <span className="value">{event.window_title}</span>
                                                </div>
                                            )}
                                            {event.duration_ms && (
                                                <div className="detail-row">
                                                    <span className="label">Duration:</span>
                                                    <span className="value">{formatDuration(event.duration_ms)}</span>
                                                </div>
                                            )}
                                            {event.description && (
                                                <div className="detail-row description">
                                                    <span className="value">{event.description}</span>
                                                </div>
                                            )}
                                            {event.episode_id && onEventClick && (
                                                <button
                                                    className="jump-to-btn"
                                                    onClick={(e) => {
                                                        e.stopPropagation();
                                                        onEventClick(event);
                                                    }}
                                                >
                                                    🎯 Jump to Evidence
                                                </button>
                                            )}
                                        </div>
                                    )}
                                </div>
                            </div>
                        );
                    })
                )}
            </div>

            {/* CSS Styles (inlined for component portability) */}
            <style>{`
                .activity-timeline {
                    display: flex;
                    flex-direction: column;
                    gap: 16px;
                    padding: 16px;
                    background: var(--bg-secondary, #1a1a2e);
                    border-radius: 12px;
                    max-height: 600px;
                    overflow-y: auto;
                }
                
                .activity-timeline.empty-state,
                .activity-timeline.loading,
                .activity-timeline.error {
                    display: flex;
                    flex-direction: column;
                    align-items: center;
                    justify-content: center;
                    min-height: 200px;
                    color: var(--text-muted, #888);
                }
                
                .empty-icon {
                    font-size: 48px;
                    margin-bottom: 12px;
                }
                
                .timeline-header {
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                }
                
                .timeline-header h3 {
                    margin: 0;
                    font-size: 18px;
                    color: var(--text-primary, #fff);
                }
                
                .timeline-stats {
                    font-size: 12px;
                    color: var(--text-muted, #888);
                }
                
                .topic-filters {
                    display: flex;
                    gap: 8px;
                    flex-wrap: wrap;
                }
                
                .topic-pill {
                    display: flex;
                    align-items: center;
                    gap: 6px;
                    padding: 6px 12px;
                    border-radius: 16px;
                    border: 1px solid var(--border-color, #333);
                    background: transparent;
                    color: var(--text-secondary, #ccc);
                    font-size: 12px;
                    cursor: pointer;
                    transition: all 0.2s;
                }
                
                .topic-pill:hover {
                    background: var(--bg-hover, #2a2a4e);
                }
                
                .topic-pill.active {
                    background: var(--bg-active, #3a3a5e);
                    color: var(--text-primary, #fff);
                }
                
                .topic-count {
                    background: rgba(255,255,255,0.1);
                    padding: 2px 6px;
                    border-radius: 8px;
                    font-size: 10px;
                }
                
                .topic-summary {
                    display: grid;
                    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
                    gap: 12px;
                }
                
                .topic-card {
                    display: flex;
                    align-items: center;
                    gap: 10px;
                    padding: 12px;
                    border-radius: 8px;
                    border-left: 3px solid;
                    cursor: pointer;
                    transition: transform 0.2s;
                }
                
                .topic-card:hover {
                    transform: translateY(-2px);
                }
                
                .topic-icon {
                    font-size: 24px;
                }
                
                .topic-name {
                    font-weight: 600;
                    font-size: 14px;
                    color: var(--text-primary, #fff);
                }
                
                .topic-meta {
                    font-size: 11px;
                    color: var(--text-muted, #888);
                }
                
                .events-list {
                    display: flex;
                    flex-direction: column;
                    gap: 0;
                }
                
                .event-item {
                    position: relative;
                    display: flex;
                    gap: 12px;
                    padding: 12px 0;
                    border-left: 2px solid var(--border-color, #333);
                    margin-left: 8px;
                    padding-left: 20px;
                }
                
                .timeline-connector {
                    position: absolute;
                    left: -2px;
                    top: 28px;
                    bottom: 0;
                    width: 2px;
                    opacity: 0.3;
                }
                
                .event-dot {
                    position: absolute;
                    left: -10px;
                    top: 12px;
                    width: 18px;
                    height: 18px;
                    border-radius: 50%;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    font-size: 10px;
                }
                
                .event-content {
                    flex: 1;
                    cursor: pointer;
                }
                
                .event-header {
                    display: flex;
                    align-items: center;
                    gap: 8px;
                    flex-wrap: wrap;
                }
                
                .event-time {
                    font-size: 12px;
                    color: var(--text-muted, #888);
                    min-width: 50px;
                }
                
                .event-title {
                    font-size: 14px;
                    color: var(--text-primary, #fff);
                    flex: 1;
                }
                
                .event-topic {
                    font-size: 10px;
                    padding: 2px 8px;
                    border-radius: 10px;
                }
                
                .importance-bar {
                    height: 3px;
                    background: rgba(255,255,255,0.1);
                    border-radius: 2px;
                    margin-top: 6px;
                    overflow: hidden;
                }
                
                .importance-fill {
                    height: 100%;
                    border-radius: 2px;
                    transition: width 0.3s;
                }
                
                .event-details {
                    margin-top: 12px;
                    padding: 12px;
                    background: rgba(0,0,0,0.2);
                    border-radius: 8px;
                    display: flex;
                    flex-direction: column;
                    gap: 8px;
                }
                
                .detail-row {
                    display: flex;
                    gap: 8px;
                    font-size: 12px;
                }
                
                .detail-row .label {
                    color: var(--text-muted, #888);
                    min-width: 60px;
                }
                
                .detail-row .value {
                    color: var(--text-secondary, #ccc);
                }
                
                .detail-row.description .value {
                    font-style: italic;
                }
                
                .jump-to-btn {
                    margin-top: 8px;
                    padding: 8px 16px;
                    background: var(--accent-color, #6366f1);
                    border: none;
                    border-radius: 6px;
                    color: white;
                    font-size: 12px;
                    cursor: pointer;
                    width: fit-content;
                }
                
                .jump-to-btn:hover {
                    filter: brightness(1.1);
                }
                
                .no-events {
                    text-align: center;
                    padding: 40px;
                    color: var(--text-muted, #888);
                }
                
                .spinner {
                    width: 32px;
                    height: 32px;
                    border: 3px solid var(--border-color, #333);
                    border-top-color: var(--accent-color, #6366f1);
                    border-radius: 50%;
                    animation: spin 1s linear infinite;
                }
                
                @keyframes spin {
                    to { transform: rotate(360deg); }
                }
            `}</style>
        </div>
    );
}

export default ActivityTimeline;
