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

type AdminTab = 'recordings' | 'data' | 'audit' | 'health' | 'tools' | 'diagnostics' | 'flags' | 'about';

// =============================================================================
// About Panel Component
// =============================================================================

function AboutPanel() {
    return (
        <div className="about-panel">
            <div style={{ textAlign: 'center', padding: '40px 20px' }}>
                <div style={{ fontSize: '64px', marginBottom: '16px' }}>🚀</div>
                <h2 style={{ fontSize: '28px', fontWeight: 700, color: '#fff', marginBottom: '8px' }}>
                    noFriction Meetings
                </h2>
                <p style={{ color: '#a78bfa', fontSize: '16px', fontWeight: 600, marginBottom: '24px' }}>
                    Version 1.0.0 RC 1
                </p>
                <p style={{ color: '#9ca3af', fontSize: '14px', maxWidth: '500px', margin: '0 auto 32px', lineHeight: 1.7 }}>
                    Your AI-powered meeting companion that captures everything—audio, screen content, and visual context—so you can focus on the conversation, not on taking notes.
                </p>
            </div>

            {/* Value Props */}
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '16px', maxWidth: '700px', margin: '0 auto 40px' }}>
                {[
                    { icon: '⏪', title: 'Total Recall', desc: 'Synchronized audio + screen + screenshots' },
                    { icon: '🎯', title: 'Zero Effort', desc: 'One-click recording, automatic transcription' },
                    { icon: '🔒', title: 'Privacy First', desc: 'All processing happens locally on your Mac' },
                ].map((prop, i) => (
                    <div key={i} style={{
                        background: 'rgba(255,255,255,0.05)',
                        padding: '20px',
                        borderRadius: '12px',
                        textAlign: 'center'
                    }}>
                        <div style={{ fontSize: '28px', marginBottom: '12px' }}>{prop.icon}</div>
                        <h4 style={{ color: '#fff', fontSize: '14px', fontWeight: 600, marginBottom: '8px' }}>{prop.title}</h4>
                        <p style={{ color: '#9ca3af', fontSize: '12px', margin: 0, lineHeight: 1.5 }}>{prop.desc}</p>
                    </div>
                ))}
            </div>

            {/* Core Features */}
            <div style={{ background: 'rgba(139, 92, 246, 0.1)', border: '1px solid rgba(139, 92, 246, 0.3)', borderRadius: '12px', padding: '24px', maxWidth: '700px', margin: '0 auto 32px' }}>
                <h3 style={{ color: '#c4b5fd', fontSize: '16px', fontWeight: 600, marginBottom: '16px' }}>✨ What's New in 1.0</h3>
                <ul style={{ color: '#e9d5ff', fontSize: '13px', lineHeight: 1.8, paddingLeft: '20px', margin: 0 }}>
                    <li><strong>Synced Rewind View</strong> — Visual timeline with synchronized audio + screen</li>
                    <li><strong>Screenshot Integration</strong> — Thumbnails with expand-to-view modal</li>
                    <li><strong>Keyboard Navigation</strong> — Use ↑↓ or j/k to navigate, / to search</li>
                    <li><strong>Search Everything</strong> — Full-text search across transcripts and screen text</li>
                    <li><strong>AI Intelligence</strong> — Summaries, action items, and key insights</li>
                </ul>
            </div>

            {/* Links */}
            <div style={{ textAlign: 'center', color: '#6b7280', fontSize: '12px' }}>
                <p style={{ marginBottom: '8px' }}>
                    <a href="mailto:support@nofriction.ai" style={{ color: '#7c3aed', textDecoration: 'none' }}>support@nofriction.ai</a>
                    {' • '}
                    <a href="https://nofriction.ai" style={{ color: '#7c3aed', textDecoration: 'none' }}>nofriction.ai</a>
                </p>
                <p style={{ margin: 0 }}>© 2026 noFriction AI. All rights reserved.</p>
            </div>
        </div>
    );
}

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
        const interval = setInterval(loadHealth, 10000);
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
        { id: 'about' as AdminTab, label: 'About', icon: '💜' },
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
                {activeTab === 'about' && <AboutPanel />}
            </div>
        </div>
    );
}

export default AdminConsole;
