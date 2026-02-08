import React, { useState } from 'react';
import { MeetingHistory } from '../../MeetingHistory';
import { KBSearch } from '../../KBSearch';
import { InsightsView } from '../../InsightsView';
import { RewindGallery } from '../../RewindGallery';

type DeckTab = 'history' | 'insights' | 'search';

interface InsightDeckViewProps {
    onSelectMeeting: (id: string) => void;
    selectedMeetingId: string | null;
    refreshKey: number;
}

export const InsightDeckView: React.FC<InsightDeckViewProps> = ({ onSelectMeeting, selectedMeetingId, refreshKey }) => {
    const [activeTab, setActiveTab] = useState<DeckTab>('history');

    return (
        <div className="agency-view insight-deck">
            <header className="deck-header">
                <div className="deck-tabs">
                    <button
                        className={`deck-tab ${activeTab === 'history' ? 'active' : ''}`}
                        onClick={() => setActiveTab('history')}
                    >
                        HISTORY
                    </button>
                    <button
                        className={`deck-tab ${activeTab === 'insights' ? 'active' : ''}`}
                        onClick={() => setActiveTab('insights')}
                    >
                        INSIGHTS
                    </button>
                    <button
                        className={`deck-tab ${activeTab === 'search' ? 'active' : ''}`}
                        onClick={() => setActiveTab('search')}
                    >
                        KNOWLEDGE BASE
                    </button>
                </div>
            </header>

            <div className="deck-content">
                {activeTab === 'history' && (
                    <div className="deck-panel" style={{ display: 'grid', gridTemplateColumns: selectedMeetingId ? '350px 1fr' : '1fr', gap: 20 }}>
                        <MeetingHistory
                            onSelectMeeting={onSelectMeeting}
                            selectedMeetingId={selectedMeetingId}
                            refreshKey={refreshKey}
                            compact={!!selectedMeetingId}
                        />
                        {selectedMeetingId && (
                            <div className="deck-playback-panel" style={{ overflow: 'hidden', height: '100%' }}>
                                <RewindGallery meetingId={selectedMeetingId} isRecording={false} />
                            </div>
                        )}
                    </div>
                )}

                {activeTab === 'insights' && (
                    <div className="deck-panel">
                        <InsightsView />
                    </div>
                )}

                {activeTab === 'search' && (
                    <div className="deck-panel">
                        <KBSearch />
                    </div>
                )}
            </div>
        </div>
    );
};
