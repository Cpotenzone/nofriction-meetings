// Management Suite - Learned Data Editor
// Browse, search, edit, and version learned data (text_snapshots, entities, episodes)

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

// =============================================================================
// Types
// =============================================================================

interface LearnedDataItem {
    entity_type: string;
    entity_id: string;
    title: string;
    content: string;
    metadata: string | null;
    created_at: string;
    updated_at: string;
}

interface DataVersion {
    id: number;
    entity_type: string;
    entity_id: string;
    field_name: string;
    previous_value: string | null;
    new_value: string | null;
    diff: string | null;
    timestamp: string;
}

interface EditResult {
    success: boolean;
    version_id: number;
    message: string;
}

type EntityType = 'text_snapshot' | 'entity' | 'episode' | 'activity_log';

// =============================================================================
// Version History Modal
// =============================================================================

interface VersionHistoryModalProps {
    versions: DataVersion[];
    isOpen: boolean;
    isLoading: boolean;
    onRestore: (versionId: number) => void;
    onClose: () => void;
}

function VersionHistoryModal({ versions, isOpen, isLoading, onRestore, onClose }: VersionHistoryModalProps) {
    // Keyboard navigation
    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === 'Escape') {
            onClose();
        }
    };

    if (!isOpen) return null;

    const formatTimestamp = (ts: string) => {
        const date = new Date(ts);
        return date.toLocaleString('en-US', {
            month: 'short',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit'
        });
    };

    return (
        <div
            className="modal-overlay"
            onClick={onClose}
            onKeyDown={handleKeyDown}
            role="dialog"
            aria-modal="true"
            aria-labelledby="version-modal-title"
        >
            <div className="modal-content version-history-modal" onClick={e => e.stopPropagation()}>
                <div className="modal-header">
                    <h2 id="version-modal-title">📜 Version History</h2>
                    <button className="modal-close" onClick={onClose} aria-label="Close">×</button>
                </div>


                <div className="modal-body">
                    {isLoading ? (
                        <div className="loading-state">
                            <div className="loading-spinner" />
                            <p>Loading versions...</p>
                        </div>
                    ) : versions.length === 0 ? (
                        <div className="empty-state">
                            <div className="empty-icon">📭</div>
                            <h3>No Version History</h3>
                            <p>This item has no previous versions</p>
                        </div>
                    ) : (
                        <div className="versions-list">
                            {versions.map(version => (
                                <div key={version.id} className="version-item">
                                    <div className="version-header">
                                        <span className="version-time">{formatTimestamp(version.timestamp)}</span>
                                        <span className="version-field">{version.field_name}</span>
                                    </div>
                                    {version.diff && (
                                        <pre className="version-diff">{version.diff}</pre>
                                    )}
                                    <button
                                        className="btn btn-secondary btn-sm"
                                        onClick={() => onRestore(version.id)}
                                    >
                                        ↩️ Restore
                                    </button>
                                </div>
                            ))}
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}

// =============================================================================
// Edit Modal
// =============================================================================

interface EditModalProps {
    item: LearnedDataItem | null;
    isOpen: boolean;
    isSaving: boolean;
    onSave: (value: string) => void;
    onCancel: () => void;
}

function EditModal({ item, isOpen, isSaving, onSave, onCancel }: EditModalProps) {
    const [editValue, setEditValue] = useState('');

    useEffect(() => {
        if (item) {
            setEditValue(item.content);
        }
    }, [item]);

    // Keyboard navigation
    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === 'Escape' && !isSaving) {
            onCancel();
        } else if ((e.metaKey || e.ctrlKey) && e.key === 'Enter' && !isSaving) {
            onSave(editValue);
        }
    };

    if (!isOpen || !item) return null;

    return (
        <div
            className="modal-overlay"
            onClick={onCancel}
            onKeyDown={handleKeyDown}
            role="dialog"
            aria-modal="true"
            aria-labelledby="edit-modal-title"
        >
            <div className="modal-content edit-modal" onClick={e => e.stopPropagation()}>
                <div className="modal-header">
                    <h2 id="edit-modal-title">✏️ Edit {item.entity_type}</h2>
                    <button className="modal-close" onClick={onCancel} aria-label="Close">×</button>
                </div>

                <div className="modal-body">
                    <div className="edit-item-title">{item.title}</div>
                    <div className="edit-field">
                        <label htmlFor="edit-content">Content</label>
                        <textarea
                            id="edit-content"
                            value={editValue}
                            onChange={(e) => setEditValue(e.target.value)}
                            rows={8}
                            className="edit-textarea"
                            aria-describedby="edit-hint"
                        />
                        <small id="edit-hint" className="edit-hint">Press Cmd+Enter to save</small>
                    </div>
                </div>

                <div className="modal-footer">
                    <button className="btn btn-secondary" onClick={onCancel} disabled={isSaving}>
                        Cancel
                    </button>
                    <button
                        className="btn btn-primary"
                        onClick={() => onSave(editValue)}
                        disabled={isSaving}
                        aria-busy={isSaving}
                    >
                        {isSaving ? 'Saving...' : 'Save Changes'}
                    </button>
                </div>
            </div>
        </div>
    );
}

// =============================================================================
// LearnedDataEditor Component
// =============================================================================

export function LearnedDataEditor() {
    const [entityType, setEntityType] = useState<EntityType>('text_snapshot');
    const [items, setItems] = useState<LearnedDataItem[]>([]);
    const [totalCount, setTotalCount] = useState(0);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [searchQuery, setSearchQuery] = useState('');
    const [page, setPage] = useState(0);

    // Edit modal state
    const [editItem, setEditItem] = useState<LearnedDataItem | null>(null);
    const [showEditModal, setShowEditModal] = useState(false);
    const [isSaving, setIsSaving] = useState(false);

    // Version history state
    const [versions, setVersions] = useState<DataVersion[]>([]);
    const [showVersions, setShowVersions] = useState(false);
    const [isLoadingVersions, setIsLoadingVersions] = useState(false);

    const PAGE_SIZE = 25;

    // Load data
    const loadData = useCallback(async () => {
        setIsLoading(true);
        setError(null);
        try {
            const [dataItems, count] = await Promise.all([
                invoke<LearnedDataItem[]>('list_learned_data', {
                    entityType,
                    limit: PAGE_SIZE,
                    offset: page * PAGE_SIZE,
                    search: searchQuery || null
                }),
                invoke<number>('count_learned_data', {
                    entityType,
                    search: searchQuery || null
                })
            ]);
            setItems(dataItems);
            setTotalCount(count);
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            setIsLoading(false);
        }
    }, [entityType, page, searchQuery]);

    useEffect(() => {
        loadData();
    }, [loadData]);

    // Handle edit
    const handleEdit = (item: LearnedDataItem) => {
        setEditItem(item);
        setShowEditModal(true);
    };

    const handleSaveEdit = async (newValue: string) => {
        if (!editItem) return;

        setIsSaving(true);
        try {
            // Determine field name based on entity type
            const fieldName = editItem.entity_type === 'text_snapshot' ? 'content' :
                editItem.entity_type === 'entity' ? 'entity_value' :
                    editItem.entity_type === 'episode' ? 'document_path' :
                        'action';

            await invoke<EditResult>('edit_learned_data', {
                entityType: editItem.entity_type,
                entityId: editItem.entity_id,
                fieldName,
                newValue
            });

            setShowEditModal(false);
            setEditItem(null);
            await loadData();
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            setIsSaving(false);
        }
    };

    // Handle version history
    const handleViewVersions = async (item: LearnedDataItem) => {
        setEditItem(item);
        setShowVersions(true);
        setIsLoadingVersions(true);

        try {
            const versionHistory = await invoke<DataVersion[]>('get_data_versions', {
                entityType: item.entity_type,
                entityId: item.entity_id
            });
            setVersions(versionHistory);
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            setIsLoadingVersions(false);
        }
    };

    const handleRestoreVersion = async (versionId: number) => {
        try {
            await invoke<EditResult>('restore_data_version', { versionId });
            setShowVersions(false);
            await loadData();
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        }
    };

    // Format date
    const formatDate = (dateStr: string) => {
        if (!dateStr) return '--';
        const date = new Date(dateStr);
        return date.toLocaleDateString('en-US', {
            month: 'short',
            day: 'numeric',
            year: 'numeric'
        });
    };

    const entityTypeLabels: Record<EntityType, { label: string; icon: string }> = {
        text_snapshot: { label: 'Text Snapshots', icon: '📝' },
        entity: { label: 'Entities', icon: '🏷️' },
        episode: { label: 'Episodes', icon: '📂' },
        activity_log: { label: 'Activity Log', icon: '📋' },
    };

    const totalPages = Math.ceil(totalCount / PAGE_SIZE);

    return (
        <div className="learned-data-editor">
            <div className="editor-header">
                <h3>📚 Learned Data Editor</h3>
                <p className="editor-subtitle">Browse and edit OCR text, entities, and episodes with version control</p>
            </div>

            {/* Entity Type Selector */}
            <div className="entity-type-selector">
                {(Object.keys(entityTypeLabels) as EntityType[]).map(type => (
                    <button
                        key={type}
                        className={`entity-type-btn ${entityType === type ? 'active' : ''}`}
                        onClick={() => {
                            setEntityType(type);
                            setPage(0);
                        }}
                    >
                        <span className="type-icon">{entityTypeLabels[type].icon}</span>
                        <span className="type-label">{entityTypeLabels[type].label}</span>
                    </button>
                ))}
            </div>

            {/* Toolbar */}
            <div className="editor-toolbar">
                <div className="toolbar-left">
                    <input
                        type="text"
                        className="search-input"
                        placeholder="Search..."
                        value={searchQuery}
                        onChange={(e) => {
                            setSearchQuery(e.target.value);
                            setPage(0);
                        }}
                    />
                    <button className="btn btn-icon" onClick={loadData} title="Refresh">
                        🔄
                    </button>
                </div>
                <div className="toolbar-right">
                    <span className="item-count">
                        {totalCount} {totalCount === 1 ? 'item' : 'items'}
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

            {/* Data table */}
            <div className="data-table-container">
                {isLoading ? (
                    <div className="loading-state">
                        <div className="loading-spinner" />
                        <p>Loading {entityTypeLabels[entityType].label.toLowerCase()}...</p>
                    </div>
                ) : items.length === 0 ? (
                    <div className="empty-state">
                        <div className="empty-icon">{entityTypeLabels[entityType].icon}</div>
                        <h3>No {entityTypeLabels[entityType].label}</h3>
                        <p>{searchQuery ? 'Try a different search term' : 'No data found for this type'}</p>
                    </div>
                ) : (
                    <table className="data-table">
                        <thead>
                            <tr>
                                <th>ID</th>
                                <th>Title / Content</th>
                                <th>Created</th>
                                <th>Updated</th>
                                <th>Actions</th>
                            </tr>
                        </thead>
                        <tbody>
                            {items.map(item => (
                                <tr key={`${item.entity_type}-${item.entity_id}`}>
                                    <td className="col-id">
                                        <code>{item.entity_id}</code>
                                    </td>
                                    <td className="col-content">
                                        <div className="item-title">{item.title}</div>
                                        <div className="item-preview">
                                            {item.content.length > 100
                                                ? item.content.substring(0, 100) + '...'
                                                : item.content}
                                        </div>
                                    </td>
                                    <td className="col-date">{formatDate(item.created_at)}</td>
                                    <td className="col-date">{formatDate(item.updated_at)}</td>
                                    <td className="col-actions">
                                        <button
                                            className="btn btn-icon"
                                            onClick={() => handleEdit(item)}
                                            title="Edit"
                                        >
                                            ✏️
                                        </button>
                                        <button
                                            className="btn btn-icon"
                                            onClick={() => handleViewVersions(item)}
                                            title="Version History"
                                        >
                                            📜
                                        </button>
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                )}
            </div>

            {/* Pagination */}
            {totalPages > 1 && (
                <div className="data-pagination">
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

            {/* Edit Modal */}
            <EditModal
                item={editItem}
                isOpen={showEditModal}
                isSaving={isSaving}
                onSave={handleSaveEdit}
                onCancel={() => {
                    setShowEditModal(false);
                    setEditItem(null);
                }}
            />

            {/* Version History Modal */}
            <VersionHistoryModal
                versions={versions}
                isOpen={showVersions}
                isLoading={isLoadingVersions}
                onRestore={handleRestoreVersion}
                onClose={() => {
                    setShowVersions(false);
                    setVersions([]);
                }}
            />
        </div>
    );
}

export default LearnedDataEditor;
