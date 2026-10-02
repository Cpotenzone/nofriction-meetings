// noFriction Meetings - Knowledge Base Settings Component (Advanced)
// Screenshot processing (vision model). Everything stays on this Mac except
// calls to the AI provider you chose.
// AI providers are configured in AIProviderSettings (Settings → AI Engine).

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ai } from "../../lib/ai";

interface HealthStatus {
    vlm: boolean | null;
}

export function KnowledgeBaseSettings() {
    // Status
    const [health, setHealth] = useState<HealthStatus>({
        vlm: null,
    });
    const [isLoading, setIsLoading] = useState(false);
    const [isSaving, setIsSaving] = useState(false);
    const [pendingFrames, setPendingFrames] = useState<number>(0);

    useEffect(() => {
        checkHealth();
        loadPendingCount();
    }, []);

    const checkHealth = async () => {
        setIsLoading(true);
        try {
            const vlm = await ai.status().then((s) => s.vision_ready).catch(() => false);
            setHealth({ vlm });
        } catch (err) {
            console.error("Health check failed:", err);
        } finally {
            setIsLoading(false);
        }
    };

    const loadPendingCount = async () => {
        try {
            const count = await invoke<number>("get_pending_frame_count");
            setPendingFrames(count);
        } catch (err) {
            console.error("Failed to get pending count:", err);
        }
    };

    const handleAnalyze = async () => {
        setIsSaving(true);
        try {
            const result = await invoke<{ frames_processed: number; activities_created: number }>("analyze_pending_frames", { limit: 10 });
            void result;  // Used for side effects
            await loadPendingCount();
        } catch (err) {
            console.error("Analysis failed:", err);
        } finally {
            setIsSaving(false);
        }
    };

    const StatusBadge = ({ status }: { status: boolean | null }) => {
        if (status === null) return <span className="status-badge loading">Checking...</span>;
        return status
            ? <span className="status-badge success">✓ Connected</span>
            : <span className="status-badge error">✗ Not Connected</span>;
    };

    return (
        <div className="knowledge-base-settings">
            {/* Health Overview */}
            <section className="settings-section">
                <h3>
                    <span className="icon">🔗</span>
                    Status
                </h3>

                <div className="health-grid" style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "var(--spacing-md)" }}>
                    <div className="health-item" style={{ display: "flex", justifyContent: "space-between", padding: "var(--spacing-sm)" }}>
                        <span>Vision model</span>
                        <StatusBadge status={health.vlm} />
                    </div>
                </div>

                <div style={{ marginTop: "var(--spacing-md)", display: "flex", gap: "var(--spacing-md)", alignItems: "center" }}>
                    <button className="btn btn-secondary" onClick={checkHealth} disabled={isLoading}>
                        {isLoading ? "Checking..." : "Refresh Status"}
                    </button>
                    <span style={{ color: "var(--text-secondary)", fontSize: "0.875rem" }}>
                        Pending frames: {pendingFrames}
                    </span>
                </div>
            </section>

            {/* Auto Processing Section */}
            <AutoProcessingSection health={health} />

            {/* Processing Actions */}
            <section className="settings-section">
                <h3>
                    <span className="icon">⚡</span>
                    Manual Processing
                </h3>
                <div style={{ display: "flex", gap: "var(--spacing-md)", marginTop: "var(--spacing-md)" }}>
                    <button
                        className="btn btn-primary"
                        onClick={handleAnalyze}
                        disabled={isSaving || !health.vlm}
                    >
                        🔍 Analyze Pending Frames
                    </button>
                </div>
                <p style={{ fontSize: "0.75rem", color: "var(--text-tertiary)", marginTop: "var(--spacing-sm)" }}>
                    Analysis uses the vision model chosen above. Results are stored locally on this Mac.
                </p>
            </section>
        </div>
    );
}

// Auto Processing Section Component
function AutoProcessingSection({ health }: { health: { vlm: boolean | null } }) {
    const [autoEnabled, setAutoEnabled] = useState(false);
    const [intervalSecs, setIntervalSecs] = useState(120);
    const [status, setStatus] = useState<{
        running: boolean;
        pending_frames: number;
        last_run: string | null;
        frames_processed: number;
    } | null>(null);
    const [isLoading, setIsLoading] = useState(true);

    useEffect(() => {
        loadStatus();
        const interval = setInterval(loadStatus, 5000);
        return () => clearInterval(interval);
    }, []);

    const loadStatus = async () => {
        try {
            const result = await invoke<{
                running: boolean;
                enabled: boolean;
                interval_secs: number;
                pending_frames: number;
                last_run: string | null;
                frames_processed: number;
            }>("get_vlm_scheduler_status");
            setStatus(result);
            setAutoEnabled(result.enabled);
            setIntervalSecs(result.interval_secs);
        } catch (err) {
            console.error("Failed to load scheduler status:", err);
        } finally {
            setIsLoading(false);
        }
    };

    const handleToggle = async () => {
        const newValue = !autoEnabled;
        setAutoEnabled(newValue);
        try {
            await invoke("set_vlm_auto_process", { enabled: newValue });
            await loadStatus();
        } catch (err) {
            console.error("Failed to set auto-process:", err);
            setAutoEnabled(!newValue); // Revert on error
        }
    };

    const handleIntervalChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
        const newInterval = parseInt(e.target.value, 10);
        setIntervalSecs(newInterval);
        try {
            await invoke("set_vlm_process_interval", { secs: newInterval });
        } catch (err) {
            console.error("Failed to set interval:", err);
        }
    };

    const formatInterval = (secs: number): string => {
        if (secs < 60) return `${secs}s`;
        if (secs < 120) return `1 min`;
        return `${Math.round(secs / 60)} min`;
    };

    const formatLastRun = (isoDate: string | null): string => {
        if (!isoDate) return "Never";
        const date = new Date(isoDate);
        const now = new Date();
        const diffSecs = Math.floor((now.getTime() - date.getTime()) / 1000);
        if (diffSecs < 60) return `${diffSecs}s ago`;
        if (diffSecs < 3600) return `${Math.floor(diffSecs / 60)}m ago`;
        return date.toLocaleTimeString();
    };

    return (
        <section className="settings-section">
            <h3>
                <span className="icon">🔄</span>
                Automatic Processing
            </h3>

            <div className="settings-row">
                <div className="settings-label">
                    <span className="label-main">Auto-analyze frames</span>
                    <span className="label-sub">
                        Automatically process screenshots with VLM in the background
                    </span>
                </div>
                <div className="settings-control">
                    <button
                        className={`toggle-btn ${autoEnabled ? "active" : ""}`}
                        onClick={handleToggle}
                        disabled={isLoading || !health.vlm}
                        style={{
                            width: "60px",
                            height: "32px",
                            borderRadius: "16px",
                            border: "none",
                            cursor: health.vlm ? "pointer" : "not-allowed",
                            background: autoEnabled ? "var(--accent-primary)" : "var(--bg-tertiary)",
                            position: "relative",
                            transition: "background 0.2s",
                        }}
                    >
                        <span
                            style={{
                                position: "absolute",
                                top: "4px",
                                left: autoEnabled ? "32px" : "4px",
                                width: "24px",
                                height: "24px",
                                borderRadius: "50%",
                                background: "white",
                                transition: "left 0.2s",
                                boxShadow: "0 2px 4px rgba(0,0,0,0.2)",
                            }}
                        />
                    </button>
                </div>
            </div>

            <div className="settings-row" style={{ marginTop: "var(--spacing-md)" }}>
                <div className="settings-label">
                    <span className="label-main">Processing interval</span>
                    <span className="label-sub">
                        How often to analyze pending frames
                    </span>
                </div>
                <div className="settings-control" style={{ display: "flex", flexDirection: "column", gap: "8px", alignItems: "flex-end" }}>
                    <input
                        type="range"
                        min={30}
                        max={600}
                        step={30}
                        value={intervalSecs}
                        onChange={handleIntervalChange}
                        disabled={!autoEnabled}
                        style={{ width: "150px" }}
                    />
                    <span style={{ fontSize: "0.75rem", color: "var(--text-secondary)" }}>
                        Every {formatInterval(intervalSecs)}
                    </span>
                </div>
            </div>

            {status && (
                <div style={{
                    marginTop: "var(--spacing-md)",
                    padding: "var(--spacing-sm)",
                    background: "var(--bg-secondary)",
                    borderRadius: "8px",
                    fontSize: "0.875rem",
                }}>
                    <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "4px" }}>
                        <span style={{ color: "var(--text-secondary)" }}>Status:</span>
                        <span style={{ color: status.running ? "var(--accent-primary)" : "var(--text-secondary)" }}>
                            {status.running ? "🟢 Running" : "⏸️ Paused"}
                        </span>
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "4px" }}>
                        <span style={{ color: "var(--text-secondary)" }}>Pending frames:</span>
                        <span>{status.pending_frames}</span>
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "4px" }}>
                        <span style={{ color: "var(--text-secondary)" }}>Last run:</span>
                        <span>{formatLastRun(status.last_run)}</span>
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between" }}>
                        <span style={{ color: "var(--text-secondary)" }}>Total processed:</span>
                        <span>{status.frames_processed}</span>
                    </div>
                </div>
            )}
        </section>
    );
}
