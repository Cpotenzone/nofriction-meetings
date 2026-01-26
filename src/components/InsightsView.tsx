// noFriction Meetings - Insights View Component
// Shows activity stats, category breakdown, and VLM-generated insights

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface ActivityStats {
    total_activities: number;
    by_category: Record<string, number>;
    by_app: Record<string, number>;
    avg_confidence: number;
}

interface Activity {
    id: number;
    start_time: string;
    end_time: string | null;
    duration_seconds: number | null;
    app_name: string | null;
    window_title: string | null;
    category: string | null;
    summary: string;
    focus_area: string | null;
    confidence: number | null;
}

export function InsightsView() {
    const [stats, setStats] = useState<ActivityStats | null>(null);
    const [recentActivities, setRecentActivities] = useState<Activity[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        loadData();
    }, []);

    const loadData = async () => {
        setIsLoading(true);
        setError(null);

        try {
            // Load activity stats
            const activityStats = await invoke<ActivityStats>("get_activity_stats");
            setStats(activityStats);

            // Load recent activities
            const activities = await invoke<Activity[]>("get_local_activities", { limit: 20 });
            setRecentActivities(activities);
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

    if (!stats || stats.total_activities === 0) {
        return (
            <div className="insights-view">
                <div className="empty-state">
                    <div className="empty-state-icon">💡</div>
                    <p className="empty-state-text">No activity data yet</p>
                    <p className="empty-state-hint">
                        Enable VLM processing in settings to see insights
                    </p>
                </div>
            </div>
        );
    }

    // Calculate total time by category
    const categoryData = Object.entries(stats.by_category || {});
    const appData = Object.entries(stats.by_app || {}).slice(0, 10); // Top 10 apps

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
                    <div className="stat-value">{stats.total_activities}</div>
                    <div className="stat-label">Total Activities</div>
                </div>

                <div className="stat-card glass-panel">
                    <div className="stat-icon">🎯</div>
                    <div className="stat-value">{Math.round(stats.avg_confidence * 100)}%</div>
                    <div className="stat-label">Avg Confidence</div>
                </div>

                <div className="stat-card glass-panel">
                    <div className="stat-icon">📁</div>
                    <div className="stat-value">{Object.keys(stats.by_category || {}).length}</div>
                    <div className="stat-label">Categories</div>
                </div>

                <div className="stat-card glass-panel">
                    <div className="stat-icon">📱</div>
                    <div className="stat-value">{Object.keys(stats.by_app || {}).length}</div>
                    <div className="stat-label">Applications</div>
                </div>
            </div>

            {/* Category Breakdown */}
            <div className="insights-section glass-panel">
                <h3>📊 Time by Category</h3>
                <div className="category-list">
                    {categoryData.length > 0 ? (
                        categoryData.map(([category, count]) => (
                            <div key={category} className="category-item">
                                <div className="category-header">
                                    <span className="category-name">{category || "Uncategorized"}</span>
                                    <span className="category-count">{count} activities</span>
                                </div>
                                <div className="category-bar">
                                    <div
                                        className="category-fill"
                                        style={{
                                            width: `${(count / stats.total_activities) * 100}%`,
                                            background: getCategoryColor(category),
                                        }}
                                    />
                                </div>
                            </div>
                        ))
                    ) : (
                        <p className="no-data">No category data available</p>
                    )}
                </div>
            </div>

            {/* Top Applications */}
            <div className="insights-section glass-panel">
                <h3>📱 Top Applications</h3>
                <div className="app-list">
                    {appData.length > 0 ? (
                        appData.map(([app, count]) => (
                            <div key={app} className="app-item">
                                <div className="app-header">
                                    <span className="app-name">{app || "Unknown"}</span>
                                    <span className="app-count">{count}</span>
                                </div>
                                <div className="app-bar">
                                    <div
                                        className="app-fill"
                                        style={{
                                            width: `${(count / stats.total_activities) * 100}%`,
                                        }}
                                    />
                                </div>
                            </div>
                        ))
                    ) : (
                        <p className="no-data">No application data available</p>
                    )}
                </div>
            </div>

            {/* Recent Activities */}
            <div className="insights-section glass-panel">
                <h3>🕒 Recent Activities</h3>
                <div className="activity-list scrollable" style={{ maxHeight: "400px" }}>
                    {recentActivities.length > 0 ? (
                        recentActivities.map((activity) => (
                            <div key={activity.id} className="activity-item">
                                <div className="activity-header">
                                    <span className="activity-app">{activity.app_name || "Unknown App"}</span>
                                    <span className="activity-time">
                                        {new Date(activity.start_time).toLocaleTimeString()}
                                    </span>
                                </div>
                                {activity.window_title && (
                                    <div className="activity-window">{activity.window_title}</div>
                                )}
                                <div className="activity-summary">{activity.summary}</div>
                                <div className="activity-meta">
                                    {activity.category && (
                                        <span
                                            className="activity-category"
                                            style={{ background: getCategoryColor(activity.category) }}
                                        >
                                            {activity.category}
                                        </span>
                                    )}
                                    {activity.duration_seconds && (
                                        <span className="activity-duration">
                                            {formatDuration(activity.duration_seconds)}
                                        </span>
                                    )}
                                    {activity.confidence && (
                                        <span className="activity-confidence">
                                            {Math.round(activity.confidence * 100)}% confidence
                                        </span>
                                    )}
                                </div>
                            </div>
                        ))
                    ) : (
                        <p className="no-data">No recent activities</p>
                    )}
                </div>
            </div>
        </div>
    );
}

// Helper function for category colors
function getCategoryColor(category: string | null): string {
    const colors: Record<string, string> = {
        coding: "#10b981",
        communication: "#3b82f6",
        research: "#8b5cf6",
        meeting: "#f59e0b",
        design: "#ec4899",
        writing: "#06b6d4",
        other: "#6b7280",
    };
    return colors[category?.toLowerCase() || "other"] || "#6b7280";
}
