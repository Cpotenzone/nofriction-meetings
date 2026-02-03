import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

interface MeetingDetection {
    id: string;
    detected_at: string;
    source: string;
    app_name: string | null;
    calendar_event: { title: string } | null;
    is_using_audio: boolean;
    is_screen_sharing: boolean;
}

interface MeetingDetectionBannerProps {
    onStartRecording?: () => void;
}

export const MeetingDetectionBanner: React.FC<MeetingDetectionBannerProps> = ({
    onStartRecording
}) => {
    const [detection, setDetection] = useState<MeetingDetection | null>(null);
    const [isVisible, setIsVisible] = useState(false);
    const [isAnimatingOut, setIsAnimatingOut] = useState(false);

    // Listen for meeting-detected events from backend
    useEffect(() => {
        const unlisten = listen<MeetingDetection>('meeting-detected', (event) => {
            setDetection(event.payload);
            setIsVisible(true);
            setIsAnimatingOut(false);
        });

        return () => {
            unlisten.then(fn => fn());
        };
    }, []);

    // Note: The backend now properly emits meeting-detected events for both
    // calendar events and app detection. We no longer need to poll here.
    // The backend's MeetingTriggerEngine handles:
    // - Calendar event detection (every 30 seconds)
    // - App detection (only when frontmost OR audio active)
    // This prevents false positives from background apps.

    const handleDismiss = useCallback(async () => {
        if (detection) {
            try {
                await invoke('dismiss_meeting_detection', { detectionId: detection.id });
            } catch (e) {
                console.error('Failed to dismiss detection:', e);
            }
        }
        setIsAnimatingOut(true);
        setTimeout(() => {
            setIsVisible(false);
            setDetection(null);
            setIsAnimatingOut(false);
        }, 300);
    }, [detection]);

    const handleStartRecording = useCallback(async () => {
        try {
            await invoke('start_meeting_capture');
            onStartRecording?.();
        } catch (e) {
            console.error('Failed to start recording:', e);
        }
        setIsAnimatingOut(true);
        setTimeout(() => {
            setIsVisible(false);
            setDetection(null);
            setIsAnimatingOut(false);
        }, 300);
    }, [onStartRecording]);

    if (!isVisible || !detection) return null;

    // Determine the icon based on source
    const getAppIcon = () => {
        const appName = detection.app_name?.toLowerCase() || '';
        if (appName.includes('zoom')) return '📹';
        if (appName.includes('teams')) return '👥';
        if (appName.includes('meet') || appName.includes('google')) return '🎥';
        if (appName.includes('slack')) return '💬';
        if (appName.includes('discord')) return '🎮';
        if (appName.includes('facetime')) return '📱';
        if (appName.includes('webex')) return '🌐';
        if (detection.source === 'Calendar') return '📅';
        return '🎤';
    };

    const getTitle = () => {
        if (detection.calendar_event) {
            return detection.calendar_event.title;
        }
        return detection.app_name || 'Meeting Detected';
    };

    const getSubtitle = () => {
        const parts: string[] = [];
        if (detection.is_using_audio) parts.push('🔴 Audio active');
        if (detection.source === 'Calendar') parts.push('📅 Calendar event');
        if (detection.source === 'AppDetection') parts.push('📱 App detected');
        return parts.join(' • ') || 'Would you like to record?';
    };

    return (
        <div
            className={`meeting-detection-banner ${isAnimatingOut ? 'sliding-out' : 'sliding-in'}`}
            role="alert"
            aria-live="polite"
        >
            <div className="mdb-icon">{getAppIcon()}</div>
            <div className="mdb-content">
                <div className="mdb-title">{getTitle()}</div>
                <div className="mdb-subtitle">{getSubtitle()}</div>
            </div>
            <div className="mdb-actions">
                <button
                    className="mdb-btn mdb-btn-primary"
                    onClick={handleStartRecording}
                    aria-label="Start recording this meeting"
                >
                    <span className="mdb-btn-icon">⏺</span>
                    Record
                </button>
                <button
                    className="mdb-btn mdb-btn-secondary"
                    onClick={handleDismiss}
                    aria-label="Dismiss suggestion"
                >
                    ✕
                </button>
            </div>
        </div>
    );
};

export default MeetingDetectionBanner;
