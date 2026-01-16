// noFriction Meetings - Knowledge Base Settings Component
// Configuration for Supabase, Pinecone, and VLM integration

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface HealthStatus {
    supabase: boolean | null;
    pinecone: boolean | null;
    vlm: boolean | null;
    vlmVision: boolean | null;
}

export function KnowledgeBaseSettings() {
    // Connection settings
    const [supabaseConnString, setSupabaseConnString] = useState("");
    const [pineconeApiKey, setPineconeApiKey] = useState("");
    const [pineconeHost, setPineconeHost] = useState("");
    const [pineconeNamespace, setPineconeNamespace] = useState("");

    // Status
    const [health, setHealth] = useState<HealthStatus>({
        supabase: null,
        pinecone: null,
        vlm: null,
        vlmVision: null,
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
            const [supabase, pinecone, vlm, vlmVision] = await Promise.all([
                invoke<boolean>("check_supabase").catch(() => false),
                invoke<boolean>("check_pinecone").catch(() => false),
                invoke<boolean>("check_vlm").catch(() => false),
                invoke<boolean>("check_vlm_vision").catch(() => false),
            ]);
            setHealth({ supabase, pinecone, vlm, vlmVision });
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
            console.log("Analysis result:", result);
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
            console.log("Sync result:", result);
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
                        <span>VLM (Ollama)</span>
                        <StatusBadge status={health.vlm} />
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

            {/* Processing Actions */}
            <section className="settings-section">
                <h3>
                    <span className="icon">⚡</span>
                    Processing Actions
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
                    Analyze uses local VLM. Sync pushes to Supabase and/or Pinecone if configured.
                </p>
            </section>
        </div>
    );
}
