import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface TranscriptionSettingsProps {
    onSave?: () => void;
}

interface WhisperModelInfo {
    name: string;
    size_mb: number;
    description: string;
    installed: boolean;
    active: boolean;
}

interface LocalSttStatus {
    models_dir: string | null;
    preferred_model: string;
    ready: boolean;
    resolved_model: string | null;
    models: WhisperModelInfo[];
}

interface WhisperDownloadProgress {
    model: string;
    downloaded_bytes: number;
    total_bytes: number | null;
    done: boolean;
    error: string | null;
}

export function TranscriptionSettings({ onSave }: TranscriptionSettingsProps) {
    const [provider, setProvider] = useState("local");
    const [deepgramKey, setDeepgramKey] = useState("");
    const [deepgramModel, setDeepgramModel] = useState("nova-3");
    const [geminiKey, setGeminiKey] = useState("");
    const [geminiModel, setGeminiModel] = useState("models/gemini-2.0-flash-exp");
    const [gladiaKey, setGladiaKey] = useState("");
    const [googleKey, setGoogleKey] = useState("");
    const [googleModel, setGoogleModel] = useState("chirp_2");
    const [googleRegion, setGoogleRegion] = useState("us-central1");
    const [googleDiarization, setGoogleDiarization] = useState(true);

    const [isSaving, setIsSaving] = useState(false);
    const [status, setStatus] = useState<string | null>(null);

    // Local (offline) Whisper state
    const [localStatus, setLocalStatus] = useState<LocalSttStatus | null>(null);
    const [downloading, setDownloading] = useState<string | null>(null);
    const [downloadPct, setDownloadPct] = useState<number>(0);

    useEffect(() => {
        loadSettings();
        loadLocalStatus();

        let unlisten: (() => void) | null = null;
        listen<WhisperDownloadProgress>("whisper_download_progress", (e) => {
            const p = e.payload;
            if (p.error) {
                setDownloading(null);
                setStatus(`Model download failed: ${p.error}`);
                return;
            }
            if (p.done) {
                setDownloading(null);
                setDownloadPct(0);
                loadLocalStatus();
                return;
            }
            if (p.total_bytes) {
                setDownloadPct(Math.round((p.downloaded_bytes / p.total_bytes) * 100));
            }
        }).then((fn) => { unlisten = fn; });

        return () => { unlisten?.(); };
    }, []);

    const loadLocalStatus = async () => {
        try {
            const s = await invoke<LocalSttStatus>("get_local_stt_status");
            setLocalStatus(s);
        } catch (err) {
            console.warn("Local STT status unavailable:", err);
        }
    };

    const handleDownloadModel = async (model: string) => {
        setDownloading(model);
        setDownloadPct(0);
        try {
            await invoke("download_whisper_model", { model });
        } catch (err) {
            setStatus(`Model download failed: ${err}`);
            setDownloading(null);
        }
        loadLocalStatus();
    };

    const handleSelectModel = async (model: string) => {
        try {
            await invoke("set_local_whisper_model", { model });
            loadLocalStatus();
        } catch (err) {
            setStatus(`Failed to select model: ${err}`);
        }
    };

    const loadSettings = async () => {
        try {
            const settings = await invoke<any>("get_settings");
            setProvider(settings.transcription_provider || "local");

            // Keys are not returned by get_settings for security (usually), 
            // but we might want placeholders or status indicators.
            // For now, we leave them blank or assume masking if the backend supported returning masked keys in a separate call.
            // get_deepgram_api_key returns masked. We'd need generic getters for others.

            const dgKey = await invoke<string | null>("get_deepgram_api_key");
            if (dgKey) setDeepgramKey(dgKey);

            const dgModel = await invoke<string | null>("get_setting", { key: "deepgram_model" });
            if (dgModel) setDeepgramModel(dgModel);


            const gModel = await invoke<string | null>("get_setting", { key: "gemini_model" });
            if (gModel) setGeminiModel(gModel);

            const gKey = await invoke<string | null>("get_gemini_api_key");
            if (gKey) setGeminiKey(gKey);
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
                } catch (err) {
                    const errorMsg = err instanceof Error ? err.message : String(err);
                    console.error("❌ Failed to save Gemini key:", errorMsg);
                    setStatus(`Failed to save Gemini key: ${errorMsg}`);
                    setIsSaving(false);
                    return;
                }
            }

            // Save models
            try {
                // Read by the providers at connect time (settings.rs)
                await invoke("set_setting", { key: "deepgram_model", value: deepgramModel });
                await invoke("set_setting", { key: "gemini_model", value: geminiModel });
            } catch (err) {
                console.error("Failed to save models", err);
            }
            if (gladiaKey && !gladiaKey.includes("****")) {
                try {
                    await invoke("set_gladia_api_key", { apiKey: gladiaKey });
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
            } catch (err) {
                const errorMsg = err instanceof Error ? err.message : String(err);
                console.error("❌ Failed to set active provider:", errorMsg);
                setStatus(`Failed to set active provider: ${errorMsg}`);
                setIsSaving(false);
                return;
            }

            setStatus("Settings saved");
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
                        <option value="local">Local Whisper — offline, no API key</option>
                        <option value="google_stt">Google Cloud STT (Chirp 2)</option>
                        <option value="deepgram">Deepgram (Nova-3)</option>
                        <option value="gemini">Google Gemini Live</option>
                        <option value="gladia">Gladia</option>
                    </select>
                </div>
            </div>

            <section className="settings-section">
                <h3>API Configuration</h3>
                <p className="section-desc">Manage API keys for supported transcription services.</p>

                <div className="api-key-grid">
                    {/* Local Whisper — offline, first-class */}
                    <div className={`provider-card ${provider === "local" ? "active" : ""}`}>
                        <div className="provider-header">
                            <span className="name">Local Whisper (Offline)</span>
                            {provider === "local" && <span className="badge">Active</span>}
                        </div>
                        <p className="provider-desc">
                            Runs entirely on this Mac (Metal-accelerated). No API key, no
                            network, nothing leaves your machine. Download a model once,
                            then transcription works offline forever.
                        </p>
                        {localStatus && (
                            <div className="input-group">
                                <label>
                                    Models{" "}
                                    {localStatus.ready
                                        ? `— ready (${localStatus.resolved_model})`
                                        : "— none installed yet"}
                                </label>
                                {localStatus.models.map((m) => (
                                    <div
                                        key={m.name}
                                        style={{
                                            display: "flex",
                                            alignItems: "center",
                                            gap: 8,
                                            padding: "6px 0",
                                            fontSize: 12,
                                        }}
                                    >
                                        <input
                                            type="radio"
                                            name="whisper-model"
                                            checked={m.active}
                                            disabled={!m.installed}
                                            onChange={() => handleSelectModel(m.name)}
                                        />
                                        <span style={{ minWidth: 110, fontWeight: 600 }}>{m.name}</span>
                                        <span style={{ flex: 1, opacity: 0.7 }}>
                                            {m.description} ({m.size_mb} MB)
                                        </span>
                                        {m.installed ? (
                                            <span className="badge">Installed</span>
                                        ) : downloading === m.name ? (
                                            <span style={{ minWidth: 90 }}>{downloadPct}%…</span>
                                        ) : (
                                            <button
                                                className="btn-secondary"
                                                style={{ padding: "2px 10px", fontSize: 11 }}
                                                disabled={downloading !== null}
                                                onClick={() => handleDownloadModel(m.name)}
                                            >
                                                Download
                                            </button>
                                        )}
                                    </div>
                                ))}
                            </div>
                        )}
                    </div>

                    {/* Google STT */}
                    <div className={`provider-card ${provider === "google_stt" ? "active" : ""}`}>
                        <div className="provider-header">
                            <span className="name">Google Cloud STT V2</span>
                            {provider === "google_stt" && <span className="badge">Active</span>}
                        </div>
                        <p className="provider-desc">
                            Chirp 2 model with speaker diarization. Requires a GCP service account with Speech-to-Text API enabled.
                        </p>
                        <div className="input-group">
                            <label>Service Account JSON</label>
                            <textarea
                                value={googleKey}
                                onChange={(e) => setGoogleKey(e.target.value)}
                                placeholder='Paste your service account JSON key here...'
                                className="modern-input"
                                rows={3}
                                style={{ fontFamily: "monospace", fontSize: "11px", resize: "vertical" }}
                            />
                        </div>
                        <div className="input-group">
                            <label>Model</label>
                            <select
                                value={googleModel}
                                onChange={(e) => setGoogleModel(e.target.value)}
                                className="modern-select"
                            >
                                <option value="chirp_2">Chirp 2 (Latest, Best Quality)</option>
                                <option value="chirp">Chirp (Previous Gen)</option>
                                <option value="latest_long">Long-form (V1 Compat)</option>
                                <option value="latest_short">Short-form (V1 Compat)</option>
                            </select>
                        </div>
                        <div className="input-group">
                            <label>Region</label>
                            <select
                                value={googleRegion}
                                onChange={(e) => setGoogleRegion(e.target.value)}
                                className="modern-select"
                            >
                                <option value="us-central1">US Central (Iowa)</option>
                                <option value="europe-west4">Europe West (Netherlands)</option>
                                <option value="asia-southeast1">Asia Southeast (Singapore)</option>
                            </select>
                        </div>
                        <div className="input-group toggle-group">
                            <label>Speaker Diarization</label>
                            <label className="toggle-switch">
                                <input
                                    type="checkbox"
                                    checked={googleDiarization}
                                    onChange={(e) => setGoogleDiarization(e.target.checked)}
                                />
                                <span className="toggle-slider" />
                            </label>
                        </div>
                    </div>

                    {/* Deepgram */}
                    <div className={`provider-card ${provider === "deepgram" ? "active" : ""}`}>
                        <div className="provider-header">
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
                        <div className="input-group">
                            <label>Model</label>
                            <select
                                value={deepgramModel}
                                onChange={(e) => setDeepgramModel(e.target.value)}
                                className="modern-select"
                            >
                                <option value="nova-3">Nova-3 (Latest, Smart)</option>
                                <option value="nova-2-meeting">Nova-2 Meeting (Speaker ID Optimized)</option>
                                <option value="nova-2">Nova-2 (General)</option>
                                <option value="enhanced">Enhanced (Legacy)</option>
                            </select>
                        </div>
                    </div>

                    {/* Gemini */}
                    <div className={`provider-card ${provider === "gemini" ? "active" : ""}`}>
                        <div className="provider-header">
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
                        <div className="input-group">
                            <label>Model</label>
                            <select
                                value={geminiModel}
                                onChange={(e) => setGeminiModel(e.target.value)}
                                className="modern-select"
                            >
                                <option value="models/gemini-2.0-flash-exp">Gemini 2.0 Flash (Experimental)</option>
                                <option value="models/gemini-1.5-flash-latest">Gemini 1.5 Flash</option>
                                <option value="models/gemini-1.5-pro-latest">Gemini 1.5 Pro</option>
                            </select>
                        </div>
                    </div>

                    {/* Gladia */}
                    <div className={`provider-card ${provider === "gladia" ? "active" : ""}`}>
                        <div className="provider-header">
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
