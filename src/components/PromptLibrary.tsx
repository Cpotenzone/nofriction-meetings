// noFriction Meetings - Prompt Library Component
// Full prompt management with editor, models, and use cases

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

// Types matching Rust structs
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
    is_builtin: boolean;
    is_active: boolean;
    created_at: string;
    updated_at: string;
}

interface PromptCreate {
    name: string;
    description?: string;
    category: string;
    system_prompt: string;
    user_prompt_template?: string;
    model_id?: string;
    temperature?: number;
    max_tokens?: number;
}

interface ModelConfig {
    id: string;
    name: string;
    display_name: string;
    model_type: string;
    base_url: string;
    capabilities: string[];
    default_temperature: number;
    default_max_tokens: number;
    is_available: boolean;
    last_health_check: string | null;
    created_at: string;
}

interface UseCase {
    id: string;
    use_case: string;
    display_name: string;
    description: string | null;
    prompt_id: string | null;
    model_id: string | null;
    priority: number;
    conditions: string | null;
    is_active: boolean;
    created_at: string;
}

type Tab = "prompts" | "models" | "usecases";

export function PromptLibrary() {
    const [activeTab, setActiveTab] = useState<Tab>("prompts");
    const [prompts, setPrompts] = useState<Prompt[]>([]);
    const [models, setModels] = useState<ModelConfig[]>([]);
    const [useCases, setUseCases] = useState<UseCase[]>([]);
    const [selectedPrompt, setSelectedPrompt] = useState<Prompt | null>(null);
    const [isEditing, setIsEditing] = useState(false);
    const [isLoading, setIsLoading] = useState(true);
    const [categoryFilter, setCategoryFilter] = useState<string>("all");
    const [testInput, setTestInput] = useState("");
    const [testOutput, setTestOutput] = useState("");
    const [isTesting, setIsTesting] = useState(false);

    // Load data on mount
    useEffect(() => {
        loadData();
    }, []);

    const loadData = async () => {
        setIsLoading(true);
        try {
            const [promptList, modelList, useCaseList] = await Promise.all([
                invoke<Prompt[]>("list_prompts", { category: null }),
                invoke<ModelConfig[]>("list_model_configs"),
                invoke<UseCase[]>("list_use_cases"),
            ]);
            setPrompts(promptList);
            setModels(modelList);
            setUseCases(useCaseList);
        } catch (err) {
            console.error("Failed to load data:", err);
        } finally {
            setIsLoading(false);
        }
    };

    const refreshModels = async () => {
        try {
            const updated = await invoke<ModelConfig[]>("refresh_model_availability");
            setModels(updated);
        } catch (err) {
            console.error("Failed to refresh models:", err);
            alert("Failed to refresh model availability");
        }
    };

    const createPrompt = async (input: PromptCreate) => {
        try {
            const newPrompt = await invoke<Prompt>("create_prompt", { input });
            setPrompts([...prompts, newPrompt]);
            setSelectedPrompt(newPrompt);
            setIsEditing(false);
        } catch (err) {
            console.error("Failed to create prompt:", err);
            alert("Failed to create prompt");
        }
    };

    const updatePrompt = async (id: string, updates: Partial<Prompt>) => {
        try {
            const updated = await invoke<Prompt | null>("update_prompt", { id, updates });
            if (updated) {
                setPrompts(prompts.map(p => p.id === id ? updated : p));
                setSelectedPrompt(updated);
            }
        } catch (err) {
            console.error("Failed to update prompt:", err);
            alert("Failed to update prompt");
        }
    };

    const deletePrompt = async (id: string) => {
        if (!confirm("Delete this prompt?")) return;
        try {
            const deleted = await invoke<boolean>("delete_prompt", { id });
            if (deleted) {
                setPrompts(prompts.filter(p => p.id !== id));
                if (selectedPrompt?.id === id) {
                    setSelectedPrompt(null);
                }
            } else {
                alert("Cannot delete built-in prompts");
            }
        } catch (err) {
            console.error("Failed to delete prompt:", err);
        }
    };

    const duplicatePrompt = async (id: string) => {
        const name = prompt("Enter name for duplicate:");
        if (!name) return;
        try {
            const newPrompt = await invoke<Prompt | null>("duplicate_prompt", { id, newName: name });
            if (newPrompt) {
                setPrompts([...prompts, newPrompt]);
                setSelectedPrompt(newPrompt);
            }
        } catch (err) {
            console.error("Failed to duplicate prompt:", err);
        }
    };

    const testPrompt = async () => {
        if (!selectedPrompt || !testInput.trim()) return;
        setIsTesting(true);
        setTestOutput("");
        try {
            const result = await invoke<string>("test_prompt", {
                promptId: selectedPrompt.id,
                testInput: testInput.trim(),
            });
            setTestOutput(result);
        } catch (err) {
            setTestOutput(`Error: ${err}`);
        } finally {
            setIsTesting(false);
        }
    };

    const categories = [...new Set(prompts.map(p => p.category))].sort();
    const filteredPrompts = categoryFilter === "all"
        ? prompts
        : prompts.filter(p => p.category === categoryFilter);

    const getModelName = (modelId: string | null) => {
        if (!modelId) return "Default";
        const model = models.find(m => m.id === modelId);
        return model?.display_name || "Unknown";
    };

    if (isLoading) {
        return (
            <div className="prompt-library" style={{ padding: "var(--spacing-lg)", textAlign: "center" }}>
                Loading prompt library...
            </div>
        );
    }

    return (
        <div className="prompt-library" style={{ display: "flex", flexDirection: "column", height: "100%" }}>
            {/* Tab Navigation */}
            <div style={{ display: "flex", gap: "var(--spacing-sm)", marginBottom: "var(--spacing-md)", borderBottom: "1px solid var(--border)", paddingBottom: "var(--spacing-sm)" }}>
                <button
                    className={`tab ${activeTab === "prompts" ? "active" : ""}`}
                    onClick={() => setActiveTab("prompts")}
                >
                    🎯 Prompts ({prompts.length})
                </button>
                <button
                    className={`tab ${activeTab === "models" ? "active" : ""}`}
                    onClick={() => setActiveTab("models")}
                >
                    🤖 Models ({models.length})
                </button>
                <button
                    className={`tab ${activeTab === "usecases" ? "active" : ""}`}
                    onClick={() => setActiveTab("usecases")}
                >
                    🔗 Use Cases ({useCases.length})
                </button>
            </div>

            {/* Prompts Tab */}
            {activeTab === "prompts" && (
                <div style={{ display: "flex", gap: "var(--spacing-md)", flex: 1, minHeight: 0 }}>
                    {/* Prompt List */}
                    <div style={{ width: "300px", display: "flex", flexDirection: "column" }}>
                        <div style={{ display: "flex", gap: "var(--spacing-sm)", marginBottom: "var(--spacing-sm)" }}>
                            <select
                                value={categoryFilter}
                                onChange={(e) => setCategoryFilter(e.target.value)}
                                style={{ flex: 1, padding: "6px", borderRadius: "6px", border: "1px solid var(--border)", background: "var(--bg-secondary)" }}
                            >
                                <option value="all">All Categories</option>
                                {categories.map(cat => (
                                    <option key={cat} value={cat}>{cat}</option>
                                ))}
                            </select>
                            <button
                                className="btn btn-primary"
                                onClick={() => {
                                    setSelectedPrompt(null);
                                    setIsEditing(true);
                                }}
                                style={{ padding: "6px 12px" }}
                            >
                                + New
                            </button>
                        </div>
                        <div style={{ flex: 1, overflowY: "auto", display: "flex", flexDirection: "column", gap: "var(--spacing-xs)" }}>
                            {filteredPrompts.map(p => (
                                <div
                                    key={p.id}
                                    onClick={() => { setSelectedPrompt(p); setIsEditing(false); }}
                                    style={{
                                        padding: "var(--spacing-sm)",
                                        borderRadius: "8px",
                                        border: `1px solid ${selectedPrompt?.id === p.id ? "var(--primary)" : "var(--border)"}`,
                                        background: selectedPrompt?.id === p.id ? "var(--primary-bg)" : "var(--bg-secondary)",
                                        cursor: "pointer",
                                    }}
                                >
                                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                                        <span style={{ fontWeight: 500 }}>{p.name}</span>
                                        {p.is_builtin && (
                                            <span style={{ fontSize: "0.65rem", padding: "2px 6px", background: "var(--primary)", color: "white", borderRadius: "10px" }}>
                                                Built-in
                                            </span>
                                        )}
                                    </div>
                                    <div style={{ fontSize: "0.75rem", color: "var(--text-tertiary)", marginTop: "4px" }}>
                                        {p.category} • {getModelName(p.model_id)}
                                    </div>
                                </div>
                            ))}
                        </div>
                    </div>

                    {/* Prompt Editor */}
                    <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
                        {(selectedPrompt || isEditing) ? (
                            <PromptEditor
                                prompt={selectedPrompt}
                                models={models}
                                isNew={isEditing && !selectedPrompt}
                                onSave={async (data) => {
                                    if (selectedPrompt) {
                                        await updatePrompt(selectedPrompt.id, data);
                                    } else {
                                        await createPrompt(data as PromptCreate);
                                    }
                                    setIsEditing(false);
                                }}
                                onCancel={() => setIsEditing(false)}
                                onDelete={selectedPrompt ? () => deletePrompt(selectedPrompt.id) : undefined}
                                onDuplicate={selectedPrompt ? () => duplicatePrompt(selectedPrompt.id) : undefined}
                            />
                        ) : (
                            <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", color: "var(--text-tertiary)" }}>
                                Select a prompt to view or edit
                            </div>
                        )}

                        {/* Test Area */}
                        {selectedPrompt && !isEditing && (
                            <div style={{ marginTop: "var(--spacing-md)", padding: "var(--spacing-md)", background: "var(--bg-secondary)", borderRadius: "8px", border: "1px solid var(--border)" }}>
                                <h4 style={{ margin: 0, marginBottom: "var(--spacing-sm)" }}>🧪 Test Prompt</h4>
                                <div style={{ display: "flex", gap: "var(--spacing-sm)" }}>
                                    <input
                                        type="text"
                                        placeholder="Enter test input..."
                                        value={testInput}
                                        onChange={(e) => setTestInput(e.target.value)}
                                        style={{ flex: 1, padding: "8px", borderRadius: "6px", border: "1px solid var(--border)" }}
                                        onKeyDown={(e) => e.key === "Enter" && testPrompt()}
                                    />
                                    <button
                                        className="btn btn-primary"
                                        onClick={testPrompt}
                                        disabled={isTesting || !testInput.trim()}
                                    >
                                        {isTesting ? "Testing..." : "Test"}
                                    </button>
                                </div>
                                {testOutput && (
                                    <div style={{ marginTop: "var(--spacing-sm)", padding: "var(--spacing-sm)", background: "var(--bg-primary)", borderRadius: "6px", whiteSpace: "pre-wrap", fontSize: "0.875rem", maxHeight: "150px", overflow: "auto" }}>
                                        {testOutput}
                                    </div>
                                )}
                            </div>
                        )}
                    </div>
                </div>
            )}

            {/* Models Tab */}
            {activeTab === "models" && (
                <div>
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "var(--spacing-md)" }}>
                        <h3 style={{ margin: 0 }}>Model Configurations</h3>
                        <button className="btn btn-secondary" onClick={refreshModels}>
                            🔄 Refresh Availability
                        </button>
                    </div>
                    <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))", gap: "var(--spacing-md)" }}>
                        {models.map(model => (
                            <div
                                key={model.id}
                                style={{
                                    padding: "var(--spacing-md)",
                                    background: "var(--bg-secondary)",
                                    borderRadius: "12px",
                                    border: "1px solid var(--border)",
                                }}
                            >
                                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                                    <span style={{ fontWeight: 600 }}>{model.display_name}</span>
                                    <span style={{
                                        fontSize: "0.75rem",
                                        padding: "2px 8px",
                                        borderRadius: "10px",
                                        background: model.is_available ? "var(--success-bg)" : "var(--error-bg)",
                                        color: model.is_available ? "var(--success)" : "var(--error)",
                                    }}>
                                        {model.is_available ? "✓ Available" : "✗ Not Found"}
                                    </span>
                                </div>
                                <div style={{ fontSize: "0.75rem", color: "var(--text-tertiary)", margin: "8px 0" }}>
                                    <code>{model.name}</code>
                                </div>
                                <div style={{ display: "flex", gap: "4px", flexWrap: "wrap" }}>
                                    <span style={{ fontSize: "0.65rem", padding: "2px 6px", background: model.model_type === "vlm" ? "var(--info)" : "var(--primary)", color: "white", borderRadius: "8px" }}>
                                        {model.model_type.toUpperCase()}
                                    </span>
                                    {model.capabilities.slice(0, 3).map(cap => (
                                        <span key={cap} style={{ fontSize: "0.65rem", padding: "2px 6px", background: "var(--bg-tertiary)", borderRadius: "8px" }}>
                                            {cap}
                                        </span>
                                    ))}
                                </div>
                                <div style={{ fontSize: "0.7rem", color: "var(--text-tertiary)", marginTop: "8px" }}>
                                    Temp: {model.default_temperature} • Tokens: {model.default_max_tokens}
                                </div>
                            </div>
                        ))}
                    </div>
                </div>
            )}

            {/* Use Cases Tab */}
            {activeTab === "usecases" && (
                <div>
                    <h3 style={{ margin: "0 0 var(--spacing-md) 0" }}>Use Case Mappings</h3>
                    <p style={{ fontSize: "0.75rem", color: "var(--text-tertiary)", marginBottom: "var(--spacing-md)" }}>
                        Configure which prompt and model is used for each application feature.
                    </p>
                    <table style={{ width: "100%", borderCollapse: "collapse" }}>
                        <thead>
                            <tr style={{ borderBottom: "2px solid var(--border)" }}>
                                <th style={{ textAlign: "left", padding: "8px" }}>Use Case</th>
                                <th style={{ textAlign: "left", padding: "8px" }}>Prompt</th>
                                <th style={{ textAlign: "left", padding: "8px" }}>Model</th>
                                <th style={{ textAlign: "center", padding: "8px" }}>Active</th>
                            </tr>
                        </thead>
                        <tbody>
                            {useCases.map(uc => (
                                <tr key={uc.id} style={{ borderBottom: "1px solid var(--border)" }}>
                                    <td style={{ padding: "12px 8px" }}>
                                        <div style={{ fontWeight: 500 }}>{uc.display_name}</div>
                                        <div style={{ fontSize: "0.7rem", color: "var(--text-tertiary)" }}>{uc.use_case}</div>
                                    </td>
                                    <td style={{ padding: "12px 8px" }}>
                                        <select
                                            value={uc.prompt_id || ""}
                                            onChange={async (e) => {
                                                await invoke("update_use_case_mapping", {
                                                    useCase: uc.use_case,
                                                    promptId: e.target.value || null,
                                                    modelId: uc.model_id,
                                                });
                                                loadData();
                                            }}
                                            style={{ padding: "4px", borderRadius: "4px", border: "1px solid var(--border)" }}
                                        >
                                            <option value="">Select Prompt</option>
                                            {prompts.map(p => (
                                                <option key={p.id} value={p.id}>{p.name}</option>
                                            ))}
                                        </select>
                                    </td>
                                    <td style={{ padding: "12px 8px" }}>
                                        <select
                                            value={uc.model_id || ""}
                                            onChange={async (e) => {
                                                await invoke("update_use_case_mapping", {
                                                    useCase: uc.use_case,
                                                    promptId: uc.prompt_id,
                                                    modelId: e.target.value || null,
                                                });
                                                loadData();
                                            }}
                                            style={{ padding: "4px", borderRadius: "4px", border: "1px solid var(--border)" }}
                                        >
                                            <option value="">Select Model</option>
                                            {models.map(m => (
                                                <option key={m.id} value={m.id}>{m.display_name}</option>
                                            ))}
                                        </select>
                                    </td>
                                    <td style={{ padding: "12px 8px", textAlign: "center" }}>
                                        {uc.is_active ? "✓" : "✗"}
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
            )}
        </div>
    );
}

// Prompt Editor Sub-component
interface PromptEditorProps {
    prompt: Prompt | null;
    models: ModelConfig[];
    isNew: boolean;
    onSave: (data: Partial<Prompt> | PromptCreate) => Promise<void>;
    onCancel: () => void;
    onDelete?: () => void;
    onDuplicate?: () => void;
}

function PromptEditor({ prompt, models, isNew, onSave, onCancel, onDelete, onDuplicate }: PromptEditorProps) {
    const [name, setName] = useState(prompt?.name || "");
    const [description, setDescription] = useState(prompt?.description || "");
    const [category, setCategory] = useState(prompt?.category || "general");
    const [systemPrompt, setSystemPrompt] = useState(prompt?.system_prompt || "");
    const [modelId, setModelId] = useState(prompt?.model_id || "");
    const [temperature, setTemperature] = useState(prompt?.temperature || 0.5);
    const [isSaving, setIsSaving] = useState(false);

    // Reset when prompt changes
    useEffect(() => {
        setName(prompt?.name || "");
        setDescription(prompt?.description || "");
        setCategory(prompt?.category || "general");
        setSystemPrompt(prompt?.system_prompt || "");
        setModelId(prompt?.model_id || "");
        setTemperature(prompt?.temperature || 0.5);
    }, [prompt]);

    const handleSave = async () => {
        if (!name.trim() || !systemPrompt.trim()) {
            alert("Name and System Prompt are required");
            return;
        }
        setIsSaving(true);
        try {
            await onSave({
                name: name.trim(),
                description: description.trim() || null,
                category,
                system_prompt: systemPrompt,
                model_id: modelId || null,
                temperature,
            });
        } finally {
            setIsSaving(false);
        }
    };

    return (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--spacing-md)", flex: 1 }}>
            {/* Header */}
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                <h3 style={{ margin: 0 }}>{isNew ? "Create New Prompt" : `Edit: ${prompt?.name}`}</h3>
                <div style={{ display: "flex", gap: "var(--spacing-sm)" }}>
                    {onDuplicate && !isNew && (
                        <button className="btn btn-secondary" onClick={onDuplicate}>📋 Duplicate</button>
                    )}
                    {onDelete && !prompt?.is_builtin && (
                        <button className="btn btn-secondary" onClick={onDelete} style={{ color: "var(--error)" }}>🗑️ Delete</button>
                    )}
                </div>
            </div>

            {/* Form */}
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "var(--spacing-md)" }}>
                <div>
                    <label style={{ fontSize: "0.75rem", color: "var(--text-secondary)" }}>Name *</label>
                    <input
                        type="text"
                        value={name}
                        onChange={(e) => setName(e.target.value)}
                        disabled={prompt?.is_builtin}
                        style={{ width: "100%", padding: "8px", borderRadius: "6px", border: "1px solid var(--border)", marginTop: "4px" }}
                    />
                </div>
                <div>
                    <label style={{ fontSize: "0.75rem", color: "var(--text-secondary)" }}>Category</label>
                    <select
                        value={category}
                        onChange={(e) => setCategory(e.target.value)}
                        style={{ width: "100%", padding: "8px", borderRadius: "6px", border: "1px solid var(--border)", marginTop: "4px" }}
                    >
                        <option value="general">General</option>
                        <option value="meeting">Meeting</option>
                        <option value="vlm">VLM (Vision)</option>
                        <option value="coding">Coding</option>
                        <option value="writing">Writing</option>
                    </select>
                </div>
            </div>

            <div>
                <label style={{ fontSize: "0.75rem", color: "var(--text-secondary)" }}>Description</label>
                <input
                    type="text"
                    value={description}
                    onChange={(e) => setDescription(e.target.value)}
                    placeholder="Brief description of what this prompt does"
                    style={{ width: "100%", padding: "8px", borderRadius: "6px", border: "1px solid var(--border)", marginTop: "4px" }}
                />
            </div>

            <div style={{ flex: 1, display: "flex", flexDirection: "column" }}>
                <label style={{ fontSize: "0.75rem", color: "var(--text-secondary)" }}>System Prompt *</label>
                <textarea
                    value={systemPrompt}
                    onChange={(e) => setSystemPrompt(e.target.value)}
                    disabled={prompt?.is_builtin}
                    placeholder="Enter the system prompt that defines the AI's behavior..."
                    style={{
                        flex: 1,
                        minHeight: "150px",
                        padding: "12px",
                        borderRadius: "6px",
                        border: "1px solid var(--border)",
                        marginTop: "4px",
                        fontFamily: "monospace",
                        fontSize: "0.875rem",
                        resize: "vertical",
                    }}
                />
            </div>

            <div style={{ display: "grid", gridTemplateColumns: "2fr 1fr", gap: "var(--spacing-md)" }}>
                <div>
                    <label style={{ fontSize: "0.75rem", color: "var(--text-secondary)" }}>Model</label>
                    <select
                        value={modelId}
                        onChange={(e) => setModelId(e.target.value)}
                        style={{ width: "100%", padding: "8px", borderRadius: "6px", border: "1px solid var(--border)", marginTop: "4px" }}
                    >
                        <option value="">Default (llama3.2)</option>
                        {models.map(m => (
                            <option key={m.id} value={m.id}>
                                {m.display_name} {m.is_available ? "✓" : "(not installed)"}
                            </option>
                        ))}
                    </select>
                </div>
                <div>
                    <label style={{ fontSize: "0.75rem", color: "var(--text-secondary)" }}>Temperature: {temperature.toFixed(2)}</label>
                    <input
                        type="range"
                        min="0"
                        max="1"
                        step="0.05"
                        value={temperature}
                        onChange={(e) => setTemperature(parseFloat(e.target.value))}
                        style={{ width: "100%", marginTop: "8px" }}
                    />
                </div>
            </div>

            {/* Actions */}
            <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--spacing-sm)", paddingTop: "var(--spacing-md)", borderTop: "1px solid var(--border)" }}>
                <button className="btn btn-secondary" onClick={onCancel}>Cancel</button>
                <button
                    className="btn btn-primary"
                    onClick={handleSave}
                    disabled={isSaving || prompt?.is_builtin}
                >
                    {isSaving ? "Saving..." : isNew ? "Create" : "Save Changes"}
                </button>
            </div>

            {prompt?.is_builtin && (
                <div style={{ padding: "var(--spacing-sm)", background: "var(--warning-bg)", borderRadius: "6px", fontSize: "0.75rem", color: "var(--warning)" }}>
                    ⚠️ This is a built-in prompt. Duplicate it to create an editable copy.
                </div>
            )}
        </div>
    );
}
