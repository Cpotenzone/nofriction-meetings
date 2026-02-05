// noFriction Meetings - Settings Component
// Device selection and API key configuration with persistence

import { useState, useEffect } from "react";
import type { AudioDevice, MonitorInfo } from "../../lib/tauri";
import * as tauri from "../../lib/tauri";

interface SettingsProps {
    devices: AudioDevice[];
    monitors: MonitorInfo[];
    selectedDevice: string | null;
    selectedMonitor: number | null;
    onSelectDevice: (deviceId: string) => void;
    onSelectMonitor: (monitorId: number) => void;
}

export function Settings({
    devices,
    monitors,
    selectedDevice,
    selectedMonitor,
    onSelectDevice,
    onSelectMonitor,
}: SettingsProps) {
    const [apiKey, setApiKey] = useState("");
    const [savedApiKeyMask, setSavedApiKeyMask] = useState<string | null>(null);
    const [apiKeySaved, setApiKeySaved] = useState(false);
    const [isSaving, setIsSaving] = useState(false);
    const [isLoading, setIsLoading] = useState(true);

    // Load saved API key on mount
    useEffect(() => {
        const loadSavedKey = async () => {
            try {
                const mask = await tauri.getDeepgramApiKey();
                setSavedApiKeyMask(mask);
            } catch (err) {
                console.error("Failed to load API key:", err);
            } finally {
                setIsLoading(false);
            }
        };
        loadSavedKey();
    }, []);

    const handleSaveApiKey = async () => {
        if (!apiKey.trim()) return;

        setIsSaving(true);
        try {
            await tauri.setDeepgramApiKey(apiKey);
            setApiKeySaved(true);
            // Refresh the saved key mask
            const mask = await tauri.getDeepgramApiKey();
            setSavedApiKeyMask(mask);
            setApiKey(""); // Clear the input
            setTimeout(() => setApiKeySaved(false), 3000);
        } catch (err) {
            console.error("Failed to save API key:", err);
        } finally {
            setIsSaving(false);
        }
    };

    const handleDeviceChange = async (deviceId: string) => {
        onSelectDevice(deviceId);
        try {
            await tauri.setAudioDevice(deviceId);
        } catch (err) {
            console.error("Failed to save device:", err);
        }
    };

    const handleMonitorChange = async (monitorId: number) => {
        onSelectMonitor(monitorId);
        try {
            await tauri.setMonitor(monitorId);
        } catch (err) {
            console.error("Failed to save monitor:", err);
        }
    };

    return (
        <div className="settings-panel glass-panel">
            <h3 className="settings-title">Settings</h3>

            {/* API Key Section */}
            <div className="settings-group">
                <label className="settings-label">
                    Deepgram API Key
                    {savedApiKeyMask && (
                        <span style={{
                            marginLeft: "8px",
                            fontSize: "0.75rem",
                            color: "var(--success)",
                            fontWeight: "normal"
                        }}>
                            ✓ Configured ({savedApiKeyMask})
                        </span>
                    )}
                </label>

                {isLoading ? (
                    <div style={{ color: "var(--text-tertiary)", fontSize: "0.875rem" }}>
                        Loading...
                    </div>
                ) : (
                    <>
                        <div style={{ display: "flex", gap: "8px" }}>
                            <input
                                type="password"
                                className="settings-input"
                                placeholder={savedApiKeyMask ? "Enter new key to update" : "Enter your API key"}
                                value={apiKey}
                                onChange={(e) => setApiKey(e.target.value)}
                                style={{ flex: 1 }}
                            />
                            <button
                                className="btn btn-primary"
                                onClick={handleSaveApiKey}
                                disabled={isSaving || !apiKey.trim()}
                            >
                                {isSaving ? "..." : savedApiKeyMask ? "Update" : "Save"}
                            </button>
                        </div>
                        {apiKeySaved && (
                            <div style={{ marginTop: "8px", fontSize: "0.75rem", color: "var(--success)" }}>
                                ✓ API key saved successfully
                            </div>
                        )}
                        <div style={{ marginTop: "8px", fontSize: "0.75rem", color: "var(--text-tertiary)" }}>
                            Get your API key at{" "}
                            <a
                                href="https://deepgram.com"
                                target="_blank"
                                rel="noopener noreferrer"
                                style={{ color: "var(--accent-primary)" }}
                            >
                                deepgram.com
                            </a>
                        </div>
                    </>
                )}
            </div>

            {/* Microphone Section */}
            <div className="settings-group">
                <label className="settings-label">Microphone</label>
                {devices.length === 0 ? (
                    <div style={{ color: "var(--text-tertiary)", fontSize: "0.875rem" }}>
                        No microphones found
                    </div>
                ) : (
                    <select
                        className="settings-select"
                        value={selectedDevice || ""}
                        onChange={(e) => handleDeviceChange(e.target.value)}
                    >
                        {devices.map((device) => (
                            <option key={device.id} value={device.id}>
                                {device.name} {device.is_default ? "(Default)" : ""}
                            </option>
                        ))}
                    </select>
                )}
            </div>

            {/* Monitor Section */}
            <div className="settings-group">
                <label className="settings-label">Screen to Capture</label>
                {monitors.length === 0 ? (
                    <div style={{ color: "var(--text-tertiary)", fontSize: "0.875rem" }}>
                        No displays found. Grant Screen Recording permission in System Preferences.
                    </div>
                ) : (
                    <select
                        className="settings-select"
                        value={selectedMonitor || ""}
                        onChange={(e) => handleMonitorChange(Number(e.target.value))}
                    >
                        {monitors.map((monitor) => (
                            <option key={monitor.id} value={monitor.id}>
                                {monitor.name} ({monitor.width}x{monitor.height})
                                {monitor.is_primary ? " (Primary)" : ""}
                            </option>
                        ))}
                    </select>
                )}
            </div>

            {/* Version Info */}
            <div style={{
                marginTop: "24px",
                paddingTop: "16px",
                borderTop: "1px solid var(--glass-border)",
                fontSize: "0.75rem",
                color: "var(--text-tertiary)"
            }}>
                noFriction Meetings v1.0.0
            </div>
        </div>
    );
}
