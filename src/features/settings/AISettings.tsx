// noFriction Meetings - AI Settings Component
// Configuration for AI models and Ollama integration

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface OllamaModel {
    name: string;
    size: string;
    modified_at: string;
}



export function AISettings() {
    const [aiProvider, setAiProvider] = useState<"local" | "remote">("remote");
    const [ollamaAvailable, setOllamaAvailable] = useState<boolean | null>(null);
    const [models, setModels] = useState<OllamaModel[]>([]);
    const [selectedModel, setSelectedModel] = useState<string>("llama3.2");
    const [isLoading, setIsLoading] = useState(false);
    const [ollamaUrl, setOllamaUrl] = useState("http://localhost:11434");

    // Remote settings
    const [remoteUrl, setRemoteUrl] = useState("https://api.openai.com/v1");
    const [remoteKey, setRemoteKey] = useState("");

    // Load settings on mount
    useEffect(() => {
        loadSettings();
    }, []);

    // Check Ollama only if selected
    useEffect(() => {
        if (aiProvider === "local") {
            checkOllamaStatus();
        }
    }, [aiProvider]);

    const loadSettings = async () => {
        try {
            const [provider, url, key] = await invoke<[string, string | null, string | null]>("get_ai_provider_settings");
            // Basic validation to prevent invalid state
            if (provider === "local" || provider === "remote") {
                setAiProvider(provider);
            }
            if (url) setRemoteUrl(url);
            if (key) setRemoteKey(key);
        } catch (err) {
            console.error("Failed to load AI settings:", err);
        }
    };

    const saveSettings = async (newProvider?: string, newUrl?: string, newKey?: string) => {
        try {
            await invoke("set_ai_provider_settings", {
                provider: newProvider || aiProvider,
                url: newUrl !== undefined ? newUrl : null,
                key: newKey !== undefined ? newKey : null
            });
        } catch (err) {
            console.error("Failed to save AI settings:", err);
        }
    };

    // Auto-save when switching provider
    const handleProviderChange = (provider: "local" | "remote") => {
        setAiProvider(provider);
        saveSettings(provider);
    };

    const checkOllamaStatus = async () => {
        setIsLoading(true);
        try {
            const available = await invoke<boolean>("check_ollama");
            setOllamaAvailable(available);

            if (available) {
                const modelList = await invoke<OllamaModel[]>("get_ollama_models");
                setModels(modelList);

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
            <section className="settings-section">
                <h3>
                    <span className="icon">🧠</span>
                    AI Provider
                </h3>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Select Provider</span>
                        <span className="label-sub">Choose where AI processing happens</span>
                    </div>
                    <div className="settings-control">
                        <div className="provider-toggle"
                            style={{ display: 'flex', gap: '8px', background: 'var(--bg-main)', padding: '4px', borderRadius: '8px', border: '1px solid var(--border-color)' }}>
                            <button
                                className={`btn ${aiProvider === 'local' ? 'btn-primary' : 'btn-ghost'}`}
                                onClick={() => handleProviderChange('local')}
                                style={{ flex: 1, justifyContent: 'center' }}
                            >
                                Local (Ollama)
                            </button>
                            <button
                                className={`btn ${aiProvider === 'remote' ? 'btn-primary' : 'btn-ghost'}`}
                                onClick={() => handleProviderChange('remote')}
                                style={{ flex: 1, justifyContent: 'center' }}
                            >
                                Remote / Cloud
                            </button>
                        </div>
                    </div>
                </div>
            </section>

            {aiProvider === "local" ? (
                <>
                    {/* Ollama Status */}
                    <section className="settings-section">
                        <h3>
                            <span className="icon">🤖</span>
                            Ollama Configuration
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
                                <p>Ollama is not running. Install/Run Ollama or switch to Remote provider.</p>
                                <div style={{ display: "flex", gap: "var(--spacing-md)", marginTop: "var(--spacing-md)" }}>
                                    <a
                                        href="https://ollama.ai"
                                        target="_blank"
                                        rel="noopener noreferrer"
                                        className="btn btn-secondary"
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
                            <div className="settings-row">
                                <div className="settings-label">
                                    <span className="label-main">Ollama URL</span>
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
                        )}
                    </section>

                    {/* Available Models (Only show if Ollama active) */}
                    {ollamaAvailable && models.length > 0 && (
                        <section className="settings-section">
                            <h3>Available Models ({models.length})</h3>
                            <div className="model-list">
                                {models.map((model) => (
                                    <div
                                        key={model.name}
                                        className={`model-item ${selectedModel === model.name ? "selected" : ""}`}
                                        onClick={() => setSelectedModel(model.name)}
                                    >
                                        <div className="model-name">{model.name}</div>
                                        <div className="model-size">{model.size}</div>
                                    </div>
                                ))}
                            </div>
                        </section>
                    )}
                </>
            ) : (
                <section className="settings-section">
                    <h3>
                        <span className="icon">☁️</span>
                        Remote Configuration
                    </h3>
                    <div className="settings-row">
                        <div className="settings-label">
                            <span className="label-main">API Endpoint</span>
                            <span className="label-sub">OpenAI-compatible endpoint</span>
                        </div>
                        <div className="settings-control">
                            <input
                                type="text"
                                className="settings-input"
                                value={remoteUrl}
                                onChange={(e) => setRemoteUrl(e.target.value)}
                                placeholder="https://api.openai.com/v1"
                            />
                        </div>
                    </div>
                    <div className="settings-row">
                        <div className="settings-label">
                            <span className="label-main">API Key</span>
                            <span className="label-sub">Stored securely in Keychain</span>
                        </div>
                        <div className="settings-control">
                            <input
                                type="password"
                                className="settings-input"
                                value={remoteKey}
                                onChange={(e) => setRemoteKey(e.target.value)}
                                placeholder="sk-..."
                            />
                        </div>
                    </div>
                </section>
            )}
        </div>
    );
}
