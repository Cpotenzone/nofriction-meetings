// noFriction Meetings - Insights View Component
// Shows meeting stats, duration breakdown, and activity timeline

import { useState, useEffect } from "react";
import * as tauri from "../lib/tauri";
import type { Meeting } from "../lib/tauri";

export function InsightsView() {
    const [meetings, setMeetings] = useState<Meeting[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        loadData();
    }, []);

    const loadData = async () => {
        setIsLoading(true);
        setError(null);

        try {
            const data = await tauri.getMeetings(1000);
            setMeetings(data);
        } catch (err) {
            console.error("Failed to load insights:", err);
            setError(String(err));
        } finally {
            setIsLoading(false);
        }
    };

    const formatDuration = (seconds: number | null) => {
        if (!seconds) return "0m";
        const hours = Math.floor(seconds / 3600);
        const mins = Math.floor((seconds % 3600) / 60);
        if (hours > 0) return `${hours}h ${mins}m`;
        return `${mins}m`;
    };

    if (isLoading) {
        return (
            <div className="insights-view">
                <div className="loading-spinner" />
                <p>Loading insights...</p>
            </div>
        );
    }

    if (error) {
        return (
            <div className="insights-view">
                <div className="error-state">
                    <p>⚠️ {error}</p>
                    <button className="btn btn-primary" onClick={loadData}>
                        Retry
                    </button>
                </div>
            </div>
        );
    }

    if (meetings.length === 0) {
        return (
            <div className="insights-view">
                <div className="empty-state">
                    <div className="empty-state-icon">💡</div>
                    <p className="empty-state-text">No meeting data yet</p>
                    <p className="empty-state-hint">
                        Start recording to see meeting insights
                    </p>
                </div>
            </div>
        );
    }

    // Compute stats from meetings
    const totalDuration = meetings.reduce((sum, m) => sum + (m.duration_seconds || 0), 0);
    const avgDuration = meetings.length > 0 ? totalDuration / meetings.length : 0;
    const meetingsWithDuration = meetings.filter(m => m.duration_seconds && m.duration_seconds > 0);
    const longestMeeting = meetingsWithDuration.length > 0
        ? meetingsWithDuration.reduce((max, m) => (m.duration_seconds || 0) > (max.duration_seconds || 0) ? m : max)
        : null;

    // Group by date
    const byDate: Record<string, Meeting[]> = {};
    for (const m of meetings) {
        const date = new Date(m.started_at).toLocaleDateString("en-US", {
            month: "short",
            day: "numeric",
            year: "numeric",
        });
        if (!byDate[date]) byDate[date] = [];
        byDate[date].push(m);
    }

    // Duration buckets
    const durationBuckets = {
        "< 1 min": 0,
        "1–5 min": 0,
        "5–15 min": 0,
        "15–30 min": 0,
        "30–60 min": 0,
        "1+ hours": 0,
    };
    for (const m of meetings) {
        const secs = m.duration_seconds || 0;
        const mins = secs / 60;
        if (mins < 1) durationBuckets["< 1 min"]++;
        else if (mins < 5) durationBuckets["1–5 min"]++;
        else if (mins < 15) durationBuckets["5–15 min"]++;
        else if (mins < 30) durationBuckets["15–30 min"]++;
        else if (mins < 60) durationBuckets["30–60 min"]++;
        else durationBuckets["1+ hours"]++;
    }

    const bucketColors: Record<string, string> = {
        "< 1 min": "#6b7280",
        "1–5 min": "#3b82f6",
        "5–15 min": "#10b981",
        "15–30 min": "#f59e0b",
        "30–60 min": "#ef4444",
        "1+ hours": "#8b5cf6",
    };

    const dateEntries = Object.entries(byDate);

    return (
        <div className="insights-view">
            {/* Header */}
            <div className="insights-header">
                <h2>💡 Activity Insights</h2>
                <button className="btn btn-ghost" onClick={loadData}>
                    🔄 Refresh
                </button>
            </div>

            {/* Stats Cards */}
            <div className="insights-stats-grid">
                <div className="stat-card glass-panel">
                    <div className="stat-icon">📊</div>
                    <div className="stat-value">{meetings.length}</div>
                    <div className="stat-label">Total Meetings</div>
                </div>

                <div className="stat-card glass-panel">
                    <div className="stat-icon">⏱️</div>
                    <div className="stat-value">{formatDuration(totalDuration)}</div>
                    <div className="stat-label">Total Duration</div>
                </div>

                <div className="stat-card glass-panel">
                    <div className="stat-icon">📁</div>
                    <div className="stat-value">{dateEntries.length}</div>
                    <div className="stat-label">Active Days</div>
                </div>

                <div className="stat-card glass-panel">
                    <div className="stat-icon">📏</div>
                    <div className="stat-value">{formatDuration(Math.round(avgDuration))}</div>
                    <div className="stat-label">Avg Duration</div>
                </div>
            </div>

            {/* Duration Breakdown */}
            <div className="insights-section glass-panel">
                <h3>📊 Duration Breakdown</h3>
                <div className="category-list">
                    {Object.entries(durationBuckets).filter(([, count]) => count > 0).map(([bucket, count]) => (
                        <div key={bucket} className="category-item">
                            <div className="category-header">
                                <span className="category-name">{bucket}</span>
                                <span className="category-count">{count} meeting{count !== 1 ? 's' : ''}</span>
                            </div>
                            <div className="category-bar">
                                <div
                                    className="category-fill"
                                    style={{
                                        width: `${(count / meetings.length) * 100}%`,
                                        background: bucketColors[bucket] || "#6b7280",
                                    }}
                                />
                            </div>
                        </div>
                    ))}
                </div>
            </div>

            {/* Meetings by Day */}
            <div className="insights-section glass-panel">
                <h3>📅 Meetings by Day</h3>
                <div className="category-list">
                    {dateEntries.map(([date, dayMeetings]) => {
                        const dayDuration = dayMeetings.reduce((sum, m) => sum + (m.duration_seconds || 0), 0);
                        return (
                            <div key={date} className="category-item">
                                <div className="category-header">
                                    <span className="category-name">{date}</span>
                                    <span className="category-count">
                                        {dayMeetings.length} meeting{dayMeetings.length !== 1 ? 's' : ''}{dayDuration > 0 ? ` · ${formatDuration(dayDuration)}` : ''}
                                    </span>
                                </div>
                                <div className="category-bar">
                                    <div
                                        className="category-fill"
                                        style={{
                                            width: `${(dayMeetings.length / Math.max(...dateEntries.map(([, m]) => m.length))) * 100}%`,
                                            background: "#FFB800",
                                        }}
                                    />
                                </div>
                            </div>
                        );
                    })}
                </div>
            </div>

            {/* Longest Meeting */}
            {longestMeeting && (
                <div className="insights-section glass-panel">
                    <h3>🏆 Longest Meeting</h3>
                    <div className="activity-item" style={{ cursor: 'default' }}>
                        <div className="activity-header">
                            <span className="activity-app">{longestMeeting.title}</span>
                            <span className="activity-time">
                                {new Date(longestMeeting.started_at).toLocaleDateString()}
                            </span>
                        </div>
                        <div className="activity-meta">
                            <span className="activity-duration">
                                {formatDuration(longestMeeting.duration_seconds)}
                            </span>
                        </div>
                    </div>
                </div>
            )}

            {/* Recent Meetings */}
            <div className="insights-section glass-panel">
                <h3>🕒 Recent Meetings</h3>
                <div className="activity-list scrollable" style={{ maxHeight: "400px" }}>
                    {meetings.slice(0, 20).map((meeting) => (
                        <div key={meeting.id} className="activity-item">
                            <div className="activity-header">
                                <span className="activity-app">{meeting.title}</span>
                                <span className="activity-time">
                                    {new Date(meeting.started_at).toLocaleTimeString("en-US", {
                                        hour: "numeric",
                                        minute: "2-digit",
                                    })}
                                </span>
                            </div>
                            <div className="activity-meta">
                                <span className="activity-category" style={{ background: "#FFB800" }}>
                                    {new Date(meeting.started_at).toLocaleDateString("en-US", {
                                        month: "short",
                                        day: "numeric",
                                    })}
                                </span>
                                {meeting.duration_seconds && (
                                    <span className="activity-duration">
                                        {formatDuration(meeting.duration_seconds)}
                                    </span>
                                )}
                            </div>
                        </div>
                    ))}
                </div>
            </div>
        </div>
    );
}
