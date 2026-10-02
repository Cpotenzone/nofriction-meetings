import React, { useState, useEffect, useCallback } from 'react';
import {
    listPrompts,
    updatePrompt,
    deletePrompt,
    duplicatePrompt,
    getActiveTheme,
    testPrompt,
    type PromptRecord,
    type PromptUpdate,
} from '../../../lib/tauri';
import './PromptStudio.css';
import { friendlyAiError } from '../../../lib/ai';
import { AiSetupNotice, useAiStatus } from '../../AiSetupNotice';

type CategoryFilter = 'all' | 'intelligence' | 'meeting' | 'vlm';
const CATEGORIES: { key: CategoryFilter; label: string }[] = [
    { key: 'all', label: '✦  ALL' },
    { key: 'intelligence', label: 'INTELLIGENCE' },
    { key: 'meeting', label: '📋  MEETING' },
    { key: 'vlm', label: '👁  VLM' },
];

const PERSONAS = [
    { key: '', label: 'All Personas' },
    { key: 'prospecting', label: '🎯 Prospecting' },
    { key: 'fundraising', label: '💰 Fundraising' },
    { key: 'product_dev', label: '🛠 Product Dev' },
    { key: 'admin', label: '📁 Admin' },
    { key: 'personal', label: '👤 Personal' },
];

export const PromptStudio: React.FC = () => {
    const [prompts, setPrompts] = useState<PromptRecord[]>([]);
    const [selectedId, setSelectedId] = useState<string | null>(null);
    const [categoryFilter, setCategoryFilter] = useState<CategoryFilter>('all');
    const [personaFilter, setPersonaFilter] = useState('');
    const [activeTheme, setActiveTheme] = useState('personal');
    const [loading, setLoading] = useState(true);
    const [status, setStatus] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

    // Draft state for editing
    const [draft, setDraft] = useState<Partial<PromptRecord>>({});
    const [isDirty, setIsDirty] = useState(false);

    // Test prompt state
    const [testInput, setTestInput] = useState('');
    const [testResponse, setTestResponse] = useState('');
    const [testing, setTesting] = useState(false);
    const [showTest, setShowTest] = useState(false);
    const { configured: aiConfigured } = useAiStatus();

    // Load prompts
    const loadPrompts = useCallback(async () => {
        try {
            setLoading(true);
            const category = categoryFilter === 'all' ? undefined : categoryFilter;
            const all = await listPrompts(category);
            setPrompts(all);
        } catch (err) {
            console.error('Failed to load prompts:', err);
            setStatus({ type: 'error', message: `Failed to load prompts: ${err}` });
        } finally {
            setLoading(false);
        }
    }, [categoryFilter]);

    // Load active theme
    useEffect(() => {
        getActiveTheme().then(setActiveTheme).catch(() => { });
    }, []);

    useEffect(() => {
        loadPrompts();
    }, [loadPrompts]);

    // Clear status after 3s
    useEffect(() => {
        if (status) {
            const t = setTimeout(() => setStatus(null), 3000);
            return () => clearTimeout(t);
        }
    }, [status]);

    // Select a prompt
    const handleSelect = (prompt: PromptRecord) => {
        if (isDirty && selectedId) {
            const proceed = window.confirm('Discard unsaved changes?');
            if (!proceed) return;
        }
        setSelectedId(prompt.id);
        setDraft({ ...prompt });
        setIsDirty(false);
    };

    // Update draft field
    const updateDraft = (field: keyof PromptRecord, value: string | number | boolean) => {
        setDraft(prev => ({ ...prev, [field]: value }));
        setIsDirty(true);
    };

    // Save
    const handleSave = async () => {
        if (!selectedId || !draft) return;
        try {
            const updates: PromptUpdate = {
                name: draft.name,
                description: draft.description ?? undefined,
                system_prompt: draft.system_prompt,
                temperature: draft.temperature,
                is_active: draft.is_active,
            };
            await updatePrompt(selectedId, updates);
            setIsDirty(false);
            setStatus({ type: 'success', message: 'Prompt saved successfully' });
            loadPrompts();
        } catch (err) {
            setStatus({ type: 'error', message: `Save failed: ${err}` });
        }
    };

    // Delete
    const handleDelete = async () => {
        if (!selectedId) return;
        const confirmed = window.confirm('Delete this prompt? This cannot be undone.');
        if (!confirmed) return;
        try {
            await deletePrompt(selectedId);
            setSelectedId(null);
            setDraft({});
            setIsDirty(false);
            setStatus({ type: 'success', message: 'Prompt deleted' });
            loadPrompts();
        } catch (err) {
            setStatus({ type: 'error', message: `Delete failed: ${err}` });
        }
    };

    // Duplicate
    const handleDuplicate = async () => {
        if (!selectedId || !draft.name) return;
        try {
            const newPrompt = await duplicatePrompt(selectedId, `${draft.name}_copy`);
            if (newPrompt) {
                setStatus({ type: 'success', message: `Duplicated as "${newPrompt.name}"` });
                loadPrompts();
                setSelectedId(newPrompt.id);
                setDraft({ ...newPrompt });
                setIsDirty(false);
            }
        } catch (err) {
            setStatus({ type: 'error', message: `Duplicate failed: ${err}` });
        }
    };

    // Test prompt
    const handleTest = async () => {
        if (!selectedId || !testInput.trim()) return;
        try {
            setTesting(true);
            setTestResponse('');
            const response = await testPrompt(selectedId, testInput);
            setTestResponse(response);
        } catch (err) {
            // No provider → "Add an AI key in Settings → AI Engine…", never a raw code
            setTestResponse(friendlyAiError(err));
        } finally {
            setTesting(false);
        }
    };

    // Filter prompts by persona
    const filteredPrompts = personaFilter
        ? prompts.filter(p => p.theme === personaFilter || !p.theme)
        : prompts;

    const selected = draft as PromptRecord | undefined;

    return (
        <div className="prompt-studio">
            <div className="prompt-header">
                <h2>Prompt Studio</h2>
                <p>Master prompts powering Genie, Reports, Catch-Up, and Live Intelligence — customize per persona</p>
            </div>

            {status && (
                <div className={`prompt-status ${status.type}`}>
                    {status.message}
                </div>
            )}

            <div className="prompt-controls">
                <div className="prompt-category-tabs">
                    {CATEGORIES.map(cat => (
                        <button
                            key={cat.key}
                            className={`prompt-cat-tab ${categoryFilter === cat.key ? 'active' : ''}`}
                            onClick={() => setCategoryFilter(cat.key)}
                        >
                            {cat.label}
                        </button>
                    ))}
                </div>

                <select
                    className="prompt-persona-select"
                    value={personaFilter}
                    onChange={e => setPersonaFilter(e.target.value)}
                >
                    {PERSONAS.map(p => (
                        <option key={p.key} value={p.key}>{p.label}</option>
                    ))}
                </select>
            </div>

            <div className="prompt-body">
                {/* Left: Prompt list */}
                <div className="prompt-list-panel">
                    {loading ? (
                        <div className="prompt-loading">Loading prompts</div>
                    ) : filteredPrompts.length === 0 ? (
                        <div className="prompt-loading" style={{ animation: 'none' }}>No prompts found</div>
                    ) : (
                        filteredPrompts.map(prompt => (
                            <div
                                key={prompt.id}
                                className={`prompt-list-item ${selectedId === prompt.id ? 'selected' : ''} ${!prompt.is_active ? 'inactive' : ''}`}
                                onClick={() => handleSelect(prompt)}
                            >
                                <div className="prompt-item-name">
                                    {prompt.name.replace(/_/g, ' ')}
                                    {prompt.theme === activeTheme && (
                                        <span className="prompt-badge theme" title="Active persona">●</span>
                                    )}
                                </div>
                                <div className="prompt-item-meta">
                                    <span className={`prompt-badge ${prompt.category}`}>
                                        {prompt.category}
                                    </span>
                                    {prompt.theme && (
                                        <span className="prompt-badge theme">{prompt.theme}</span>
                                    )}
                                </div>
                                {prompt.description && (
                                    <div className="prompt-item-desc">{prompt.description}</div>
                                )}
                            </div>
                        ))
                    )}
                </div>

                {/* Right: Editor */}
                <div className="prompt-editor-panel">
                    {!selected?.id ? (
                        <div className="prompt-editor-empty">Select a prompt to edit</div>
                    ) : (
                        <>
                            <div className="prompt-editor-header">
                                <div className="prompt-editor-title">
                                    {selected.name?.replace(/_/g, ' ')}
                                    {selected.is_builtin && (
                                        <span className="prompt-badge theme" style={{ marginLeft: '0.5rem' }}>BUILTIN</span>
                                    )}
                                </div>
                                <div className="prompt-editor-actions">
                                    <button className="prompt-action-btn" onClick={handleDuplicate}>
                                        ⧉ Duplicate
                                    </button>
                                    {!selected.is_builtin && (
                                        <button className="prompt-action-btn delete" onClick={handleDelete}>
                                            ✕ Delete
                                        </button>
                                    )}
                                    <button
                                        className={`prompt-action-btn ${isDirty ? 'save' : ''}`}
                                        onClick={handleSave}
                                        disabled={!isDirty}
                                    >
                                        ✓ Save
                                    </button>
                                </div>
                            </div>

                            <div className="prompt-editor-fields">
                                <div className="prompt-field">
                                    <label>Display Name</label>
                                    <input
                                        type="text"
                                        value={draft.name ?? ''}
                                        onChange={e => updateDraft('name', e.target.value)}
                                    />
                                </div>

                                <div className="prompt-field">
                                    <label>Description</label>
                                    <input
                                        type="text"
                                        value={draft.description ?? ''}
                                        onChange={e => updateDraft('description', e.target.value)}
                                    />
                                </div>

                                <div className="prompt-field">
                                    <label>System Prompt</label>
                                    <textarea
                                        value={draft.system_prompt ?? ''}
                                        onChange={e => updateDraft('system_prompt', e.target.value)}
                                        rows={12}
                                    />
                                </div>

                                <div className="prompt-field">
                                    <label>Temperature</label>
                                    <div className="prompt-slider-row">
                                        <input
                                            type="range"
                                            min="0"
                                            max="1"
                                            step="0.05"
                                            value={draft.temperature ?? 0.5}
                                            onChange={e => updateDraft('temperature', parseFloat(e.target.value))}
                                        />
                                        <span className="prompt-slider-value">{(draft.temperature ?? 0.5).toFixed(2)}</span>
                                    </div>
                                </div>

                                <div className="prompt-field">
                                    <label>Active</label>
                                    <div className="prompt-toggle-row">
                                        <label className="prompt-toggle">
                                            <input
                                                type="checkbox"
                                                checked={draft.is_active ?? true}
                                                onChange={e => updateDraft('is_active', e.target.checked)}
                                            />
                                            <span className="prompt-toggle-slider" />
                                        </label>
                                        <span className="prompt-toggle-label">
                                            {draft.is_active ? 'Prompt is active and will be used' : 'Prompt is disabled'}
                                        </span>
                                    </div>
                                </div>

                                <div className="prompt-field">
                                    <label>Meta</label>
                                    <div className="prompt-item-meta" style={{ gap: '0.75rem' }}>
                                        <span style={{ fontSize: '0.75rem', color: 'rgba(255,255,255,0.4)' }}>
                                            Category: <strong style={{ color: 'rgba(255,255,255,0.7)' }}>{selected.category}</strong>
                                        </span>
                                        <span style={{ fontSize: '0.75rem', color: 'rgba(255,255,255,0.4)' }}>
                                            Theme: <strong style={{ color: 'rgba(255,255,255,0.7)' }}>{selected.theme ?? 'None'}</strong>
                                        </span>
                                        <span style={{ fontSize: '0.75rem', color: 'rgba(255,255,255,0.4)' }}>
                                            Version: <strong style={{ color: 'rgba(255,255,255,0.7)' }}>v{selected.version}</strong>
                                        </span>
                                    </div>
                                </div>

                                {/* Test Panel */}
                                <div className="prompt-field">
                                    <label>
                                        Test
                                        <button
                                            className="prompt-action-btn"
                                            style={{ marginLeft: '0.5rem', padding: '0.15rem 0.4rem', fontSize: '0.65rem' }}
                                            onClick={() => setShowTest(!showTest)}
                                        >
                                            {showTest ? '▲ Hide' : '▼ Show'}
                                        </button>
                                    </label>
                                    {showTest && (
                                        <div className="prompt-test-panel">
                                            {aiConfigured === false && <AiSetupNotice feature="Prompt tests" compact />}
                                            <textarea
                                                className="prompt-test-input"
                                                value={testInput}
                                                onChange={e => setTestInput(e.target.value)}
                                                placeholder="Enter test input to run against this prompt..."
                                                rows={3}
                                            />
                                            <button
                                                className={`prompt-action-btn save ${testing ? '' : ''}`}
                                                onClick={handleTest}
                                                disabled={testing || !testInput.trim() || aiConfigured === false}
                                                style={{ alignSelf: 'flex-start' }}
                                            >
                                                {testing ? '⏳ Running...' : '▶ Run Test'}
                                            </button>
                                            {testResponse && (
                                                <div className="prompt-test-response">
                                                    <label>Response</label>
                                                    <pre>{testResponse}</pre>
                                                </div>
                                            )}
                                        </div>
                                    )}
                                </div>
                            </div>
                        </>
                    )}
                </div>
            </div>
        </div>
    );
};

