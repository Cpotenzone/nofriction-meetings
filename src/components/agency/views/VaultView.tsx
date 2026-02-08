import React, { useState, useEffect } from 'react';
import * as tauri from '../../../lib/tauri';
import {
    VaultTopic,
    VaultTreeNode,
    VaultFileContent,
    VaultStatus,
    Meeting
} from '../../../lib/tauri';
import './VaultView.css';

interface VaultViewProps {
    onSelectMeeting: (id: string) => void;
}

export const VaultView: React.FC<VaultViewProps> = ({ onSelectMeeting: _onSelectMeeting }) => {
    const [status, setStatus] = useState<VaultStatus | null>(null);
    const [topics, setTopics] = useState<VaultTopic[]>([]);
    const [selectedTopic, setSelectedTopic] = useState<VaultTopic | null>(null);
    const [tree, setTree] = useState<VaultTreeNode | null>(null);
    const [selectedFile, setSelectedFile] = useState<VaultFileContent | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [isCreatingTopic, setIsCreatingTopic] = useState(false);
    const [newTopicName, setNewTopicName] = useState('');
    const [meetingList, setMeetingList] = useState<Meeting[]>([]);
    const [isExporting, setIsExporting] = useState(false);
    const [isExportLoading, setIsExportLoading] = useState(false);

    useEffect(() => {
        loadVaultData();
    }, []);

    const loadVaultData = async () => {
        setIsLoading(true);
        try {
            const vaultStatus = await tauri.getVaultStatus();
            setStatus(vaultStatus);

            if (vaultStatus.valid) {
                const [vaultTopics, vaultTree] = await Promise.all([
                    tauri.listVaultTopics(),
                    tauri.getVaultTree()
                ]);
                setTopics(vaultTopics);
                setTree(vaultTree);

                if (vaultTopics.length > 0 && !selectedTopic) {
                    setSelectedTopic(vaultTopics[0]);
                }
            }
        } catch (err) {
            console.error("Failed to load vault data:", err);
        } finally {
            setIsLoading(false);
        }
    };

    const handleSelectFile = async (path: string) => {
        if (path.endsWith('.md')) {
            try {
                const content = await tauri.readVaultFile(path);
                setSelectedFile(content);
            } catch (err) {
                console.error("Failed to read file:", err);
            }
        }
    };

    const handleCreateTopic = async () => {
        if (!newTopicName.trim()) return;
        try {
            await tauri.createVaultTopic(newTopicName, []);
            setNewTopicName('');
            setIsCreatingTopic(false);
            loadVaultData();
        } catch (err) {
            console.error("Failed to create topic:", err);
        }
    };

    const handleExportMeeting = async (meetingId: string) => {
        if (!selectedTopic) return;
        setIsExportLoading(true);
        try {
            await tauri.exportMeetingToVault(selectedTopic.name, meetingId);
            setIsExporting(false);
            loadVaultData();
        } catch (err) {
            console.error("Failed to export meeting:", err);
            alert("Export failed. See console for details.");
        } finally {
            setIsExportLoading(false);
        }
    };

    const handleUploadFile = async () => {
        if (!selectedTopic) return;
        // In a real app, we'd use @tauri-apps/plugin-dialog
        // Since we are refining, we use a simple prompt for now or assume internal path
        const sourcePath = prompt("Enter the absolute path of the file to upload:");
        if (!sourcePath) return;

        try {
            await tauri.uploadToVault(selectedTopic.name, sourcePath);
            loadVaultData();
        } catch (err) {
            console.error("Failed to upload file:", err);
            alert("Upload failed. Ensure the path is valid and accessible.");
        }
    };

    const handleOpenExportModal = async () => {
        try {
            const meetings = await tauri.getMeetings(10);
            setMeetingList(meetings);
            setIsExporting(true);
        } catch (err) {
            console.error("Failed to load meetings:", err);
        }
    };

    const handleJumpToMeeting = (meetingId: string) => {
        // Remove the date/time suffix if it exists in the inspector text
        const cleanId = meetingId.includes(' - ') ? meetingId.split(' - ')[0] : meetingId;
        _onSelectMeeting(cleanId);
    };

    const renderTree = (node: VaultTreeNode) => {
        return (
            <div key={node.path} className="tree-node">
                <div
                    className={`node-row ${selectedFile?.path === node.path ? 'active' : ''}`}
                    onClick={() => node.isDir ? null : handleSelectFile(node.path)}
                >
                    <span className="node-icon">{node.isDir ? '📁' : '📄'}</span>
                    <span className="node-name">{node.name}</span>
                </div>
                {node.isDir && node.children && node.children.length > 0 && (
                    <div className="node-children">
                        {node.children.map(child => renderTree(child))}
                    </div>
                )}
            </div>
        );
    };

    if (isLoading) {
        return (
            <div className="vault-view-loading">
                <div className="loading-spinner" />
                <p>Accessing Obsidian Vault...</p>
            </div>
        );
    }

    if (!status?.configured || !status?.valid) {
        return (
            <div className="vault-empty-state">
                <div className="empty-icon">📂</div>
                <h2 className="empty-title">Vault Not Configured</h2>
                <p className="empty-desc">
                    Connect your Obsidian vault in Settings to start managing meeting knowledge.
                </p>
                <button
                    className="action-btn primary"
                    onClick={() => {
                        // This usually triggers a custom event or handled by parent
                        // For now we just tell the user to use settings
                    }}
                >
                    Open Settings
                </button>
            </div>
        );
    }

    return (
        <div className="vault-view">
            {/* Left Sidebar: Topics */}
            <aside className="vault-sidebar">
                <div className="sidebar-section">
                    <h3>Topics</h3>
                    <div className="topic-list">
                        {topics.map(topic => (
                            <div
                                key={topic.name}
                                className={`topic-item ${selectedTopic?.name === topic.name ? 'active' : ''}`}
                                onClick={() => setSelectedTopic(topic)}
                            >
                                <span className="topic-icon">📁</span>
                                <span className="topic-name">{topic.name}</span>
                                <span className="topic-count">{topic.noteCount + topic.meetings.length}</span>
                            </div>
                        ))}
                    </div>

                    {isCreatingTopic ? (
                        <div className="topic-create-form active">
                            <input
                                autoFocus
                                type="text"
                                placeholder="Topic name..."
                                value={newTopicName}
                                onChange={e => setNewTopicName(e.target.value)}
                                onKeyDown={e => e.key === 'Enter' && handleCreateTopic()}
                            />
                            <div className="form-actions">
                                <button onClick={handleCreateTopic}>Add</button>
                                <button onClick={() => setIsCreatingTopic(false)}>×</button>
                            </div>
                        </div>
                    ) : (
                        <button
                            className="add-topic-btn"
                            onClick={() => setIsCreatingTopic(true)}
                        >
                            + New Topic
                        </button>
                    )}
                </div>

                <div className="sidebar-section file-tree-section">
                    <h3>Vault Explorer</h3>
                    <div className="vault-file-tree">
                        {tree && renderTree(tree)}
                    </div>
                </div>
            </aside>

            {/* Main Content: Markdown Preview */}
            <main className="vault-main">
                <div className="vault-path-bar">
                    {selectedFile ? selectedFile.path.split('/').slice(-3).join(' / ') : 'Select a file to preview'}
                </div>

                <div className="markdown-container">
                    {selectedFile ? (
                        <div className="markdown-body">
                            {selectedFile.frontmatter && Object.keys(selectedFile.frontmatter).length > 0 && (
                                <div className="markdown-metadata">
                                    {Object.entries(selectedFile.frontmatter).map(([key, val]) => (
                                        <div key={key} className="meta-item">
                                            <span className="meta-key">{key}:</span>
                                            <span className="meta-val">{String(val)}</span>
                                        </div>
                                    ))}
                                </div>
                            )}
                            <h1>{selectedFile.frontmatter?.title || selectedFile.path.split('/').pop()?.replace('.md', '')}</h1>
                            <div className="markdown-content">
                                {selectedFile.body.split('\n').map((line, i) => {
                                    if (line.startsWith('# ')) return <h1 key={i}>{line.substring(2)}</h1>;
                                    if (line.startsWith('## ')) return <h2 key={i}>{line.substring(3)}</h2>;
                                    if (line.startsWith('### ')) return <h3 key={i}>{line.substring(4)}</h3>;
                                    if (line.startsWith('- ')) return <li key={i}>{line.substring(2)}</li>;
                                    if (line.trim() === '') return <br key={i} />;
                                    return <p key={i}>{line}</p>;
                                })}
                            </div>
                        </div>
                    ) : (
                        <div className="vault-empty-state">
                            <div className="empty-icon">📄</div>
                            <h2 className="empty-title">No File Selected</h2>
                            <p className="empty-desc">Select a meeting or note from the sidebar to preview its content.</p>
                        </div>
                    )}
                </div>
            </main>

            {/* Right Panel: Inspector & Actions */}
            <section className="vault-inspector">
                <div className="inspector-header">
                    <h3>{selectedTopic ? selectedTopic.name : 'Topic Details'}</h3>
                    <p>Managed via noFriction</p>
                </div>

                <div className="action-card">
                    <h4>Actions</h4>
                    <button
                        className="action-btn primary"
                        disabled={!selectedTopic || isExportLoading}
                        onClick={handleOpenExportModal}
                    >
                        {isExportLoading ? '🚢 Exporting...' : '🚢 Export Meeting'}
                    </button>
                    <button
                        className="action-btn"
                        disabled={!selectedTopic}
                        onClick={handleUploadFile}
                    >
                        📤 Upload File
                    </button>
                </div>

                {isExporting && (
                    <div className="export-modal-overlay">
                        <div className="export-modal">
                            <h4>Export Recent Meeting to {selectedTopic?.name}</h4>
                            <div className="meeting-select-list">
                                {meetingList.map(m => (
                                    <div key={m.id} className="meeting-select-item" onClick={() => handleExportMeeting(m.id)}>
                                        <span className="m-title">{m.title || "Untitled Meeting"}</span>
                                        <span className="m-date">{new Date(m.started_at).toLocaleString()}</span>
                                    </div>
                                ))}
                            </div>
                            <button className="cancel-btn" onClick={() => setIsExporting(false)}>Cancel</button>
                        </div>
                    </div>
                )}

                {selectedTopic && (
                    <div className="inspector-section">
                        <h4>Linked Meetings</h4>
                        <div className="inspector-list">
                            {selectedTopic.meetings.length > 0 ? (
                                selectedTopic.meetings.map(m => (
                                    <div key={m} className="inspector-item" onClick={() => handleJumpToMeeting(m)}>
                                        <span className="item-icon">📅</span>
                                        <span className="item-text">{m}</span>
                                    </div>
                                ))
                            ) : (
                                <p className="empty-text">No meetings linked yet</p>
                            )}
                        </div>
                    </div>
                )}

                <div className="inspector-footer">
                    <button className="action-btn" onClick={() => loadVaultData()}>
                        🔄 Refresh Vault
                    </button>
                </div>
            </section>
        </div>
    );
};
