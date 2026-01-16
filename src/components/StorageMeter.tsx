// Storage Meter Component
// Visual disk usage indicator for video recordings

import { useState, useEffect } from 'react';
import * as tauri from '../lib/tauri';

interface StorageStats {
    total_bytes: number;
    video_bytes: number;
    frames_bytes: number;
    meetings_count: number;
    chunks_count: number;
    usage_percent: number;
    disk_limit_bytes: number;
}

export function StorageMeter() {
    const [stats, setStats] = useState<StorageStats | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        const fetchStats = async () => {
            try {
                const data = await tauri.getStorageStats();
                setStats(data);
                setError(null);
            } catch (err) {
                setError('Failed to load storage stats');
                console.error(err);
            } finally {
                setIsLoading(false);
            }
        };

        fetchStats();
        // Refresh every 30 seconds
        const interval = setInterval(fetchStats, 30000);
        return () => clearInterval(interval);
    }, []);

    const formatSize = (bytes: number): string => {
        if (bytes >= 1024 * 1024 * 1024) {
            return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
        }
        if (bytes >= 1024 * 1024) {
            return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
        }
        if (bytes >= 1024) {
            return `${(bytes / 1024).toFixed(1)} KB`;
        }
        return `${bytes} B`;
    };

    const getStatusColor = (percent: number): string => {
        if (percent >= 90) return 'var(--accent-red, #ef4444)';
        if (percent >= 75) return 'var(--accent-orange, #f59e0b)';
        return 'var(--accent-green, #10b981)';
    };

    if (isLoading) {
        return (
            <div className="storage-meter loading">
                <div className="storage-icon">💾</div>
                <span>Loading...</span>
            </div>
        );
    }

    if (error || !stats) {
        return null; // Don't show if error
    }

    const usagePercent = Math.min(stats.usage_percent, 100);

    return (
        <div className="storage-meter">
            <div className="storage-header">
                <span className="storage-icon">💾</span>
                <span className="storage-label">Storage</span>
                <span className="storage-value">{formatSize(stats.total_bytes)}</span>
            </div>

            <div className="storage-bar">
                <div
                    className="storage-fill"
                    style={{
                        width: `${usagePercent}%`,
                        backgroundColor: getStatusColor(usagePercent),
                    }}
                />
            </div>

            <div className="storage-details">
                <span className="storage-detail">
                    🎬 {stats.chunks_count} video chunks
                </span>
                <span className="storage-detail">
                    📁 {stats.meetings_count} meetings
                </span>
            </div>

            {usagePercent >= 90 && (
                <div className="storage-warning">
                    ⚠️ Storage nearly full
                </div>
            )}
        </div>
    );
}

// Hook for storage stats
export function useStorageStats() {
    const [stats, setStats] = useState<StorageStats | null>(null);
    const [isLoading, setIsLoading] = useState(true);

    useEffect(() => {
        const fetchStats = async () => {
            try {
                const data = await tauri.getStorageStats();
                setStats(data);
            } catch (err) {
                console.error('Failed to get storage stats:', err);
            } finally {
                setIsLoading(false);
            }
        };

        fetchStats();
    }, []);

    const refresh = async () => {
        const data = await tauri.getStorageStats();
        setStats(data);
    };

    const cleanup = async () => {
        const [deleted, freed] = await tauri.applyRetention();
        await refresh();
        return { deleted, freed };
    };

    return { stats, isLoading, refresh, cleanup };
}

export default StorageMeter;
