import React, { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { LiveTranscriptView } from '../../LiveTranscript';
import { CaptureBar, CaptureFilmstrip } from '../../CaptureBar';
import { useRecording } from '../../../hooks/useRecording';
import { useTranscripts } from '../../../hooks/useTranscripts';
import { invoke } from '@tauri-apps/api/core';
import { LiveInsightEvent } from '../../../lib/tauri';
import { AiSetupNotice, useAiStatus } from '../../AiSetupNotice';
import { AnimatePresence, motion } from 'framer-motion';
import { useRecordPicker } from '../../RecordPicker';
import { PrivacyPromiseBadge } from '../../PrivacyPromise';
import {
    CheckSquareIcon,
    CheckIcon,
    WarningIcon,
    QuestionIcon,
    UsersIcon,
    TargetIcon,
    LightbulbIcon,
    SparkleIcon,
} from '../../icons';

interface FlowStateViewProps {
    recording: ReturnType<typeof useRecording>;
    transcripts: ReturnType<typeof useTranscripts>;
}

interface TranscriptionStatus {
    connected: boolean;
    provider: string;
    error: string | null;
}

export const FlowStateView: React.FC<FlowStateViewProps> = ({ recording, transcripts }) => {
    const recordPicker = useRecordPicker();
    const [insights, setInsights] = useState<LiveInsightEvent[]>([]);
    const [isPolling, setIsPolling] = useState(false);
    const [sttStatus, setSttStatus] = useState<TranscriptionStatus | null>(null);
    // No AI provider yet: say so (with a way to add one) instead of implying analysis runs
    const { configured: aiConfigured, refresh: refreshAi } = useAiStatus();
    // "Live insights during meetings" (Settings → AI Engine → Automatic AI)
    const [liveInsightsOn, setLiveInsightsOn] = useState(true);
    useEffect(() => {
        refreshAi();
        invoke<{ liveInsights: boolean }>('get_ai_automation')
            .then((a) => setLiveInsightsOn(a.liveInsights))
            .catch(() => setLiveInsightsOn(true));
    }, [recording.isRecording, refreshAi]);

    // Surface transcription health — historically failures were silent and
    // users got screenshots with no transcript and no explanation.
    useEffect(() => {
        let unlisten: (() => void) | null = null;
        listen<TranscriptionStatus>('transcription_status', (e) => {
            setSttStatus(e.payload);
        }).then((fn) => { unlisten = fn; });
        return () => { unlisten?.(); };
    }, []);

    // Poll for live insights during recording
    useEffect(() => {
        const activeMeetingId = recording.isRecording ? recording.meetingId : null;
        if (!activeMeetingId) return;

        const fetchInsights = async () => {
            if (isPolling) return;
            setIsPolling(true);
            try {
                const result = await invoke<LiveInsightEvent[]>("get_live_insights", {
                    meetingId: activeMeetingId
                });
                setInsights(result.slice(-20).reverse()); // Show latest 20, newest first
            } catch (err) {
                console.error("Failed to fetch live insights:", err);
            } finally {
                setIsPolling(false);
            }
        };

        // Fetch immediately and then poll
        fetchInsights();
        const interval = setInterval(fetchInsights, 5000);

        return () => clearInterval(interval);
    }, [recording.isRecording, recording.meetingId]);

    const getInsightIcon = (type: string): React.ReactNode => {
        switch (type.toLowerCase()) {
            case 'action_item': return <CheckSquareIcon size={13} />;
            case 'decision': return <CheckIcon size={13} />;
            case 'risk_signal': return <WarningIcon size={13} />;
            case 'question_suggestion': return <QuestionIcon size={13} />;
            case 'commitment': return <UsersIcon size={13} />;
            case 'topic_shift': return <TargetIcon size={13} />;
            default: return <LightbulbIcon size={13} />;
        }
    };

    return (
        <div className="agency-view flow-state">
            <CaptureBar isRecording={recording.isRecording} sttStatus={sttStatus} audioWarning={recording.audioWarning} />
            <div className="flow-content">
                {/* Main Transcript Area */}
                <div className="flow-transcript-container">
                    <LiveTranscriptView
                        isRecording={recording.isRecording}
                        transcripts={transcripts.liveTranscripts}
                        // A Record button: ask "how long?" first (App starts it)
                        onStartRecording={recordPicker.open}
                    />
                    {/* The empty home screen shows the promise in full; with a
                        transcript on screen it stays as a small badge */}
                    {transcripts.liveTranscripts.length > 0 && <PrivacyPromiseBadge />}
                </div>

                {/* Right Panel: Real-time Intelligence */}
                <aside className="flow-intelligence-panel">
                    <CaptureFilmstrip isRecording={recording.isRecording} />
                    <div className="panel-header">
                        <h3>LIVE INTELLIGENCE</h3>
                        <div className={`live-indicator ${recording.isRecording ? 'active' : ''}`}>
                            <span className="pulse-dot"></span>
                            {recording.isRecording ? 'ACTIVE' : 'IDLE'}
                        </div>
                    </div>

                    <div className="intelligence-stream">
                        <AnimatePresence mode="popLayout">
                            {insights.length === 0 ? (
                                <motion.div
                                    key="placeholder"
                                    initial={{ opacity: 0 }}
                                    animate={{ opacity: 1 }}
                                    exit={{ opacity: 0 }}
                                    className="placeholder-card"
                                >
                                    <span className="icon"><SparkleIcon size={22} strokeWidth={1.5} /></span>
                                    <p>
                                        {!liveInsightsOn
                                            ? "Live insights are off. Turn them on in Settings → AI Engine → Automatic AI."
                                            : recording.isRecording
                                                ? "Listening for action items, decisions and risks…"
                                                : "Action items, decisions and risks surface here while you record."}
                                    </p>
                                    {aiConfigured === false && (
                                        <AiSetupNotice feature="After-meeting AI notes" compact />
                                    )}
                                </motion.div>
                            ) : (
                                insights.map((insight) => (
                                    <motion.div
                                        key={insight.id}
                                        layout
                                        initial={{ opacity: 0, x: 20 }}
                                        animate={{ opacity: 1, x: 0 }}
                                        className={`insight-card type-${insight.type.toLowerCase()}`}
                                    >
                                        <div className="insight-header">
                                            <span className="insight-icon">{getInsightIcon(insight.type)}</span>
                                            <span className="insight-type">{insight.type.replace('_', ' ').toUpperCase()}</span>
                                            <span className="insight-time">
                                                {new Date(insight.timestamp_ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                                            </span>
                                        </div>
                                        <p className="insight-text">{insight.text || insight.context}</p>
                                        {insight.assignee && (
                                            <div className="insight-assignee">
                                                <span>Assignee:</span> {insight.assignee}
                                            </div>
                                        )}
                                    </motion.div>
                                ))
                            )}
                        </AnimatePresence>
                    </div>
                </aside>
            </div>
        </div>
    );
};
