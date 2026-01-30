// Management Suite - Admin Console
// Main admin dashboard with tabs for Recordings, Audit Log, System Health, and Feature Flags

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RecordingsLibrary } from './RecordingsLibrary';
import { AuditLog } from './AuditLog';
import { LearnedDataEditor } from './LearnedDataEditor';
import { ToolsConsole } from './ToolsConsole';
import { VideoDiagnostics } from './VideoDiagnostics';

// =============================================================================
// Types
// =============================================================================

interface ServiceHealth {
    name: string;
    status: string;
    message: string | null;
    last_check: string;
}

interface QueueStats {
    pending: number;
    processing: number;
    completed: number;
    failed: number;
    total_bytes: number;
    total_bytes_formatted: string;
}

interface FeatureFlags {
    admin_console_enabled: boolean;
    dedup_enabled: boolean;
    vlm_auto_process: boolean;
    enable_ingest: boolean;
    queue_frames_for_vlm: boolean;
}

type AdminTab = 'recordings' | 'data' | 'audit' | 'health' | 'tools' | 'diagnostics' | 'flags';

// =============================================================================
// SystemHealth Component
// =============================================================================

function SystemHealth() {
    const [services, setServices] = useState<ServiceHealth[]>([]);
    const [queueStats, setQueueStats] = useState<QueueStats | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    const loadHealth = async () => {
        setIsLoading(true);
        try {
            const [health, queue] = await Promise.all([
                invoke<ServiceHealth[]>('get_system_health'),
                invoke<QueueStats>('get_admin_queue_stats')
            ]);
            setServices(health);
            setQueueStats(queue);
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            setIsLoading(false);
        }
    };

    useEffect(() => {
        loadHealth();
        const interval = setInterval(loadHealth, 10000); // Refresh every 10s
        return () => clearInterval(interval);
    }, []);

    const getStatusIcon = (status: string) => {
        switch (status) {
            case 'healthy': return '✅';
            case 'degraded': return '⚠️';
            case 'error': return '❌';
            default: return '❓';
        }
    };

    const getStatusClass = (status: string) => {
        switch (status) {
            case 'healthy': return 'status-healthy';
            case 'degraded': return 'status-degraded';
            case 'error': return 'status-error';
            default: return 'status-unknown';
        }
    };

    if (isLoading && services.length === 0) {
        return (
            <div className="loading-state">
                <div className="loading-spinner" />
                <p>Checking system health...</p>
            </div>
        );
    }

    return (
        <div className="system-health">
            <div className="health-header">
                <h3>🏥 System Health</h3>
                <button className="btn btn-icon" onClick={loadHealth} title="Refresh">
                    🔄
                </button>
            </div>

            {error && (
                <div className="error-banner">
                    ⚠️ {error}
                    <button onClick={() => setError(null)}>×</button>
                </div>
            )}

            {/* Services Grid */}
            <div className="services-grid">
                {services.map((service, idx) => (
                    <div key={idx} className={`service-card ${getStatusClass(service.status)}`}>
                        <div className="service-header">
                            <span className="service-icon">{getStatusIcon(service.status)}</span>
                            <span className="service-name">{service.name}</span>
                        </div>
                        <div className="service-status">{service.status}</div>
                        {service.message && (
                            <div className="service-message">{service.message}</div>
                        )}
                    </div>
                ))}
            </div>

            {/* Queue Stats */}
            {queueStats && (
                <div className="queue-stats">
                    <h4>📊 Ingest Queue</h4>
                    <div className="queue-stats-grid">
                        <div className="queue-stat">
                            <div className="stat-value">{queueStats.pending}</div>
                            <div className="stat-label">Pending</div>
                        </div>
                        <div className="queue-stat">
                            <div className="stat-value">{queueStats.processing}</div>
                            <div className="stat-label">Processing</div>
                        </div>
                        <div className="queue-stat">
                            <div className="stat-value">{queueStats.completed}</div>
                            <div className="stat-label">Completed</div>
                        </div>
                        <div className="queue-stat warning">
                            <div className="stat-value">{queueStats.failed}</div>
                            <div className="stat-label">Failed</div>
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}

// =============================================================================
// FeatureFlagsPanel Component
// =============================================================================

function FeatureFlagsPanel() {
    const [flags, setFlags] = useState<FeatureFlags | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [saving, setSaving] = useState<string | null>(null);

    const loadFlags = async () => {
        setIsLoading(true);
        try {
            const featureFlags = await invoke<FeatureFlags>('get_feature_flags');
            setFlags(featureFlags);
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            setIsLoading(false);
        }
    };

    useEffect(() => {
        loadFlags();
    }, []);

    const toggleFlag = async (flagName: string, currentValue: boolean) => {
        setSaving(flagName);
        try {
            await invoke('set_feature_flag', {
                flag: flagName,
                value: !currentValue
            });
            await loadFlags();
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        } finally {
            setSaving(null);
        }
    };

    if (isLoading) {
        return (
            <div className="loading-state">
                <div className="loading-spinner" />
                <p>Loading feature flags...</p>
            </div>
        );
    }

    if (!flags) return null;

    const flagConfigs = [
        { key: 'dedup_enabled', label: 'Frame Deduplication', description: 'Reduce storage by skipping duplicate frames', icon: '🔄' },
        { key: 'vlm_auto_process', label: 'VLM Auto Process', description: 'Automatically process frames with Vision LLM', icon: '🧠' },
        { key: 'enable_ingest', label: 'Intelligence Ingest', description: 'Send data to intelligence pipeline', icon: '📡' },
        { key: 'queue_frames_for_vlm', label: 'Queue VLM Frames', description: 'Queue captured frames for VLM analysis', icon: '📸' },
    ];

    return (
        <div className="feature-flags">
            <div className="flags-header">
                <h3>🚩 Feature Flags</h3>
                <p className="flags-subtitle">Toggle experimental features and behaviors</p>
            </div>

            {error && (
                <div className="error-banner">
                    ⚠️ {error}
                    <button onClick={() => setError(null)}>×</button>
                </div>
            )}

            <div className="flags-list">
                {flagConfigs.map(config => {
                    const value = flags[config.key as keyof FeatureFlags];
                    return (
                        <div key={config.key} className="flag-item">
                            <div className="flag-icon">{config.icon}</div>
                            <div className="flag-info">
                                <div className="flag-label">{config.label}</div>
                                <div className="flag-description">{config.description}</div>
                            </div>
                            <div className="flag-toggle">
                                <button
                                    className={`toggle-switch ${value ? 'active' : ''} ${saving === config.key ? 'saving' : ''}`}
                                    onClick={() => toggleFlag(config.key, value as boolean)}
                                    disabled={saving !== null}
                                >
                                    <span className="toggle-slider" />
                                </button>
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}

// =============================================================================
// AdminConsole Main Component
// =============================================================================

export function AdminConsole() {
    const [activeTab, setActiveTab] = useState<AdminTab>('recordings');

    const tabs = [
        { id: 'recordings' as AdminTab, label: 'Recordings', icon: '📼' },
        { id: 'data' as AdminTab, label: 'Learned Data', icon: '📚' },
        { id: 'audit' as AdminTab, label: 'Audit Log', icon: '📋' },
        { id: 'health' as AdminTab, label: 'System Health', icon: '🏥' },
        { id: 'tools' as AdminTab, label: 'Tools', icon: '🛠️' },
        { id: 'diagnostics' as AdminTab, label: 'Video Diagnostics', icon: '📹' },
        { id: 'flags' as AdminTab, label: 'Feature Flags', icon: '🚩' },
    ];

    return (
        <div className="admin-console">
            {/* Tab Navigation */}
            <div className="admin-tabs">
                {tabs.map(tab => (
                    <button
                        key={tab.id}
                        className={`admin-tab ${activeTab === tab.id ? 'active' : ''}`}
                        onClick={() => setActiveTab(tab.id)}
                    >
                        <span className="tab-icon">{tab.icon}</span>
                        <span className="tab-label">{tab.label}</span>
                    </button>
                ))}
            </div>

            {/* Tab Content */}
            <div className="admin-content">
                {activeTab === 'recordings' && <RecordingsLibrary />}
                {activeTab === 'data' && <LearnedDataEditor />}
                {activeTab === 'audit' && <AuditLog />}
                {activeTab === 'health' && <SystemHealth />}
                {activeTab === 'tools' && <ToolsConsole />}
                {activeTab === 'diagnostics' && <VideoDiagnostics />}
                {activeTab === 'flags' && <FeatureFlagsPanel />}
            </div>
        </div>
    );
}

export default AdminConsole;
