import React, { useState } from 'react';
import { MeetingHistory } from '../../MeetingHistory';
import { KBSearch } from '../../KBSearch';
import { InsightsView } from '../../InsightsView';
import { RewindGallery } from '../../RewindGallery';
import { MeetingNotesPanel } from '../../MeetingNotesPanel';
import { MeetingPeople, PeopleDirectory } from '../../People';
import { MeetingClassField } from '../../MeetingClass';

type DeckTab = 'history' | 'people' | 'insights' | 'search';

interface InsightDeckViewProps {
    onSelectMeeting: (id: string) => void;
    selectedMeetingId: string | null;
    refreshKey: number;
}

export const InsightDeckView: React.FC<InsightDeckViewProps> = ({ onSelectMeeting, selectedMeetingId, refreshKey }) => {
    const [activeTab, setActiveTab] = useState<DeckTab>('history');
    // Selected recording: screenshots + transcript, or its AI notes
    const [meetingView, setMeetingView] = useState<'rewind' | 'notes'>('rewind');
    // Bumped when a recording's class is edited, so the list and its filter refresh
    const [classEdits, setClassEdits] = useState(0);

    return (
        <div className="agency-view insight-deck">
            <header className="deck-header">
                <div className="deck-tabs">
                    <button
                        className={`deck-tab ${activeTab === 'history' ? 'active' : ''}`}
                        onClick={() => setActiveTab('history')}
                    >
                        RECORDINGS
                    </button>
                    <button
                        className={`deck-tab ${activeTab === 'people' ? 'active' : ''}`}
                        onClick={() => setActiveTab('people')}
                    >
                        PEOPLE
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
                        SEARCH
                    </button>
                </div>
            </header>

            <div className="deck-content">
                {activeTab === 'history' && (
                    <div className="deck-panel" style={{ display: 'grid', gridTemplateColumns: selectedMeetingId ? '350px 1fr' : '1fr', gap: 20 }}>
                        <MeetingHistory
                            onSelectMeeting={onSelectMeeting}
                            selectedMeetingId={selectedMeetingId}
                            refreshKey={refreshKey + classEdits}
                            compact={!!selectedMeetingId}
                        />
                        {selectedMeetingId && (
                            <div className="deck-playback-panel" style={{ overflow: 'hidden', height: '100%', display: 'flex', flexDirection: 'column' }}>
                                <MeetingClassField meetingId={selectedMeetingId} onSaved={() => setClassEdits((n) => n + 1)} />
                                <MeetingPeople meetingId={selectedMeetingId} />
                                <div className="deck-tabs" role="tablist" aria-label="Recording view" style={{ padding: '6px 0' }}>
                                    <button
                                        role="tab"
                                        aria-selected={meetingView === 'rewind'}
                                        className={`deck-tab ${meetingView === 'rewind' ? 'active' : ''}`}
                                        onClick={() => setMeetingView('rewind')}
                                    >
                                        REWIND
                                    </button>
                                    <button
                                        role="tab"
                                        aria-selected={meetingView === 'notes'}
                                        className={`deck-tab ${meetingView === 'notes' ? 'active' : ''}`}
                                        onClick={() => setMeetingView('notes')}
                                    >
                                        NOTES
                                    </button>
                                </div>
                                <div style={{ flex: 1, minHeight: 0 }}>
                                    {meetingView === 'rewind' ? (
                                        <RewindGallery meetingId={selectedMeetingId} isRecording={false} />
                                    ) : (
                                        <MeetingNotesPanel meetingId={selectedMeetingId} />
                                    )}
                                </div>
                            </div>
                        )}
                    </div>
                )}

                {activeTab === 'people' && (
                    <div className="deck-panel" style={{ overflowY: 'auto' }}>
                        <PeopleDirectory />
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
