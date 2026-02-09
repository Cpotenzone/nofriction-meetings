import React, { useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import { LiveInsightEvent } from "../lib/tauri";

interface GenieViewProps {
    onRestore: () => void;
    liveTranscripts: string[];
    isRecording: boolean;
    onStop: () => void;
    meetingId: string | null;
}

export const GenieView: React.FC<GenieViewProps> = ({ onRestore, liveTranscripts, isRecording, onStop, meetingId }) => {
    const [insights, setInsights] = useState<LiveInsightEvent[]>([]);
    const [isAnalyzing, setIsAnalyzing] = useState(false);
    const lastFetchRef = useRef<number>(0);

    // Get the last 2 transcripts for display
    const recentTranscripts = liveTranscripts.slice(-2);

    // Fetch real AI insights periodically
    useEffect(() => {
        if (!isRecording || !meetingId) {
            setInsights([]);
            return;
        }

        const fetchInsights = async () => {
            const now = Date.now();
            if (now - lastFetchRef.current < 4000) return; // Rate limit polling
            lastFetchRef.current = now;

            setIsAnalyzing(true);
            try {
                const result = await invoke<LiveInsightEvent[]>("get_live_insights", {
                    meetingId
                });
                setInsights(result.slice(-5)); // Keep latest 5
            } catch (err) {
                console.error("Genie failed to fetch insights:", err);
            } finally {
                setIsAnalyzing(false);
            }
        };

        const interval = setInterval(fetchInsights, 5000);
        fetchInsights();

        return () => clearInterval(interval);
    }, [isRecording, meetingId]);

    const formatTime = () => {
        const now = new Date();
        return now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
    };

    const handleRestore = async () => {
        const { setGenieMode } = await import("../lib/tauri");
        await setGenieMode(false);
        onRestore();
    };

    const getInsightColorClass = (type: string) => {
        switch (type.toLowerCase()) {
            case 'action_item': return 'insight-action';
            case 'decision': return 'insight-connection';
            case 'risk_signal': return 'insight-risk';
            case 'question_suggestion': return 'insight-question';
            case 'commitment': return 'insight-commitment';
            case 'topic_shift': return 'insight-topic';
            default: return 'insight-context';
        }
    };

    const latestInsight = insights[insights.length - 1];

    return (
        <motion.div
            className="genie-container"
            initial={{ opacity: 0, scale: 0.8 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.8 }}
            transition={{ duration: 0.3, ease: "easeOut" }}
        >
            <div className="genie-glass genie-enhanced">
                {/* Header */}
                <div className="genie-header" data-tauri-drag-region>
                    <div className={`genie-status ${isRecording ? 'recording' : ''}`} />
                    <span>GENIE MODE</span>
                    <div className="genie-header-right">
                        <span className="genie-time">{formatTime()}</span>
                    </div>
                </div>

                {/* Main Content: Latest Transcripts */}
                <div className="genie-main-content">
                    {recentTranscripts.length === 0 ? (
                        <div className="genie-awaiting">
                            <div className="genie-pulse-icon">✦</div>
                            <span>Listening for intelligence...</span>
                        </div>
                    ) : (
                        <div className="genie-transcript-display">
                            <AnimatePresence mode="popLayout">
                                {recentTranscripts.map((text, index) => (
                                    <motion.div
                                        key={`${liveTranscripts.length - 2 + index}-${text.slice(0, 15)}`}
                                        className={`genie-sentence ${index === recentTranscripts.length - 1 ? 'current' : 'previous'}`}
                                        initial={{ opacity: 0, y: 15 }}
                                        animate={{ opacity: 1, y: 0 }}
                                        exit={{ opacity: 0, y: -15 }}
                                        transition={{ duration: 0.3 }}
                                    >
                                        <span className="sentence-marker">{index === recentTranscripts.length - 1 ? '▸' : '▹'}</span>
                                        <p>{text}</p>
                                    </motion.div>
                                ))}
                            </AnimatePresence>
                        </div>
                    )}
                </div>

                {/* AI Insight Panel */}
                <div className="genie-insight-panel">
                    <div className="insight-header">
                        <span className="insight-label">AI INSIGHT</span>
                        {isAnalyzing && <span className="insight-analyzing">analyzing...</span>}
                    </div>
                    <AnimatePresence mode="wait">
                        {latestInsight ? (
                            <motion.div
                                key={latestInsight.id}
                                className={`insight-content ${getInsightColorClass(latestInsight.type)}`}
                                initial={{ opacity: 0, x: -10 }}
                                animate={{ opacity: 1, x: 0 }}
                                exit={{ opacity: 0, x: 10 }}
                                transition={{ duration: 0.3 }}
                            >
                                <span style={{ marginRight: '8px', fontWeight: 'bold' }}>
                                    {latestInsight.type.replace('_', ' ').toUpperCase()}:
                                </span>
                                {latestInsight.text || latestInsight.context}
                            </motion.div>
                        ) : (
                            <motion.div
                                className="insight-content insight-idle"
                                initial={{ opacity: 0 }}
                                animate={{ opacity: 0.6 }}
                            >
                                {isRecording ? "Waiting for enough context..." : "Ready to provide insights"}
                            </motion.div>
                        )}
                    </AnimatePresence>
                </div>

                {/* Footer with Controls */}
                <div className="genie-footer">
                    <div className="genie-footer-stats">
                        <span className="genie-line-count">{liveTranscripts.length} captured</span>
                    </div>

                    <div className="genie-controls">
                        {isRecording && (
                            <motion.button
                                className="genie-stop-btn"
                                whileHover={{ scale: 1.05 }}
                                whileTap={{ scale: 0.95 }}
                                onClick={onStop}
                            >
                                ■ STOP
                            </motion.button>
                        )}

                        <motion.div
                            className="genie-restore-btn"
                            whileHover={{ scale: 1.08 }}
                            whileTap={{ scale: 0.92 }}
                            onClick={handleRestore}
                        >
                            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                                <polyline points="15 3 21 3 21 9" />
                                <polyline points="9 21 3 21 3 15" />
                                <line x1="21" y1="3" x2="14" y2="10" />
                                <line x1="3" y1="21" x2="10" y2="14" />
                            </svg>
                            <span>Expand</span>
                        </motion.div>
                    </div>
                </div>
            </div>
        </motion.div>
    );
};
