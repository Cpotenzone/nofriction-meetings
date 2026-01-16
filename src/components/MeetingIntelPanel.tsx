// Meeting Intelligence Panel
// 3-mode panel: Pre-Meeting Brief, Live Insights, Late-Join Catch-Up

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

// Types
interface MeetingState {
    meeting_id: string | null;
    mode: 'pre' | 'live' | 'catchup';
    minutes_since_start: number;
    minutes_until_start: number;
    confidence: number;
    title: string;
    attendees: string[];
    is_transcript_running: boolean;
    is_meeting_window_active: boolean;
}

interface InsightItem {
    text: string;
    importance: number;
}

interface Decision {
    text: string;
    made_by: string | null;
}

interface RiskSignal {
    text: string;
    severity: number;
    signal_type: string;
}

interface CatchUpCapsule {
    what_missed: InsightItem[];
    current_topic: string;
    decisions: Decision[];
    open_threads: string[];
    next_moves: string[];
    risks: RiskSignal[];
    questions_to_ask: string[];
    ten_second_version: string;
    sixty_second_version: string;
    confidence: number;
    generated_at_minute: number;
}

interface LiveInsightEvent {
    type: string;
    id: string;
    text?: string;
    assignee?: string;
    context?: string;
    severity?: number;
    by?: string;
    from_topic?: string;
    to_topic?: string;
    reason?: string;
    timestamp_ms: number;
}

type IntelMode = 'pre' | 'live' | 'catchup';

interface MeetingIntelPanelProps {
    meetingId?: string;
    isRecording: boolean;
    onStartRecording?: () => void;
}

export function MeetingIntelPanel({
    meetingId,
    isRecording,
    onStartRecording
}: MeetingIntelPanelProps) {
    const [mode, setMode] = useState<IntelMode>('pre');
    const [manualOverride, setManualOverride] = useState(false);
    const [meetingState, setMeetingState] = useState<MeetingState | null>(null);
    const [catchUpCapsule, setCatchUpCapsule] = useState<CatchUpCapsule | null>(null);
    const [liveInsights, setLiveInsights] = useState<LiveInsightEvent[]>([]);
    const [showTenSecond, setShowTenSecond] = useState(false);
    const [isLoading, setIsLoading] = useState(false);

    // Fetch meeting state periodically
    useEffect(() => {
        const fetchState = async () => {
            try {
                const state = await invoke<MeetingState>('get_meeting_state');
                setMeetingState(state);

                // Auto-switch mode based on state (unless manually overridden)
                if (!manualOverride) {
                    if (state.is_transcript_running && state.minutes_since_start >= 2) {
                        setMode('catchup');
                    } else if (state.is_transcript_running) {
                        setMode('live');
                    } else if (state.minutes_until_start > 0 && state.minutes_until_start <= 30) {
                        setMode('pre');
                    }
                }
            } catch (err) {
                console.error('Failed to get meeting state:', err);
            }
        };

        fetchState();
        const interval = setInterval(fetchState, 5000);
        return () => clearInterval(interval);
    }, [manualOverride]);

    // Fetch catch-up capsule when in catch-up mode
    useEffect(() => {
        if (mode === 'catchup' && meetingId) {
            const fetchCatchUp = async () => {
                setIsLoading(true);
                try {
                    const capsule = await invoke<CatchUpCapsule>('generate_catch_up', {
                        meetingId
                    });
                    setCatchUpCapsule(capsule);
                } catch (err) {
                    console.error('Failed to generate catch-up:', err);
                } finally {
                    setIsLoading(false);
                }
            };

            fetchCatchUp();
            // Refresh every 60 seconds
            const interval = setInterval(fetchCatchUp, 60000);
            return () => clearInterval(interval);
        }
    }, [mode, meetingId]);

    // Fetch live insights when in live mode
    useEffect(() => {
        if (mode === 'live' && meetingId) {
            const fetchInsights = async () => {
                try {
                    const insights = await invoke<LiveInsightEvent[]>('get_live_insights', {
                        meetingId
                    });
                    setLiveInsights(insights);
                } catch (err) {
                    console.error('Failed to get live insights:', err);
                }
            };

            fetchInsights();
            const interval = setInterval(fetchInsights, 3000);
            return () => clearInterval(interval);
        }
    }, [mode, meetingId]);

    const handleModeChange = (newMode: IntelMode) => {
        setMode(newMode);
        setManualOverride(true);
    };

    const handlePinInsight = async (insight: LiveInsightEvent) => {
        if (!meetingId) return;
        try {
            await invoke('pin_insight', {
                meetingId,
                insightType: insight.type,
                insightText: insight.text || '',
                timestampMs: insight.timestamp_ms,
            });
        } catch (err) {
            console.error('Failed to pin insight:', err);
        }
    };

    const handleMarkDecision = async (text: string) => {
        if (!meetingId) return;
        try {
            await invoke('mark_decision', {
                meetingId,
                decisionText: text,
                context: null,
            });
        } catch (err) {
            console.error('Failed to mark decision:', err);
        }
    };

    const getModeLabel = () => {
        switch (mode) {
            case 'pre': return 'PRE';
            case 'live': return 'LIVE';
            case 'catchup': return 'CATCH-UP';
        }
    };

    const getModeColor = () => {
        switch (mode) {
            case 'pre': return 'var(--accent-purple, #8b5cf6)';
            case 'live': return 'var(--accent-green, #10b981)';
            case 'catchup': return 'var(--accent-orange, #f59e0b)';
        }
    };

    return (
        <div className="meeting-intel-panel">
            {/* Header */}
            <div className="intel-header">
                <div className="intel-title-row">
                    <h2>{meetingState?.title || 'Meeting Intelligence'}</h2>
                    <span
                        className="intel-mode-pill"
                        style={{ backgroundColor: getModeColor() }}
                    >
                        {getModeLabel()}
                    </span>
                </div>

                {meetingState && meetingState.minutes_since_start > 0 && (
                    <div className="intel-timer">
                        Meeting started {meetingState.minutes_since_start} min ago
                    </div>
                )}

                {/* Mode Switcher */}
                <div className="intel-mode-switcher">
                    <button
                        className={`mode-btn ${mode === 'pre' ? 'active' : ''}`}
                        onClick={() => handleModeChange('pre')}
                    >
                        Pre
                    </button>
                    <button
                        className={`mode-btn ${mode === 'live' ? 'active' : ''}`}
                        onClick={() => handleModeChange('live')}
                    >
                        Live
                    </button>
                    <button
                        className={`mode-btn ${mode === 'catchup' ? 'active' : ''}`}
                        onClick={() => handleModeChange('catchup')}
                    >
                        Catch-Up
                    </button>
                </div>
            </div>

            {/* Content Area */}
            <div className="intel-content">
                {/* Pre-Meeting Mode */}
                {mode === 'pre' && (
                    <PreBriefContent meetingState={meetingState} />
                )}

                {/* Live Mode */}
                {mode === 'live' && (
                    <LiveInsightContent
                        insights={liveInsights}
                        onPinInsight={handlePinInsight}
                        onMarkDecision={handleMarkDecision}
                    />
                )}

                {/* Catch-Up Mode */}
                {mode === 'catchup' && (
                    <CatchUpContent
                        capsule={catchUpCapsule}
                        showTenSecond={showTenSecond}
                        onToggleTenSecond={() => setShowTenSecond(!showTenSecond)}
                        isLoading={isLoading}
                    />
                )}
            </div>

            {/* Action Bar */}
            <div className="intel-actions">
                {!isRecording && (
                    <button className="intel-action-btn primary" onClick={onStartRecording}>
                        ● Start Live Capture
                    </button>
                )}
                {mode === 'catchup' && (
                    <button
                        className="intel-action-btn"
                        onClick={() => setShowTenSecond(!showTenSecond)}
                    >
                        {showTenSecond ? '📖 Full Version' : '⚡ 10-Second Version'}
                    </button>
                )}
            </div>
        </div>
    );
}

// Pre-Brief Content Component
function PreBriefContent({ meetingState }: { meetingState: MeetingState | null }) {
    if (!meetingState || !meetingState.meeting_id) {
        return (
            <div className="intel-empty">
                <div className="intel-empty-icon">📅</div>
                <p>No upcoming meeting detected</p>
                <p className="intel-hint">Open a meeting invite or wait for a scheduled meeting</p>
            </div>
        );
    }

    return (
        <div className="pre-brief-content">
            <div className="brief-section">
                <h3>📋 Meeting Brief</h3>
                <p className="meeting-title">{meetingState.title}</p>
                {meetingState.minutes_until_start > 0 && (
                    <p className="time-hint">Starts in {meetingState.minutes_until_start} minutes</p>
                )}
            </div>

            {meetingState.attendees.length > 0 && (
                <div className="brief-section">
                    <h3>👥 Attendees</h3>
                    <div className="attendee-list">
                        {meetingState.attendees.map((attendee, i) => (
                            <div key={i} className="attendee-card">
                                <span className="attendee-avatar">
                                    {attendee.charAt(0).toUpperCase()}
                                </span>
                                <span className="attendee-name">{attendee}</span>
                            </div>
                        ))}
                    </div>
                </div>
            )}

            <div className="brief-section">
                <h3>💡 Suggested Questions</h3>
                <ul className="question-list">
                    <li>What are the key objectives for this meeting?</li>
                    <li>Are there any blockers we should address?</li>
                    <li>What decisions need to be made today?</li>
                </ul>
            </div>
        </div>
    );
}

// Live Insight Content Component
function LiveInsightContent({
    insights,
    onPinInsight,
    onMarkDecision: _onMarkDecision,
}: {
    insights: LiveInsightEvent[];
    onPinInsight: (insight: LiveInsightEvent) => void;
    onMarkDecision: (text: string) => void;
}) {
    if (insights.length === 0) {
        return (
            <div className="intel-empty">
                <div className="intel-empty-icon">🎙️</div>
                <p>Listening for insights...</p>
                <p className="intel-hint">Action items, decisions, and risks will appear here</p>
            </div>
        );
    }

    const getInsightIcon = (type: string) => {
        switch (type) {
            case 'action_item': return '☑️';
            case 'decision': return '⚖️';
            case 'risk_signal': return '⚠️';
            case 'question_suggestion': return '💡';
            case 'commitment': return '🤝';
            case 'topic_shift': return '↪️';
            default: return '📌';
        }
    };

    return (
        <div className="live-insight-content">
            {insights.map((insight) => (
                <div key={insight.id} className={`insight-card ${insight.type}`}>
                    <div className="insight-icon">{getInsightIcon(insight.type)}</div>
                    <div className="insight-body">
                        <div className="insight-text">{insight.text}</div>
                        {insight.assignee && (
                            <div className="insight-meta">Assigned to: {insight.assignee}</div>
                        )}
                        {insight.reason && (
                            <div className="insight-meta">{insight.reason}</div>
                        )}
                    </div>
                    <button
                        className="insight-pin-btn"
                        onClick={() => onPinInsight(insight)}
                        title="Pin this insight"
                    >
                        📌
                    </button>
                </div>
            ))}
        </div>
    );
}

// Catch-Up Content Component
function CatchUpContent({
    capsule,
    showTenSecond,
    onToggleTenSecond,
    isLoading,
}: {
    capsule: CatchUpCapsule | null;
    showTenSecond: boolean;
    onToggleTenSecond: () => void;
    isLoading: boolean;
}) {
    if (isLoading) {
        return (
            <div className="intel-loading">
                <div className="loading-spinner" />
                <p>Generating catch-up summary...</p>
            </div>
        );
    }

    if (!capsule) {
        return (
            <div className="intel-empty">
                <div className="intel-empty-icon">🎯</div>
                <p>No transcript data available</p>
                <p className="intel-hint">Start recording to enable catch-up summaries</p>
            </div>
        );
    }

    if (showTenSecond) {
        return (
            <div className="catch-up-ten-second">
                <div className="ten-second-header">
                    <span className="ten-second-label">⚡ 10-Second Version</span>
                    <button onClick={onToggleTenSecond} className="expand-btn">
                        Show Full
                    </button>
                </div>
                <div className="ten-second-content">
                    {capsule.ten_second_version}
                </div>
            </div>
        );
    }

    return (
        <div className="catch-up-content">
            {/* Current Topic */}
            <div className="catchup-section current-topic">
                <h3>📍 Current Topic</h3>
                <p className="topic-text">{capsule.current_topic}</p>
            </div>

            {/* What I Missed */}
            {capsule.what_missed.length > 0 && (
                <div className="catchup-section">
                    <h3>📋 What I Missed</h3>
                    <ul className="missed-list">
                        {capsule.what_missed.map((item, i) => (
                            <li key={i}>{item.text}</li>
                        ))}
                    </ul>
                </div>
            )}

            {/* Decisions Made */}
            {capsule.decisions.length > 0 && (
                <div className="catchup-section decisions">
                    <h3>✅ Decisions Made</h3>
                    <ul className="decision-list">
                        {capsule.decisions.map((decision, i) => (
                            <li key={i}>
                                {decision.text}
                                {decision.made_by && <span className="decision-by"> — {decision.made_by}</span>}
                            </li>
                        ))}
                    </ul>
                </div>
            )}

            {/* Next Moves */}
            {capsule.next_moves.length > 0 && (
                <div className="catchup-section next-moves">
                    <h3>🎯 My Next Best Move</h3>
                    <ul className="next-move-list">
                        {capsule.next_moves.map((move, i) => (
                            <li key={i} className="next-move-item">{move}</li>
                        ))}
                    </ul>
                </div>
            )}

            {/* Risks */}
            {capsule.risks.length > 0 && (
                <div className="catchup-section risks">
                    <h3>⚠️ Landmines / Risks</h3>
                    <ul className="risk-list">
                        {capsule.risks.map((risk, i) => (
                            <li key={i} className="risk-item">{risk.text}</li>
                        ))}
                    </ul>
                </div>
            )}

            {/* Questions to Ask */}
            {capsule.questions_to_ask.length > 0 && (
                <div className="catchup-section questions">
                    <h3>❓ Good Questions to Ask</h3>
                    <ul className="question-list">
                        {capsule.questions_to_ask.map((q, i) => (
                            <li key={i}>{q}</li>
                        ))}
                    </ul>
                </div>
            )}

            {/* Confidence indicator */}
            <div className="catchup-confidence">
                <span className="confidence-label">Confidence:</span>
                <div className="confidence-bar">
                    <div
                        className="confidence-fill"
                        style={{ width: `${capsule.confidence * 100}%` }}
                    />
                </div>
                <span className="confidence-value">{Math.round(capsule.confidence * 100)}%</span>
            </div>
        </div>
    );
}

export default MeetingIntelPanel;
