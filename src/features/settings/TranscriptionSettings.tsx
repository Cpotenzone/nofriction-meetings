import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

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
                try {
                    await invoke("set_deepgram_api_key", { apiKey: deepgramKey });
                    console.log("✅ Deepgram API key saved");
                } catch (err) {
                    const errorMsg = err instanceof Error ? err.message : String(err);
                    console.error("❌ Failed to save Deepgram key:", errorMsg);
                    setStatus(`Failed to save Deepgram key: ${errorMsg}`);
                    setIsSaving(false);
                    return;
                }
            }
            if (geminiKey && !geminiKey.includes("****")) {
                try {
                    await invoke("set_gemini_api_key", { apiKey: geminiKey });
                    console.log("✅ Gemini API key saved");
                } catch (err) {
                    const errorMsg = err instanceof Error ? err.message : String(err);
                    console.error("❌ Failed to save Gemini key:", errorMsg);
                    setStatus(`Failed to save Gemini key: ${errorMsg}`);
                    setIsSaving(false);
                    return;
                }
            }
            if (gladiaKey && !gladiaKey.includes("****")) {
                try {
                    await invoke("set_gladia_api_key", { apiKey: gladiaKey });
                    console.log("✅ Gladia API key saved");
                } catch (err) {
                    const errorMsg = err instanceof Error ? err.message : String(err);
                    console.error("❌ Failed to save Gladia key:", errorMsg);
                    setStatus(`Failed to save Gladia key: ${errorMsg}`);
                    setIsSaving(false);
                    return;
                }
            }
            if (googleKey && !googleKey.includes("****")) {
                try {
                    await invoke("set_google_stt_key", { keyJson: googleKey });
                    console.log("✅ Google STT key saved");
                } catch (err) {
                    const errorMsg = err instanceof Error ? err.message : String(err);
                    console.error("❌ Failed to save Google STT key:", errorMsg);
                    setStatus(`Failed to save Google STT key: ${errorMsg}`);
                    setIsSaving(false);
                    return;
                }
            }

            // Set active provider
            try {
                await invoke("set_active_provider", { provider: activeProvider });
                setProvider(activeProvider);
                console.log(`✅ Active provider set to: ${activeProvider}`);
            } catch (err) {
                const errorMsg = err instanceof Error ? err.message : String(err);
                console.error("❌ Failed to set active provider:", errorMsg);
                setStatus(`Failed to set active provider: ${errorMsg}`);
                setIsSaving(false);
                return;
            }

            setStatus("✅ Settings saved successfully");
            setTimeout(() => setStatus(null), 3000);
            onSave?.();
        } catch (err) {
            const errorMsg = err instanceof Error ? err.message : String(err);
            console.error("❌ Unexpected error saving settings:", errorMsg);
            setStatus(`Failed to save settings: ${errorMsg}`);
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


        </div>
    );
}
