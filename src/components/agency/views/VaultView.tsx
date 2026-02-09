import React, { useState, useEffect, useRef } from 'react';
import {
    Folder,
    FileText,
    Plus,
    Ship,
    Upload,
    RefreshCcw,
    X,
    File as FileIcon,
    FolderOpen,
    Settings,
    Maximize2
} from 'lucide-react';
import * as tauri from '../../../lib/tauri';
import {
    VaultTopic,
    VaultTreeNode,
    VaultFileContent,
    VaultStatus,
    Meeting,
    VaultFile
} from '../../../lib/tauri';
import { BacklinksPanel } from './BacklinksPanel';
import { VaultTags } from './VaultTags';
import { VaultGraph } from './VaultGraph';
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
    const [activeTag, setActiveTag] = useState<string | null>(null);
    const [tagFiles, setTagFiles] = useState<VaultFile[]>([]);
    const [showGraph, setShowGraph] = useState(false);
    const fileInputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        loadVaultData();

        const handleKeyDown = (e: KeyboardEvent) => {
            if ((e.metaKey || e.ctrlKey) && e.key === 'g') {
                e.preventDefault();
                setShowGraph(prev => !prev);
            }
        };

        window.addEventListener('keydown', handleKeyDown);
        return () => window.removeEventListener('keydown', handleKeyDown);
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

    const handleUploadFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
        if (!selectedTopic || !e.target.files?.length) return;
        const file = e.target.files[0];

        const sourcePath = prompt(`Upload "${file.name}" to topic "${selectedTopic.name}"? \n\nPlease enter the absolute path of this file to confirm:`);
        if (!sourcePath) return;

        try {
            await tauri.uploadToVault(selectedTopic.name, sourcePath);
            loadVaultData();
        } catch (err) {
            console.error("Failed to upload file:", err);
            alert("Upload failed. Ensure the path is valid.");
        } finally {
            if (e.target) {
                e.target.value = '';
            }
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

    const findFileInTree = (node: VaultTreeNode, name: string): string | null => {
        if (!node.isDir && (node.name === name || node.name === `${name}.md`)) {
            return node.path;
        }
        if (node.children) {
            for (const child of node.children) {
                const found = findFileInTree(child, name);
                if (found) return found;
            }
        }
        return null;
    };

    const handleWikilinkClick = (target: string) => {
        if (!tree) return;
        const path = findFileInTree(tree, target);
        if (path) {
            handleSelectFile(path);
        } else {
            console.warn(`File not found for wikilink: ${target}`);
        }
    };

    const handleSelectTag = async (tag: string) => {
        if (activeTag === tag) {
            setActiveTag(null);
            setTagFiles([]);
            return;
        }
        setActiveTag(tag);
        try {
            const files = await tauri.getFilesByTag(tag);
            setTagFiles(files);
            // Optionally clear selected topic to focus on tag results
            setSelectedTopic(null);
        } catch (err) {
            console.error("Failed to load tag files:", err);
        }
    };

    const renderTree = (node: VaultTreeNode) => {
        return (
            <div key={node.path} className="tree-node">
                <div
                    className={`node-row ${selectedFile?.path === node.path ? 'active' : ''}`}
                    onClick={() => node.isDir ? null : handleSelectFile(node.path)}
                >
                    <span className="node-icon">
                        {node.isDir ? <Folder size={14} /> : <FileText size={14} />}
                    </span>
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
                <div className="empty-icon"><Settings size={48} /></div>
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
            {/* Left Sidebar: Topics & Tags */}
            <aside className="vault-sidebar">
                <div className="sidebar-section">
                    <h3>Topics</h3>
                    <div className="topic-list">
                        {topics.map(topic => (
                            <div
                                key={topic.name}
                                className={`topic-item ${selectedTopic?.name === topic.name && !activeTag ? 'active' : ''}`}
                                onClick={() => {
                                    setSelectedTopic(topic);
                                    setActiveTag(null); // Clear tag selection when picking a topic
                                }}
                            >
                                <span className="topic-icon">
                                    {selectedTopic?.name === topic.name && !activeTag ? <FolderOpen size={16} /> : <Folder size={16} />}
                                </span>
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
                                <button onClick={() => setIsCreatingTopic(false)}><X size={14} /></button>
                            </div>
                        </div>
                    ) : (
                        <button
                            className="add-topic-btn"
                            onClick={() => setIsCreatingTopic(true)}
                        >
                            <Plus size={14} /> New Topic
                        </button>
                    )}
                </div>

                <VaultTags onSelectTag={handleSelectTag} activeTag={activeTag} />

                <div className="sidebar-section file-tree-section">
                    <div className="section-header">
                        <h3>{activeTag ? `Tagged: #${activeTag}` : 'Vault Explorer'}</h3>
                        <button className="icon-btn" onClick={() => setShowGraph(true)} title="View Knowledge Graph (Cmd+G)">
                            <Maximize2 size={14} />
                        </button>
                    </div>
                    <div className="vault-file-tree">
                        {activeTag ? (
                            <div className="tag-file-list">
                                {tagFiles.length > 0 ? (
                                    tagFiles.map(file => (
                                        <div
                                            key={file.path}
                                            className={`node-row ${selectedFile?.path === file.path ? 'active' : ''}`}
                                            onClick={() => handleSelectFile(file.path)}
                                        >
                                            <span className="node-icon"><FileText size={14} /></span>
                                            <span className="node-name">{file.name}</span>
                                        </div>
                                    ))
                                ) : (
                                    <div className="empty-text">No files found with this tag</div>
                                )}
                            </div>
                        ) : (
                            tree && renderTree(tree)
                        )}
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
                                    // Basic wikilink rendering [[Page]] -> clickable
                                    const parts = line.split(/(\[\[.*?\]\])/g);
                                    if (parts.length > 1) {
                                        return (
                                            <p key={i}>
                                                {parts.map((part, idx) => {
                                                    if (part.startsWith('[[') && part.endsWith(']]')) {
                                                        const linkContent = part.slice(2, -2);
                                                        const [target, alias] = linkContent.split('|');
                                                        const displayText = alias || target;
                                                        // Simple navigation handler - assumes file exists in vault
                                                        // In a real app we'd need to resolve the path
                                                        // For now we just use the name and hope it matches (basic limitation)
                                                        // Improved logic: try to find file with this name in the tree?
                                                        // Simpler: Just make it look like a link
                                                        return (
                                                            <span
                                                                key={idx}
                                                                className="wikilink"
                                                                title={`Navigate to ${target}`}
                                                                onClick={(e) => {
                                                                    e.stopPropagation();
                                                                    handleWikilinkClick(target);
                                                                }}
                                                            >
                                                                {displayText}
                                                            </span>
                                                        );
                                                    }
                                                    return part;
                                                })}
                                            </p>
                                        );
                                    }
                                    return <p key={i}>{line}</p>;
                                })}
                            </div>
                        </div>
                    ) : (
                        <div className="vault-empty-state">
                            <div className="empty-icon"><FileText size={48} /></div>
                            <h2 className="empty-title">No File Selected</h2>
                            <p className="empty-desc">Select a meeting or note from the sidebar to preview its content.</p>
                        </div>
                    )}
                </div>
            </main>

            {/* Right Panel: Inspector & Actions */}
            <section className="vault-inspector">
                <div className="inspector-header">
                    <h3>{selectedFile ? 'File Details' : (selectedTopic ? selectedTopic.name : 'Topic Details')}</h3>
                    <p>Managed via noFriction</p>
                </div>

                <div className="action-card">
                    <h4>Actions</h4>
                    <button
                        className="action-btn primary"
                        disabled={!selectedTopic || isExportLoading}
                        onClick={handleOpenExportModal}
                    >
                        {isExportLoading ? <RefreshCcw className="spinning" size={16} /> : <Ship size={16} />}
                        {isExportLoading ? ' Exporting...' : ' Export Meeting'}
                    </button>
                    <button
                        className="action-btn"
                        disabled={!selectedTopic}
                        onClick={() => fileInputRef.current?.click()}
                    >
                        <Upload size={16} /> Upload File
                    </button>
                    <input
                        type="file"
                        ref={fileInputRef}
                        style={{ display: 'none' }}
                        onChange={handleUploadFile}
                    />
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

                {selectedFile && (
                    <BacklinksPanel filePath={selectedFile.path} onNavigate={handleSelectFile} />
                )}

                {!selectedFile && selectedTopic && (
                    <div className="inspector-section">
                        <h4>Linked Meetings</h4>
                        <div className="inspector-list">
                            {selectedTopic.meetings.length > 0 ? (
                                selectedTopic.meetings.map(m => (
                                    <div key={m} className="inspector-item" onClick={() => handleJumpToMeeting(m)}>
                                        <span className="item-icon">
                                            <FileIcon size={14} />
                                        </span>
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
                    <button className="action-btn" onClick={() => setShowGraph(true)}>
                        Graph View
                    </button>
                    <button className="action-btn" onClick={() => loadVaultData()}>
                        <RefreshCcw size={14} /> Refresh Vault
                    </button>
                </div>
            </section>

            {showGraph && (
                <VaultGraph
                    onClose={() => setShowGraph(false)}
                    onSelectNode={handleSelectFile}
                />
            )}
        </div>
    );
};
