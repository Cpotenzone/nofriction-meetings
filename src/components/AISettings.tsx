// noFriction Meetings - AI Settings Component
// Configuration for AI models and Ollama integration

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface OllamaModel {
    name: string;
    size: string;
    modified_at: string;
}

interface AIPreset {
    id: string;
    name: string;
    description: string;
    model: string;
    system_prompt: string;
    temperature: number;
}

export function AISettings() {
    const [ollamaAvailable, setOllamaAvailable] = useState<boolean | null>(null);
    const [models, setModels] = useState<OllamaModel[]>([]);
    const [presets, setPresets] = useState<AIPreset[]>([]);
    const [selectedModel, setSelectedModel] = useState<string>("llama3.2");
    const [isLoading, setIsLoading] = useState(true);
    const [ollamaUrl, setOllamaUrl] = useState("http://localhost:11434");

    useEffect(() => {
        checkOllamaStatus();
    }, []);

    const checkOllamaStatus = async () => {
        setIsLoading(true);
        try {
            const available = await invoke<boolean>("check_ollama");
            setOllamaAvailable(available);

            if (available) {
                const [modelList, presetList] = await Promise.all([
                    invoke<OllamaModel[]>("get_ollama_models"),
                    invoke<AIPreset[]>("get_ai_presets"),
                ]);
                setModels(modelList);
                setPresets(presetList);

                if (modelList.length > 0) {
                    setSelectedModel(modelList[0].name);
                }
            }
        } catch (err) {
            console.error("Failed to check Ollama:", err);
            setOllamaAvailable(false);
        } finally {
            setIsLoading(false);
        }
    };

    return (
        <div className="ai-settings">
            {/* Ollama Status */}
            <section className="settings-section">
                <h3>
                    <span className="icon">🤖</span>
                    Ollama AI
                </h3>

                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Status</span>
                        <span className="label-sub">Local AI inference via Ollama</span>
                    </div>
                    <div className="settings-control">
                        {isLoading ? (
                            <span className="status-badge loading">Checking...</span>
                        ) : ollamaAvailable ? (
                            <span className="status-badge success">✓ Connected</span>
                        ) : (
                            <span className="status-badge error">✗ Not Available</span>
                        )}
                    </div>
                </div>

                {!ollamaAvailable && !isLoading && (
                    <div className="ollama-install-prompt">
                        <p>Ollama is not running. Install and run Ollama to enable AI features.</p>
                        <div style={{ display: "flex", gap: "var(--spacing-md)", marginTop: "var(--spacing-md)" }}>
                            <a
                                href="https://ollama.ai"
                                target="_blank"
                                rel="noopener noreferrer"
                                className="btn btn-primary"
                            >
                                Download Ollama
                            </a>
                            <button className="btn btn-secondary" onClick={checkOllamaStatus}>
                                Retry Connection
                            </button>
                        </div>
                    </div>
                )}

                {ollamaAvailable && (
                    <>
                        <div className="settings-row">
                            <div className="settings-label">
                                <span className="label-main">Ollama URL</span>
                                <span className="label-sub">Usually http://localhost:11434</span>
                            </div>
                            <div className="settings-control">
                                <input
                                    type="text"
                                    className="settings-input"
                                    value={ollamaUrl}
                                    onChange={(e) => setOllamaUrl(e.target.value)}
                                />
                            </div>
                        </div>
                    </>
                )}
            </section>

            {/* Available Models */}
            {ollamaAvailable && models.length > 0 && (
                <section className="settings-section">
                    <h3>
                        <span className="icon">🧠</span>
                        Available Models ({models.length})
                    </h3>
                    <p style={{ fontSize: "0.875rem", color: "var(--text-secondary)", marginBottom: "var(--spacing-md)" }}>
                        Models installed in Ollama. Use <code>ollama pull model-name</code> to add more.
                    </p>

                    <div className="model-list">
                        {models.map((model) => (
                            <div
                                key={model.name}
                                className={`model-item ${selectedModel === model.name ? "selected" : ""}`}
                                onClick={() => setSelectedModel(model.name)}
                            >
                                <div className="model-name">{model.name}</div>
                                <div className="model-meta">
                                    <span className="model-size">{model.size}</span>
                                </div>
                            </div>
                        ))}
                    </div>
                </section>
            )}

            {/* AI Presets */}
            {ollamaAvailable && presets.length > 0 && (
                <section className="settings-section">
                    <h3>
                        <span className="icon">📋</span>
                        AI Presets
                    </h3>
                    <p style={{ fontSize: "0.875rem", color: "var(--text-secondary)", marginBottom: "var(--spacing-md)" }}>
                        Pre-configured AI modes for different meeting tasks.
                    </p>

                    <div className="preset-list">
                        {presets.map((preset) => (
                            <div key={preset.id} className="preset-item">
                                <div className="preset-header">
                                    <span className="preset-name">{preset.name}</span>
                                    <span className="preset-model">{preset.model}</span>
                                </div>
                                <p className="preset-description">{preset.description}</p>
                            </div>
                        ))}
                    </div>
                </section>
            )}
        </div>
    );
}
