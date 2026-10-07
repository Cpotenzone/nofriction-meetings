// Management Suite - Tools Console
// Job history, database stats, and queue control

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

// =============================================================================
// Types
// =============================================================================

interface JobEntry {
    id: string;
    job_type: string;
    status: string;
    started_at: string;
    completed_at: string | null;
    duration_ms: number | null;
    details: string | null;
}

interface DatabaseStats {
    meetings: number;
    frames: number;
    transcripts: number;
    entities: number;
    frame_queue: number;
    audit_log: number;
}

// =============================================================================
// ToolsConsole Component
// =============================================================================

export function ToolsConsole() {
    const [jobs, setJobs] = useState<JobEntry[]>([]);
    const [dbStats, setDbStats] = useState<DatabaseStats | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    const loadData = useCallback(async () => {
        setIsLoading(true);
        setError(null);
        try {
            const [jobHistory, stats] = await Promise.all([
                invoke<JobEntry[]>('get_job_history', { limit: 50 }),
                invoke<DatabaseStats>('get_database_stats')
            ]);
            setJobs(jobHistory);
            setDbStats(stats);
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            setIsLoading(false);
        }
    }, []);

    useEffect(() => {
        loadData();
        const interval = setInterval(loadData, 15000); // Refresh every 15s
        return () => clearInterval(interval);
    }, [loadData]);

    const formatDate = (dateStr: string) => {
        const date = new Date(dateStr);
        return date.toLocaleString('en-US', {
            month: 'short',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit'
        });
    };

    const formatDuration = (ms: number | null) => {
        if (ms === null) return '--';
        if (ms < 1000) return `${ms}ms`;
        return `${(ms / 1000).toFixed(1)}s`;
    };

    const getStatusIcon = (status: string) => {
        switch (status.toLowerCase()) {
            case 'completed': return '✅';
            case 'processing': return '⏳';
            case 'failed': return '❌';
            case 'pending': return '⏸️';
            default: return '❓';
        }
    };

    if (isLoading && jobs.length === 0) {
        return (
            <div className="loading-state">
                <div className="loading-spinner" />
                <p>Loading tools console...</p>
            </div>
        );
    }

    return (
        <div className="tools-console">
            <div className="tools-header">
                <h3>🛠️ Tools Console</h3>
                <button className="btn btn-icon" onClick={loadData} title="Refresh">
                    🔄
                </button>
            </div>

            {error && (
                <div className="error-banner">
                    ⚠️ {error}
                    <button onClick={() => setError(null)}>×</button>
                </div>
            )}

            {/* Database Stats */}
            {dbStats && (
                <div className="db-stats-section">
                    <h4>📊 Database Statistics</h4>
                    <div className="db-stats-grid">
                        <div className="db-stat">
                            <div className="stat-value">{dbStats.meetings.toLocaleString()}</div>
                            <div className="stat-label">Recordings</div>
                        </div>
                        <div className="db-stat">
                            <div className="stat-value">{dbStats.frames.toLocaleString()}</div>
                            <div className="stat-label">Captures</div>
                        </div>
                        <div className="db-stat">
                            <div className="stat-value">{dbStats.transcripts.toLocaleString()}</div>
                            <div className="stat-label">Transcripts</div>
                        </div>
                        <div className="db-stat">
                            <div className="stat-value">{dbStats.entities.toLocaleString()}</div>
                            <div className="stat-label">Entities</div>
                        </div>
                        <div className="db-stat">
                            <div className="stat-value">{dbStats.frame_queue.toLocaleString()}</div>
                            <div className="stat-label">Queue Items</div>
                        </div>
                        <div className="db-stat">
                            <div className="stat-value">{dbStats.audit_log.toLocaleString()}</div>
                            <div className="stat-label">Audit Entries</div>
                        </div>
                    </div>
                </div>
            )}

            {/* Job History */}
            <div className="job-history-section">
                <h4>📋 Recent Jobs</h4>
                {jobs.length === 0 ? (
                    <div className="empty-state">
                        <div className="empty-icon">📭</div>
                        <h3>No Job History</h3>
                        <p>No jobs have been processed yet</p>
                    </div>
                ) : (
                    <div className="job-table-container">
                        <table className="job-table">
                            <thead>
                                <tr>
                                    <th>Status</th>
                                    <th>Job Type</th>
                                    <th>Started</th>
                                    <th>Duration</th>
                                    <th>Details</th>
                                </tr>
                            </thead>
                            <tbody>
                                {jobs.map(job => (
                                    <tr key={job.id} className={`job-row status-${job.status.toLowerCase()}`}>
                                        <td className="col-status">
                                            <span className="status-icon">{getStatusIcon(job.status)}</span>
                                            <span className="status-text">{job.status}</span>
                                        </td>
                                        <td className="col-type">{job.job_type}</td>
                                        <td className="col-time">{formatDate(job.started_at)}</td>
                                        <td className="col-duration">{formatDuration(job.duration_ms)}</td>
                                        <td className="col-details">
                                            <span className="details-text" title={job.details || undefined}>
                                                {job.details || '--'}
                                            </span>
                                        </td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                )}
            </div>
        </div>
    );
}

export default ToolsConsole;
