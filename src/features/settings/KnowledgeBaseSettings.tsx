// noFriction Meetings - Knowledge Base Settings Component
// Configuration for Supabase, Pinecone, and VLM integration

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface HealthStatus {
    supabase: boolean | null;
    pinecone: boolean | null;
    vlm: boolean | null;
    vlmVision: boolean | null;
    thebrain: boolean | null;
}

export function KnowledgeBaseSettings() {
    // Connection settings
    const [supabaseConnString, setSupabaseConnString] = useState("");
    const [pineconeApiKey, setPineconeApiKey] = useState("");
    const [pineconeHost, setPineconeHost] = useState("");
    const [pineconeNamespace, setPineconeNamespace] = useState("");

    // TheBrain credentials
    const [thebrainEmail, setThebrainEmail] = useState("");
    const [thebrainPassword, setThebrainPassword] = useState("");
    const [thebrainApiUrl, setThebrainApiUrl] = useState("https://7wk6vrq9achr2djw.caas.targon.com");
    const [thebrainError, setThebrainError] = useState<string | null>(null);

    // Status
    const [health, setHealth] = useState<HealthStatus>({
        supabase: null,
        pinecone: null,
        vlm: null,
        vlmVision: null,
        thebrain: null,
    });
    const [isLoading, setIsLoading] = useState(false);
    const [isSaving, setIsSaving] = useState(false);
    const [pendingFrames, setPendingFrames] = useState<number>(0);

    useEffect(() => {
        checkHealth();
        loadPendingCount();
        loadSavedCredentials();
    }, []);

    const loadSavedCredentials = async () => {
        try {
            const [savedUsername, savedUrl] = await Promise.all([
                invoke<string | null>("get_setting", { key: "thebrain_username" }),
                invoke<string | null>("get_setting", { key: "vlm_base_url" }),
            ]);
            if (savedUsername) setThebrainEmail(savedUsername);
            if (savedUrl) setThebrainApiUrl(savedUrl);
        } catch (err) {
            console.error("Failed to load saved TheBrain credentials:", err);
        }
    };

    const checkHealth = async () => {
        setIsLoading(true);
        try {
            const [supabase, pinecone, vlm, vlmVision, thebrain] = await Promise.all([
                invoke<boolean>("check_supabase").catch(() => false),
                invoke<boolean>("check_pinecone").catch(() => false),
                invoke<boolean>("check_vlm").catch(() => false),
                invoke<boolean>("check_vlm_vision").catch(() => false),
                invoke<boolean>("check_thebrain").catch(() => false),
            ]);
            setHealth({ supabase, pinecone, vlm, vlmVision, thebrain });
        } catch (err) {
            console.error("Health check failed:", err);
        } finally {
            setIsLoading(false);
        }
    };

    const handleThebrainLogin = async () => {
        if (!thebrainEmail.trim() || !thebrainPassword.trim()) return;
        setIsSaving(true);
        setThebrainError(null);
        try {
            // First, save the API URL to settings and configure VLM client
            await invoke("set_vlm_api_url", { url: thebrainApiUrl });
            // Then authenticate
            await invoke("thebrain_authenticate", {
                username: thebrainEmail,
                password: thebrainPassword
            });
            await checkHealth();
            setThebrainPassword(""); // Clear password after success
        } catch (err) {
            console.error("TheBrain login failed:", err);
            setThebrainError(String(err));
        } finally {
            setIsSaving(false);
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

    const handleSaveSupabase = async () => {
        if (!supabaseConnString.trim()) return;
        setIsSaving(true);
        try {
            await invoke("configure_supabase", { connectionString: supabaseConnString });
            await checkHealth();
        } catch (err) {
            console.error("Failed to configure Supabase:", err);
        } finally {
            setIsSaving(false);
        }
    };

    const handleSavePinecone = async () => {
        if (!pineconeApiKey.trim() || !pineconeHost.trim()) return;
        setIsSaving(true);
        try {
            await invoke("configure_pinecone", {
                apiKey: pineconeApiKey,
                indexHost: pineconeHost,
                namespace: pineconeNamespace || null,
            });
            await checkHealth();
        } catch (err) {
            console.error("Failed to configure Pinecone:", err);
        } finally {
            setIsSaving(false);
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

    const handleSync = async () => {
        setIsSaving(true);
        try {
            const result = await invoke<{ activities_synced: number }>("sync_to_cloud", { limit: 50 });
            void result;  // Used for side effects
        } catch (err) {
            console.error("Sync failed:", err);
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
                    Knowledge Base Status
                </h3>

                <div className="health-grid" style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "var(--spacing-md)" }}>
                    <div className="health-item" style={{ display: "flex", justifyContent: "space-between", padding: "var(--spacing-sm)" }}>
                        <span>🧠 TheBrain Cloud</span>
                        <StatusBadge status={health.thebrain} />
                    </div>
                    <div className="health-item" style={{ display: "flex", justifyContent: "space-between", padding: "var(--spacing-sm)" }}>
                        <span>Vision Model</span>
                        <StatusBadge status={health.vlmVision} />
                    </div>
                    <div className="health-item" style={{ display: "flex", justifyContent: "space-between", padding: "var(--spacing-sm)" }}>
                        <span>Supabase</span>
                        <StatusBadge status={health.supabase} />
                    </div>
                    <div className="health-item" style={{ display: "flex", justifyContent: "space-between", padding: "var(--spacing-sm)" }}>
                        <span>Pinecone</span>
                        <StatusBadge status={health.pinecone} />
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

            {/* TheBrain Cloud VLM */}
            <section className="settings-section">
                <h3>
                    <span className="icon">🧠</span>
                    TheBrain Cloud AI
                </h3>
                <p style={{ fontSize: "0.875rem", color: "var(--text-secondary)", marginBottom: "var(--spacing-md)" }}>
                    Connect to TheBrain Cloud for AI-powered screen analysis and knowledge extraction.
                </p>

                {health.thebrain ? (
                    <div className="success-box" style={{
                        padding: "var(--spacing-md)",
                        background: "rgba(16, 185, 129, 0.1)",
                        borderRadius: "8px",
                        border: "1px solid rgba(16, 185, 129, 0.3)"
                    }}>
                        ✅ Connected to TheBrain Cloud
                    </div>
                ) : (
                    <>
                        <div className="settings-row">
                            <div className="settings-label">
                                <span className="label-main">API URL</span>
                                <span className="label-hint">VLM endpoint URL</span>
                            </div>
                            <div className="settings-control">
                                <input
                                    type="url"
                                    className="settings-input"
                                    placeholder="https://your-api-endpoint.com"
                                    value={thebrainApiUrl}
                                    onChange={(e) => setThebrainApiUrl(e.target.value)}
                                    style={{ fontSize: "0.8rem" }}
                                />
                            </div>
                        </div>
                        <div className="settings-row">
                            <div className="settings-label">
                                <span className="label-main">Email</span>
                            </div>
                            <div className="settings-control">
                                <input
                                    type="email"
                                    className="settings-input"
                                    placeholder="your@email.com"
                                    value={thebrainEmail}
                                    onChange={(e) => setThebrainEmail(e.target.value)}
                                />
                            </div>
                        </div>
                        <div className="settings-row">
                            <div className="settings-label">
                                <span className="label-main">Password</span>
                            </div>
                            <div className="settings-control" style={{ display: "flex", gap: "var(--spacing-sm)" }}>
                                <input
                                    type="password"
                                    className="settings-input"
                                    placeholder="••••••••"
                                    value={thebrainPassword}
                                    onChange={(e) => setThebrainPassword(e.target.value)}
                                />
                                <button
                                    className="settings-button"
                                    onClick={handleThebrainLogin}
                                    disabled={isSaving || !thebrainEmail.trim() || !thebrainPassword.trim()}
                                >
                                    {isSaving ? "Connecting..." : "Connect"}
                                </button>
                            </div>
                        </div>
                        {thebrainError && (
                            <p style={{ color: "var(--error)", fontSize: "0.875rem", marginTop: "var(--spacing-sm)" }}>
                                ⚠️ {thebrainError}
                            </p>
                        )}
                    </>
                )}
            </section>

            {/* Supabase Configuration */}
            <section className="settings-section">
                <h3>
                    <span className="icon">🐘</span>
                    Supabase (Cloud Database)
                </h3>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Connection String</span>
                        <span className="label-sub">PostgreSQL connection URL from Supabase dashboard</span>
                    </div>
                    <div className="settings-control" style={{ display: "flex", gap: "var(--spacing-sm)" }}>
                        <input
                            type="password"
                            className="settings-input"
                            placeholder="postgresql://user:pass@host:5432/postgres"
                            value={supabaseConnString}
                            onChange={(e) => setSupabaseConnString(e.target.value)}
                        />
                        <button
                            className="settings-button"
                            onClick={handleSaveSupabase}
                            disabled={isSaving || !supabaseConnString.trim()}
                        >
                            {isSaving ? "Saving..." : "Connect"}
                        </button>
                    </div>
                </div>
            </section>

            {/* Pinecone Configuration */}
            <section className="settings-section">
                <h3>
                    <span className="icon">🌲</span>
                    Pinecone (Vector Search)
                </h3>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">API Key</span>
                        <span className="label-sub">From Pinecone dashboard</span>
                    </div>
                    <div className="settings-control">
                        <input
                            type="password"
                            className="settings-input"
                            placeholder="pc-..."
                            value={pineconeApiKey}
                            onChange={(e) => setPineconeApiKey(e.target.value)}
                        />
                    </div>
                </div>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Index Host</span>
                        <span className="label-sub">e.g., index-name-abc123.svc.pinecone.io</span>
                    </div>
                    <div className="settings-control">
                        <input
                            type="text"
                            className="settings-input"
                            placeholder="your-index.svc.pinecone.io"
                            value={pineconeHost}
                            onChange={(e) => setPineconeHost(e.target.value)}
                        />
                    </div>
                </div>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Namespace (optional)</span>
                        <span className="label-sub">Partition for your data</span>
                    </div>
                    <div className="settings-control" style={{ display: "flex", gap: "var(--spacing-sm)" }}>
                        <input
                            type="text"
                            className="settings-input"
                            placeholder="default"
                            value={pineconeNamespace}
                            onChange={(e) => setPineconeNamespace(e.target.value)}
                        />
                        <button
                            className="settings-button"
                            onClick={handleSavePinecone}
                            disabled={isSaving || !pineconeApiKey.trim() || !pineconeHost.trim()}
                        >
                            {isSaving ? "Saving..." : "Connect"}
                        </button>
                    </div>
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
                    <button
                        className="btn btn-secondary"
                        onClick={handleSync}
                        disabled={isSaving || (!health.supabase && !health.pinecone)}
                    >
                        ☁️ Sync to Cloud
                    </button>
                </div>
                <p style={{ fontSize: "0.75rem", color: "var(--text-tertiary)", marginTop: "var(--spacing-sm)" }}>
                    Manual analysis using local VLM. Results sync to cloud if configured.
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
