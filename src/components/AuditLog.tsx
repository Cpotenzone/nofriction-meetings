// Management Suite - Audit Log Viewer
// Displays admin action history with filtering

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

// =============================================================================
// Types
// =============================================================================

interface AuditLogEntry {
    id: string;
    action: string;
    target_type: string;
    target_id: string;
    timestamp: string;
    details: string | null;
    bytes_affected: number;
}

// =============================================================================
// AuditLog Component
// =============================================================================

export function AuditLog() {
    const [entries, setEntries] = useState<AuditLogEntry[]>([]);
    const [totalCount, setTotalCount] = useState(0);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [page, setPage] = useState(0);
    const [filter, setFilter] = useState<string | null>(null);

    const PAGE_SIZE = 50;

    // Load audit log entries
    const loadEntries = useCallback(async () => {
        setIsLoading(true);
        setError(null);
        try {
            const [logEntries, count] = await Promise.all([
                invoke<AuditLogEntry[]>('get_audit_log', {
                    limit: PAGE_SIZE,
                    offset: page * PAGE_SIZE,
                    actionFilter: filter
                }),
                invoke<number>('get_audit_log_count', {
                    actionFilter: filter
                })
            ]);
            setEntries(logEntries);
            setTotalCount(count);
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            setIsLoading(false);
        }
    }, [page, filter]);

    useEffect(() => {
        loadEntries();
    }, [loadEntries]);

    // Format timestamp
    const formatTimestamp = (ts: string) => {
        const date = new Date(ts);
        return date.toLocaleString('en-US', {
            month: 'short',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit'
        });
    };

    // Format bytes
    const formatBytes = (bytes: number) => {
        if (bytes === 0) return '--';
        const units = ['B', 'KB', 'MB', 'GB'];
        let size = bytes;
        let unitIdx = 0;
        while (size >= 1024 && unitIdx < units.length - 1) {
            size /= 1024;
            unitIdx++;
        }
        return `${size.toFixed(1)} ${units[unitIdx]}`;
    };

    // Action icon mapping
    const getActionIcon = (action: string) => {
        switch (action) {
            case 'delete_recording': return '🗑️';
            case 'toggle_flag': return '🔀';
            case 'export_data': return '📤';
            case 'clear_cache': return '🧹';
            default: return '📝';
        }
    };

    // Action color mapping
    const getActionClass = (action: string) => {
        if (action.includes('delete')) return 'action-danger';
        if (action.includes('toggle')) return 'action-warning';
        return 'action-info';
    };

    // Unique actions for filter dropdown
    const uniqueActions = [...new Set(entries.map(e => e.action))];

    const totalPages = Math.ceil(totalCount / PAGE_SIZE);

    return (
        <div className="audit-log">
            <div className="audit-header">
                <h3>📋 Admin Activity Log</h3>
                <p className="audit-subtitle">
                    All administrative actions are recorded for accountability
                </p>
            </div>

            {/* Toolbar */}
            <div className="audit-toolbar">
                <div className="toolbar-left">
                    <select
                        className="filter-select"
                        value={filter || ''}
                        onChange={(e) => {
                            setFilter(e.target.value || null);
                            setPage(0);
                        }}
                    >
                        <option value="">All Actions</option>
                        {uniqueActions.map(action => (
                            <option key={action} value={action}>{action}</option>
                        ))}
                    </select>
                    <button className="btn btn-icon" onClick={loadEntries} title="Refresh">
                        🔄
                    </button>
                </div>
                <div className="toolbar-right">
                    <span className="entry-count">
                        {totalCount} {totalCount === 1 ? 'entry' : 'entries'}
                    </span>
                </div>
            </div>

            {/* Error display */}
            {error && (
                <div className="error-banner">
                    ⚠️ {error}
                    <button onClick={() => setError(null)}>×</button>
                </div>
            )}

            {/* Entries list */}
            <div className="audit-entries">
                {isLoading ? (
                    <div className="loading-state">
                        <div className="loading-spinner" />
                        <p>Loading audit log...</p>
                    </div>
                ) : entries.length === 0 ? (
                    <div className="empty-state">
                        <div className="empty-icon">📭</div>
                        <h3>No Audit Entries</h3>
                        <p>Administrative actions will appear here</p>
                    </div>
                ) : (
                    <table className="audit-table">
                        <thead>
                            <tr>
                                <th>Time</th>
                                <th>Action</th>
                                <th>Target</th>
                                <th>Details</th>
                                <th>Data</th>
                            </tr>
                        </thead>
                        <tbody>
                            {entries.map(entry => (
                                <tr key={entry.id} className={getActionClass(entry.action)}>
                                    <td className="col-time">
                                        {formatTimestamp(entry.timestamp)}
                                    </td>
                                    <td className="col-action">
                                        <span className="action-badge">
                                            {getActionIcon(entry.action)} {entry.action}
                                        </span>
                                    </td>
                                    <td className="col-target">
                                        <span className="target-type">{entry.target_type}</span>
                                        <span className="target-id" title={entry.target_id}>
                                            {entry.target_id.substring(0, 8)}...
                                        </span>
                                    </td>
                                    <td className="col-details">
                                        {entry.details ? (
                                            <code className="details-json">
                                                {entry.details.length > 50
                                                    ? entry.details.substring(0, 50) + '...'
                                                    : entry.details}
                                            </code>
                                        ) : (
                                            <span className="no-details">--</span>
                                        )}
                                    </td>
                                    <td className="col-bytes">
                                        {formatBytes(entry.bytes_affected)}
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                )}
            </div>

            {/* Pagination */}
            {totalPages > 1 && (
                <div className="audit-pagination">
                    <button
                        className="btn btn-secondary"
                        onClick={() => setPage(p => Math.max(0, p - 1))}
                        disabled={page === 0}
                    >
                        ← Previous
                    </button>
                    <span className="page-info">
                        Page {page + 1} of {totalPages}
                    </span>
                    <button
                        className="btn btn-secondary"
                        onClick={() => setPage(p => Math.min(totalPages - 1, p + 1))}
                        disabled={page >= totalPages - 1}
                    >
                        Next →
                    </button>
                </div>
            )}
        </div>
    );
}

export default AuditLog;
