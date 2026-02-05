// Phase 2: Prompt Browser Component
// Displays theme-specific prompts with filtering and version history

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface Prompt {
    id: string;
    name: string;
    description: string | null;
    category: string;
    system_prompt: string;
    user_prompt_template: string | null;
    model_id: string | null;
    temperature: number;
    max_tokens: number | null;
    theme: string | null;
    version: number;
    is_builtin: boolean;
    is_active: boolean;
    created_at: string;
    updated_at: string;
}

const THEMES = [
    { id: 'all', name: 'All Themes', color: '#6366f1' },
    { id: 'prospecting', name: 'Prospecting', color: '#10b981' },
    { id: 'fundraising', name: 'Fundraising', color: '#f59e0b' },
    { id: 'product_dev', name: 'Product Development', color: '#3b82f6' },
    { id: 'admin', name: 'Admin', color: '#8b5cf6' },
    { id: 'personal', name: 'Personal', color: '#ec4899' },
];

export default function PromptBrowser() {
    const [prompts, setPrompts] = useState<Prompt[]>([]);
    const [selectedTheme, setSelectedTheme] = useState('all');
    const [selectedPrompt, setSelectedPrompt] = useState<Prompt | null>(null);
    const [versions, setVersions] = useState<Prompt[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        loadPrompts();
    }, [selectedTheme]);

    const loadPrompts = async () => {
        try {
            setLoading(true);
            setError(null);

            let results: Prompt[];
            if (selectedTheme === 'all') {
                results = await invoke<Prompt[]>('list_prompts', { category: null });
                // Filter to only theme-specific prompts
                results = results.filter(p => p.theme !== null);
            } else {
                results = await invoke<Prompt[]>('list_prompts_by_theme', { theme: selectedTheme });
            }

            setPrompts(results);
        } catch (err) {
            console.error('Failed to load prompts:', err);
            setError(`Failed to load prompts: ${err}`);
        } finally {
            setLoading(false);
        }
    };

    const loadVersions = async (prompt: Prompt) => {
        try {
            const results = await invoke<Prompt[]>('get_prompt_versions', {
                name: prompt.name,
                theme: prompt.theme,
            });
            setVersions(results);
        } catch (err) {
            console.error('Failed to load versions:', err);
            setVersions([]);
        }
    };

    const openPromptDetail = async (prompt: Prompt) => {
        setSelectedPrompt(prompt);
        await loadVersions(prompt);
    };

    const closeModal = () => {
        setSelectedPrompt(null);
        setVersions([]);
    };

    const filteredPrompts = prompts.filter(p =>
        selectedTheme === 'all' || p.theme === selectedTheme
    );

    const getThemeColor = (themeId: string | null) => {
        const theme = THEMES.find(t => t.id === themeId);
        return theme?.color || '#6366f1';
    };

    return (
        <div className="prompt-browser">
            {/* Header */}
            <div className="prompt-browser-header">
                <div>
                    <h2>Prompt Management</h2>
                    <p className="subtitle">Browse and view theme-specific prompts for VLM analysis</p>
                </div>
            </div>

            {/* Theme Selector */}
            <div className="theme-selector-wrapper">
                <label htmlFor="theme-select">Filter by Theme:</label>
                <select
                    id="theme-select"
                    value={selectedTheme}
                    onChange={(e) => setSelectedTheme(e.target.value)}
                    className="theme-select"
                >
                    {THEMES.map(theme => (
                        <option key={theme.id} value={theme.id}>
                            {theme.name}
                        </option>
                    ))}
                </select>
            </div>

            {/* Loading State */}
            {loading && (
                <div className="loading-state">
                    <div className="spinner"></div>
                    <p>Loading prompts...</p>
                </div>
            )}

            {/* Error State */}
            {error && (
                <div className="error-state">
                    <p>{error}</p>
                    <button onClick={loadPrompts} className="btn-primary">Retry</button>
                </div>
            )}

            {/* Prompts Grid */}
            {!loading && !error && (
                <div className="prompts-grid">
                    {filteredPrompts.length === 0 ? (
                        <div className="empty-state">
                            <p>No prompts found for this theme.</p>
                            <p className="hint">Check database initialization or switch themes.</p>
                        </div>
                    ) : (
                        filteredPrompts.map(prompt => (
                            <div
                                key={prompt.id}
                                className="prompt-card"
                                onClick={() => openPromptDetail(prompt)}
                                style={{ borderLeftColor: getThemeColor(prompt.theme) }}
                            >
                                <div className="prompt-card-header">
                                    <h3>{prompt.name}</h3>
                                    <span className="version-badge">v{prompt.version}</span>
                                </div>

                                {prompt.description && (
                                    <p className="prompt-description">{prompt.description}</p>
                                )}

                                <div className="prompt-meta">
                                    <span className="theme-tag" style={{ backgroundColor: getThemeColor(prompt.theme) + '20', color: getThemeColor(prompt.theme) }}>
                                        {THEMES.find(t => t.id === prompt.theme)?.name || prompt.theme}
                                    </span>
                                    <span className="category-tag">{prompt.category}</span>
                                </div>
                            </div>
                        ))
                    )}
                </div>
            )}

            {/* Prompt Detail Modal */}
            {selectedPrompt && (
                <div className="modal-overlay" onClick={closeModal}>
                    <div className="modal-content" onClick={(e) => e.stopPropagation()}>
                        <div className="modal-header">
                            <div>
                                <h2>{selectedPrompt.name}</h2>
                                <p className="modal-subtitle">Version {selectedPrompt.version}</p>
                            </div>
                            <button onClick={closeModal} className="modal-close" aria-label="Close">
                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                                </svg>
                            </button>
                        </div>

                        <div className="modal-body">
                            {selectedPrompt.description && (
                                <div className="prompt-section">
                                    <h4>Description</h4>
                                    <p>{selectedPrompt.description}</p>
                                </div>
                            )}

                            <div className="prompt-section">
                                <h4>System Prompt</h4>
                                <pre className="prompt-content">{selectedPrompt.system_prompt}</pre>
                            </div>

                            <div className="prompt-meta-grid">
                                <div>
                                    <label>Theme</label>
                                    <span>{THEMES.find(t => t.id === selectedPrompt.theme)?.name || selectedPrompt.theme}</span>
                                </div>
                                <div>
                                    <label>Category</label>
                                    <span>{selectedPrompt.category}</span>
                                </div>
                                <div>
                                    <label>Temperature</label>
                                    <span>{selectedPrompt.temperature}</span>
                                </div>
                                <div>
                                    <label>Built-in</label>
                                    <span>{selectedPrompt.is_builtin ? 'Yes' : 'No'}</span>
                                </div>
                            </div>

                            {versions.length > 1 && (
                                <div className="prompt-section">
                                    <h4>Version History ({versions.length} versions)</h4>
                                    <div className="version-list">
                                        {versions.map(v => (
                                            <div
                                                key={v.id}
                                                className={`version-item ${v.id === selectedPrompt.id ? 'active' : ''}`}
                                            >
                                                <span className="version-number">v{v.version}</span>
                                                <span className="version-date">
                                                    {new Date(v.created_at).toLocaleDateString()}
                                                </span>
                                                {v.id === selectedPrompt.id && <span className="current-badge">Current</span>}
                                            </div>
                                        ))}
                                    </div>
                                </div>
                            )}
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}
