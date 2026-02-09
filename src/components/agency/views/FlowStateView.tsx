import React, { useEffect, useState } from 'react';
import { LiveTranscriptView } from '../../LiveTranscript';
import { useRecording } from '../../../hooks/useRecording';
import { useTranscripts } from '../../../hooks/useTranscripts';
import { invoke } from '@tauri-apps/api/core';
import { LiveInsightEvent } from '../../../lib/tauri';
import { AnimatePresence, motion } from 'framer-motion';

interface FlowStateViewProps {
    recording: ReturnType<typeof useRecording>;
    transcripts: ReturnType<typeof useTranscripts>;
}

export const FlowStateView: React.FC<FlowStateViewProps> = ({ recording, transcripts }) => {
    const [insights, setInsights] = useState<LiveInsightEvent[]>([]);
    const [isPolling, setIsPolling] = useState(false);

    // Poll for live insights during recording
    useEffect(() => {
        if (!recording.isRecording || !recording.meetingId) {
            setInsights([]);
            return;
        }

        const fetchInsights = async () => {
            if (isPolling) return;
            setIsPolling(true);
            try {
                const result = await invoke<LiveInsightEvent[]>("get_live_insights", {
                    meetingId: recording.meetingId
                });
                setInsights(result.slice(-10).reverse()); // Show latest 10, newest first
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

    const getInsightIcon = (type: string) => {
        switch (type.toLowerCase()) {
            case 'action_item': return '📋';
            case 'decision': return '✅';
            case 'risk_signal': return '⚠️';
            case 'question_suggestion': return '❓';
            case 'commitment': return '🤝';
            case 'topic_shift': return '🎯';
            default: return '💡';
        }
    };

    return (
        <div className="agency-view flow-state">
            <div className="flow-content">
                {/* Main Transcript Area */}
                <div className="flow-transcript-container">
                    <LiveTranscriptView
                        isRecording={recording.isRecording}
                        transcripts={transcripts.liveTranscripts}
                    />
                </div>

                {/* Right Panel: Real-time Intelligence */}
                <aside className="flow-intelligence-panel">
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
                                    <span className="icon">✦</span>
                                    <p>{recording.isRecording ? "Analyzing conversation..." : "Start recording to see insights"}</p>
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
