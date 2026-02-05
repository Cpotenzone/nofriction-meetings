// Intelligence Pipeline Settings Component
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";


export function IngestSettings() {
    const [enabled, setEnabled] = useState(false);
    const [baseUrl, setBaseUrl] = useState("https://localhost:8080");
    const [bearerToken, setBearerToken] = useState("");
    const [queueStats, setQueueStats] = useState<[number, number]>([0, 0]);
    const [connectionStatus, setConnectionStatus] = useState<"unknown" | "connected" | "error">("unknown");
    const [saving, setSaving] = useState(false);

    const loadSettings = async () => {
        try {
            const settings: any = await invoke("get_settings");
            setEnabled(settings.enable_ingest || false);
            setBaseUrl(settings.ingest_base_url || "https://localhost:8080");
            // Don't load token for security
        } catch (err) {
            console.error("Failed to load ingest settings:", err);
        }
    };

    const saveSettings = async () => {
        setSaving(true);
        try {
            await invoke("set_enable_ingest", { enabled });
            if (baseUrl && bearerToken) {
                await invoke("set_ingest_config", { baseUrl, bearerToken });
            }
            alert("Intelligence Pipeline settings saved successfully");

            // Test connection if enabled
            if (enabled) {
                testConnection();
            }
        } catch (err) {
            alert(`Failed to save settings: ${err}`);
        } finally {
            setSaving(false);
        }
    };

    const testConnection = async () => {
        try {
            const result = await invoke("test_ingest_connection");
            setConnectionStatus(result ? "connected" : "error");
        } catch (err) {
            setConnectionStatus("error");
        }
    };

    const refreshQueueStats = async () => {
        try {
            const stats: [number, number] = await invoke("get_ingest_queue_stats");
            setQueueStats(stats);
        } catch (err) {
            console.error("Failed to get queue stats:", err);
        }
    };

    useEffect(() => {
        loadSettings();
        const interval = setInterval(refreshQueueStats, 5000);
        return () => clearInterval(interval);
    }, []);

    return (
        <div className="settings-section">
            <h3>🧠 Intelligence Pipeline</h3>
            <p className="settings-description">
                Upload frames and transcripts to server-side pipeline for VLM+LLM analysis and vector search.
            </p>

            <div className="settings-row">
                <label className="checkbox-label">
                    <input
                        type="checkbox"
                        checked={enabled}
                        onChange={(e) => setEnabled(e.target.checked)}
                    />
                    <span>Enable Remote Intelligence</span>
                </label>
            </div>

            {enabled && (
                <>
                    <div className="settings-row">
                        <label>Base URL</label>
                        <input
                            type="text"
                            value={baseUrl}
                            onChange={(e) => setBaseUrl(e.target.value)}
                            placeholder="https://localhost:8080"
                        />
                    </div>

                    <div className="settings-row">
                        <label>Bearer Token</label>
                        <input
                            type="password"
                            value={bearerToken}
                            onChange={(e) => setBearerToken(e.target.value)}
                            placeholder="Enter API token from server .env"
                        />
                        <small>Stored securely, never logged</small>
                    </div>

                    <div className="settings-row">
                        <label>Connection Status</label>
                        <div className="status-indicator">
                            {connectionStatus === "connected" && <span className="status-ok">✓ Connected</span>}
                            {connectionStatus === "error" && <span className="status-error">✗ Connection Failed</span>}
                            {connectionStatus === "unknown" && <span className="status-unknown">? Not Tested</span>}
                        </div>
                        <button onClick={testConnection} className="btn-secondary">Test Connection</button>
                    </div>

                    <div className="settings-row">
                        <label>Upload Queue</label>
                        <div className="queue-stats">
                            <span>Pending: {queueStats[0]}</span>
                            <span>Failed: {queueStats[1]}</span>
                        </div>
                    </div>
                </>
            )}

            <div className="settings-actions">
                <button onClick={saveSettings} disabled={saving} className="btn-primary">
                    {saving ? "Saving..." : "Save Settings"}
                </button>
            </div>
        </div>
    );
}
