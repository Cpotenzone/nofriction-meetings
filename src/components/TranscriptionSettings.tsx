import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import ComparisonLab from "./ComparisonLab";

interface TranscriptionSettingsProps {
    onSave?: () => void;
}

export function TranscriptionSettings({ onSave }: TranscriptionSettingsProps) {
    const [provider, setProvider] = useState("deepgram");
    const [deepgramKey, setDeepgramKey] = useState("");
    const [geminiKey, setGeminiKey] = useState("");
    const [gladiaKey, setGladiaKey] = useState("");
    const [googleKey, setGoogleKey] = useState("");

    const [isSaving, setIsSaving] = useState(false);
    const [status, setStatus] = useState<string | null>(null);
    const [showComparisonLab, setShowComparisonLab] = useState(false);

    useEffect(() => {
        loadSettings();
    }, []);

    const loadSettings = async () => {
        try {
            const settings = await invoke<any>("get_settings");
            setProvider(settings.transcription_provider || "deepgram");

            // Keys are not returned by get_settings for security (usually), 
            // but we might want placeholders or status indicators.
            // For now, we leave them blank or assume masking if the backend supported returning masked keys in a separate call.
            // get_deepgram_api_key returns masked. We'd need generic getters for others.

            const dgKey = await invoke<string | null>("get_deepgram_api_key");
            if (dgKey) setDeepgramKey(dgKey);

            // TODO: Implement get_gemini_api_key etc if needed for masked display
        } catch (err) {
            console.error("Failed to load settings:", err);
        }
    };

    const handleSave = async (activeProvider: string) => {
        setIsSaving(true);
        setStatus(null);
        try {
            // Save keys if changed (length > 0 && not masked)
            if (deepgramKey && !deepgramKey.includes("****")) {
                await invoke("set_deepgram_api_key", { api_key: deepgramKey });
            }
            if (geminiKey && !geminiKey.includes("****")) {
                await invoke("set_gemini_api_key", { api_key: geminiKey });
            }
            if (gladiaKey && !gladiaKey.includes("****")) {
                await invoke("set_gladia_api_key", { api_key: gladiaKey });
            }
            if (googleKey && !googleKey.includes("****")) {
                await invoke("set_google_stt_key", { key_json: googleKey });
            }

            // Set active provider
            await invoke("set_active_provider", { provider: activeProvider });
            setProvider(activeProvider);

            setStatus("Settings saved successfully");
            setTimeout(() => setStatus(null), 3000);
            onSave?.();
        } catch (err) {
            console.error("Failed to save:", err);
            setStatus("Failed to save settings");
        } finally {
            setIsSaving(false);
        }
    };

    return (
        <div className="settings-content-panel fade-in">
            <div className="content-header">
                <h2>Transcription Engine</h2>
                <div className="provider-selector">
                    <select
                        value={provider}
                        onChange={(e) => handleSave(e.target.value)}
                        className="modern-select"
                        disabled={isSaving}
                    >
                        <option value="deepgram">Deepgram (Nova-3)</option>
                        <option value="gemini">Google Gemini Live</option>
                        <option value="gladia">Gladia</option>
                        <option value="google_stt">Google Cloud STT</option>
                    </select>
                </div>
            </div>

            <section className="settings-section">
                <h3>API Configuration</h3>
                <p className="section-desc">Manage API keys for supported transcription services.</p>

                <div className="api-key-grid">
                    {/* Deepgram */}
                    <div className={`provider-card ${provider === "deepgram" ? "active" : ""}`}>
                        <div className="provider-header">
                            <span className="icon">🦄</span>
                            <span className="name">Deepgram</span>
                            {provider === "deepgram" && <span className="badge">Active</span>}
                        </div>
                        <div className="input-group">
                            <label>API Key</label>
                            <input
                                type="password"
                                value={deepgramKey}
                                onChange={(e) => setDeepgramKey(e.target.value)}
                                placeholder="Enter Deepgram Key"
                                className="modern-input"
                            />
                        </div>
                    </div>

                    {/* Gemini */}
                    <div className={`provider-card ${provider === "gemini" ? "active" : ""}`}>
                        <div className="provider-header">
                            <span className="icon">✨</span>
                            <span className="name">Google Gemini</span>
                            {provider === "gemini" && <span className="badge">Active</span>}
                        </div>
                        <div className="input-group">
                            <label>API Key</label>
                            <input
                                type="password"
                                value={geminiKey}
                                onChange={(e) => setGeminiKey(e.target.value)}
                                placeholder="Enter Gemini Key"
                                className="modern-input"
                            />
                        </div>
                    </div>

                    {/* Gladia */}
                    <div className={`provider-card ${provider === "gladia" ? "active" : ""}`}>
                        <div className="provider-header">
                            <span className="icon">🌊</span>
                            <span className="name">Gladia</span>
                            {provider === "gladia" && <span className="badge">Active</span>}
                        </div>
                        <div className="input-group">
                            <label>API Key</label>
                            <input
                                type="password"
                                value={gladiaKey}
                                onChange={(e) => setGladiaKey(e.target.value)}
                                placeholder="Enter Gladia Key"
                                className="modern-input"
                            />
                        </div>
                    </div>

                    {/* Google STT */}
                    <div className={`provider-card ${provider === "google_stt" ? "active" : ""}`}>
                        <div className="provider-header">
                            <span className="icon">☁️</span>
                            <span className="name">Google Cloud STT</span>
                            {provider === "google_stt" && <span className="badge">Active</span>}
                        </div>
                        <div className="input-group">
                            <label>JSON Key (Base64)</label>
                            <input
                                type="password"
                                value={googleKey}
                                onChange={(e) => setGoogleKey(e.target.value)}
                                placeholder="Paste JSON Key content"
                                className="modern-input"
                            />
                        </div>
                    </div>
                </div>

                <div className="action-row">
                    <button
                        className="btn-primary"
                        onClick={() => handleSave(provider)}
                        disabled={isSaving}
                    >
                        {isSaving ? "Saving..." : "Save Configuration"}
                    </button>
                    {status && <span className="status-msg">{status}</span>}
                </div>
            </section>

            <section className="settings-section">
                <h3>Comparison Lab 🧪</h3>
                <p className="section-desc">Test different providers side-by-side.</p>
                <div className="lab-placeholder">
                    <div className="lab-icon">🔬</div>
                    <p>Record a sample to compare transcript quality and latency.</p>
                    <button
                        className="btn-primary"
                        onClick={() => setShowComparisonLab(true)}
                    >
                        Open Comparison Lab
                    </button>
                </div>
            </section>

            {showComparisonLab && (
                <div className="modal-overlay" onClick={() => setShowComparisonLab(false)}>
                    <div className="modal-content" style={{ maxWidth: '95vw', width: '1600px' }} onClick={(e) => e.stopPropagation()}>
                        <div className="modal-header">
                            <div>
                                <h2>Comparison Lab</h2>
                                <p className="modal-subtitle">Test multiple providers side-by-side</p>
                            </div>
                            <button className="modal-close" onClick={() => setShowComparisonLab(false)}>
                                <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor">
                                    <path d="M18 6L6 18M6 6l12 12" strokeWidth="2" strokeLinecap="round" />
                                </svg>
                            </button>
                        </div>
                        <div className="modal-body" style={{ padding: 0 }}>
                            <ComparisonLab />
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}
