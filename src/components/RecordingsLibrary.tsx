// Management Suite - Recordings Library
// Displays recordings with storage info, allows selection and deletion

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

// =============================================================================
// Types
// =============================================================================

interface RecordingWithStorage {
    id: string;
    title: string;
    started_at: string;
    ended_at: string | null;
    duration_seconds: number | null;
    frames_count: number;
    frames_bytes: number;
    video_bytes: number;
    audio_bytes: number;
    total_bytes: number;
    total_bytes_formatted: string;
}

interface StorageStats {
    meetings_count: number;
    frames_count: number;
    frames_bytes: number;
    frames_bytes_formatted: string;
    video_bytes: number;
    video_bytes_formatted: string;
    audio_bytes: number;
    audio_bytes_formatted: string;
    total_bytes: number;
    total_bytes_formatted: string;
}

interface DeletePreview {
    meeting_ids: string[];
    total_files: number;
    total_bytes: number;
    total_bytes_formatted: string;
    files_by_type: Record<string, number>;
    safe_to_delete: boolean;
    warnings: string[];
}

// =============================================================================
// DeletePreviewModal Component
// =============================================================================

interface DeletePreviewModalProps {
    preview: DeletePreview | null;
    isOpen: boolean;
    isDeleting: boolean;
    onConfirm: () => void;
    onCancel: () => void;
}

function DeletePreviewModal({ preview, isOpen, isDeleting, onConfirm, onCancel }: DeletePreviewModalProps) {
    // Keyboard navigation
    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === 'Escape' && !isDeleting) {
            onCancel();
        } else if (e.key === 'Enter' && preview?.safe_to_delete && !isDeleting) {
            onConfirm();
        }
    };

    if (!isOpen || !preview) return null;

    return (
        <div
            className="modal-overlay"
            onClick={onCancel}
            onKeyDown={handleKeyDown}
            role="dialog"
            aria-modal="true"
            aria-labelledby="delete-modal-title"
        >
            <div className="modal-content delete-preview-modal" onClick={e => e.stopPropagation()}>
                <div className="modal-header">
                    <h2 id="delete-modal-title">🗑️ Confirm Deletion</h2>
                    <button className="modal-close" onClick={onCancel} aria-label="Close">×</button>
                </div>

                <div className="modal-body">
                    {/* Warnings */}
                    {preview.warnings.length > 0 && (
                        <div className="delete-warnings" role="alert">
                            {preview.warnings.map((warning, idx) => (
                                <div key={idx} className="warning-item">
                                    ⚠️ {warning}
                                </div>
                            ))}
                        </div>
                    )}

                    {/* Summary */}
                    <div className="delete-summary">
                        <div className="summary-stat">
                            <span className="stat-label">Recordings</span>
                            <span className="stat-value">{preview.meeting_ids.length}</span>
                        </div>
                        <div className="summary-stat">
                            <span className="stat-label">Total Files</span>
                            <span className="stat-value">{preview.total_files}</span>
                        </div>
                        <div className="summary-stat highlight">
                            <span className="stat-label">Space to Free</span>
                            <span className="stat-value">{preview.total_bytes_formatted}</span>
                        </div>
                    </div>

                    {/* File breakdown */}
                    <div className="file-breakdown">
                        <h4>Files by Type</h4>
                        <div className="breakdown-list">
                            {Object.entries(preview.files_by_type).map(([type, count]) => (
                                <div key={type} className="breakdown-item">
                                    <span className="file-type">{type}</span>
                                    <span className="file-count">{count} files</span>
                                </div>
                            ))}
                        </div>
                    </div>

                    {!preview.safe_to_delete && (
                        <div className="danger-warning" role="alert">
                            ⛔ Cannot delete while recording is in progress
                        </div>
                    )}
                </div>

                <div className="modal-footer">
                    <button className="btn btn-secondary" onClick={onCancel} disabled={isDeleting}>
                        Cancel
                    </button>
                    <button
                        className="btn btn-danger"
                        onClick={onConfirm}
                        disabled={!preview.safe_to_delete || isDeleting}
                        aria-busy={isDeleting}
                    >
                        {isDeleting ? 'Deleting...' : `Delete ${preview.meeting_ids.length} Recording${preview.meeting_ids.length !== 1 ? 's' : ''}`}
                    </button>
                </div>
            </div>
        </div>
    );
}

// =============================================================================
// RecordingsLibrary Component
// =============================================================================

export function RecordingsLibrary() {
    const [recordings, setRecordings] = useState<RecordingWithStorage[]>([]);
    const [storageStats, setStorageStats] = useState<StorageStats | null>(null);
    const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
    const [searchQuery, setSearchQuery] = useState('');
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    // Delete modal state
    const [deletePreview, setDeletePreview] = useState<DeletePreview | null>(null);
    const [showDeleteModal, setShowDeleteModal] = useState(false);
    const [isDeleting, setIsDeleting] = useState(false);

    // Load recordings
    const loadRecordings = useCallback(async () => {
        setIsLoading(true);
        setError(null);
        try {
            const [recs, stats] = await Promise.all([
                invoke<RecordingWithStorage[]>('list_recordings_with_storage', {
                    limit: 100,
                    offset: 0,
                    search: searchQuery || null
                }),
                invoke<StorageStats>('get_admin_storage_stats')
            ]);
            setRecordings(recs);
            setStorageStats(stats);
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            setIsLoading(false);
        }
    }, [searchQuery]);

    useEffect(() => {
        loadRecordings();
    }, [loadRecordings]);

    // Selection handlers
    const toggleSelection = (id: string) => {
        const newSelected = new Set(selectedIds);
        if (newSelected.has(id)) {
            newSelected.delete(id);
        } else {
            newSelected.add(id);
        }
        setSelectedIds(newSelected);
    };

    const selectAll = () => {
        if (selectedIds.size === recordings.length) {
            setSelectedIds(new Set());
        } else {
            setSelectedIds(new Set(recordings.map(r => r.id)));
        }
    };

    // Delete handlers
    const handlePreviewDelete = async () => {
        if (selectedIds.size === 0) return;

        try {
            const preview = await invoke<DeletePreview>('preview_delete_recordings', {
                meetingIds: Array.from(selectedIds)
            });
            setDeletePreview(preview);
            setShowDeleteModal(true);
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        }
    };

    const handleConfirmDelete = async () => {
        if (!deletePreview) return;

        setIsDeleting(true);
        try {
            await invoke('delete_recordings', {
                meetingIds: deletePreview.meeting_ids,
                deleteDbRecords: true
            });
            setShowDeleteModal(false);
            setDeletePreview(null);
            setSelectedIds(new Set());
            await loadRecordings();
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            setIsDeleting(false);
        }
    };

    // Format duration
    const formatDuration = (seconds: number | null) => {
        if (!seconds) return '--:--';
        const mins = Math.floor(seconds / 60);
        const secs = seconds % 60;
        return `${mins}:${secs.toString().padStart(2, '0')}`;
    };

    // Format date
    const formatDate = (dateStr: string) => {
        const date = new Date(dateStr);
        return date.toLocaleDateString('en-US', {
            month: 'short',
            day: 'numeric',
            year: 'numeric',
            hour: '2-digit',
            minute: '2-digit'
        });
    };

    return (
        <div className="recordings-library">
            {/* Storage Overview */}
            <div className="storage-overview">
                <h3>📦 Storage Overview</h3>
                {storageStats ? (
                    <div className="storage-stats-grid">
                        <div className="storage-stat-card">
                            <div className="stat-icon">🎬</div>
                            <div className="stat-info">
                                <div className="stat-value">{storageStats.meetings_count}</div>
                                <div className="stat-label">Recordings</div>
                            </div>
                        </div>
                        <div className="storage-stat-card">
                            <div className="stat-icon">🖼️</div>
                            <div className="stat-info">
                                <div className="stat-value">{storageStats.frames_bytes_formatted}</div>
                                <div className="stat-label">Frames</div>
                            </div>
                        </div>
                        <div className="storage-stat-card">
                            <div className="stat-icon">📹</div>
                            <div className="stat-info">
                                <div className="stat-value">{storageStats.video_bytes_formatted}</div>
                                <div className="stat-label">Video</div>
                            </div>
                        </div>
                        <div className="storage-stat-card">
                            <div className="stat-icon">🔊</div>
                            <div className="stat-info">
                                <div className="stat-value">{storageStats.audio_bytes_formatted}</div>
                                <div className="stat-label">Audio</div>
                            </div>
                        </div>
                        <div className="storage-stat-card total">
                            <div className="stat-icon">💾</div>
                            <div className="stat-info">
                                <div className="stat-value">{storageStats.total_bytes_formatted}</div>
                                <div className="stat-label">Total Used</div>
                            </div>
                        </div>
                    </div>
                ) : (
                    <div className="loading-placeholder">Loading storage stats...</div>
                )}
            </div>

            {/* Toolbar */}
            <div className="library-toolbar">
                <div className="toolbar-left">
                    <input
                        type="text"
                        className="search-input"
                        placeholder="Search recordings..."
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                    />
                    <button className="btn btn-icon" onClick={loadRecordings} title="Refresh">
                        🔄
                    </button>
                </div>
                <div className="toolbar-right">
                    <span className="selection-count">
                        {selectedIds.size > 0 && `${selectedIds.size} selected`}
                    </span>
                    <button
                        className="btn btn-secondary"
                        onClick={selectAll}
                    >
                        {selectedIds.size === recordings.length ? 'Deselect All' : 'Select All'}
                    </button>
                    <button
                        className="btn btn-danger"
                        onClick={handlePreviewDelete}
                        disabled={selectedIds.size === 0}
                    >
                        🗑️ Delete Selected
                    </button>
                </div>
            </div>

            {/* Error display */}
            {error && (
                <div className="error-banner">
                    ⚠️ {error}
                    <button onClick={() => setError(null)}>×</button>
                </div>
            )}

            {/* Recordings list */}
            <div className="recordings-list">
                {isLoading ? (
                    <div className="loading-state">
                        <div className="loading-spinner" />
                        <p>Loading recordings...</p>
                    </div>
                ) : recordings.length === 0 ? (
                    <div className="empty-state">
                        <div className="empty-icon">📭</div>
                        <h3>No Recordings Found</h3>
                        <p>{searchQuery ? 'Try a different search term' : 'Start recording to see your meetings here'}</p>
                    </div>
                ) : (
                    recordings.map(recording => (
                        <div
                            key={recording.id}
                            className={`recording-card ${selectedIds.has(recording.id) ? 'selected' : ''}`}
                        >
                            <div className="recording-checkbox" onClick={() => toggleSelection(recording.id)}>
                                <input
                                    type="checkbox"
                                    checked={selectedIds.has(recording.id)}
                                    onChange={() => { }}
                                />
                            </div>
                            <div className="recording-info">
                                <div className="recording-title">{recording.title}</div>
                                <div className="recording-meta">
                                    <span className="meta-date">{formatDate(recording.started_at)}</span>
                                    <span className="meta-duration">⏱️ {formatDuration(recording.duration_seconds)}</span>
                                </div>
                            </div>
                            <div className="recording-storage">
                                <div className="storage-breakdown">
                                    {recording.frames_count > 0 && (
                                        <span className="storage-item">🖼️ {recording.frames_count} frames</span>
                                    )}
                                    {recording.video_bytes > 0 && (
                                        <span className="storage-item">📹 video</span>
                                    )}
                                    {recording.audio_bytes > 0 && (
                                        <span className="storage-item">🔊 audio</span>
                                    )}
                                </div>
                                <div className="storage-total">{recording.total_bytes_formatted}</div>
                            </div>
                        </div>
                    ))
                )}
            </div>

            {/* Delete Preview Modal */}
            <DeletePreviewModal
                preview={deletePreview}
                isOpen={showDeleteModal}
                isDeleting={isDeleting}
                onConfirm={handleConfirmDelete}
                onCancel={() => {
                    setShowDeleteModal(false);
                    setDeletePreview(null);
                }}
            />
        </div>
    );
}

export default RecordingsLibrary;
