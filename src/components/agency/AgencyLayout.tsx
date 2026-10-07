import React, { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import { motion, AnimatePresence } from 'framer-motion';
import { AgencyNavbar } from './AgencyNavbar';
import { FlowStateView } from './views/FlowStateView';
import { InsightDeckView } from './views/InsightDeckView';
import { ZenFocusView } from './views/ZenFocusView';
import { VaultView } from './views/VaultView';
import { IntelDashboard } from './views/IntelDashboard';
import { DataChat } from './views/DataChat';
import { HelpView } from './views/HelpView';
import { PromptStudio } from './views/PromptStudio';
import { useRecording } from '../../hooks/useRecording';
import { useTranscripts } from '../../hooks/useTranscripts';
import { AgencySettingsModal } from './AgencySettingsModal';
import { onOpenSettings, type SettingsCategory } from '../../lib/navigation';
import { getMeeting } from '../../lib/tauri';
import { segmentCarryOver } from '../../lib/recordPlan';
import { getTimedRecordingStatus } from '../../lib/timedRecording';

export type AgencyMode = 'flow' | 'deck' | 'zen' | 'vault' | 'intel' | 'chat' | 'help' | 'prompts';

interface AgencyLayoutProps {
    activeMode: AgencyMode;
    onModeChange: (mode: AgencyMode) => void;
    // Pass existing props needed by views
    recording: ReturnType<typeof useRecording>;
    transcripts: ReturnType<typeof useTranscripts>;
    onSelectMeeting: (id: string) => void;
    selectedMeetingId: string | null;
    onToggleRecording: () => void;
    refreshKey: number;
    onOpenCommandPalette?: () => void;
}

export const AgencyLayout: React.FC<AgencyLayoutProps> = ({
    activeMode,
    onModeChange,
    recording,
    transcripts,
    onSelectMeeting,
    selectedMeetingId,
    onToggleRecording,
    refreshKey,
    onOpenCommandPalette
}) => {
    const [isSettingsOpen, setIsSettingsOpen] = React.useState(false);
    const [settingsCategory, setSettingsCategory] = React.useState<SettingsCategory>('general');

    // "Open AI Engine" buttons, menu ⌘, and the command palette open Settings here
    useEffect(() => onOpenSettings((category) => {
        setSettingsCategory(category);
        setIsSettingsOpen(true);
    }), []);
    // const [segmentPrompt, setSegmentPrompt] = useState(false); // Removed for native dialog

    // Listen for recording segmentation prompt (75+ minutes)
    useEffect(() => {
        const unlisten = listen('recording_segment_prompt', async () => {
            if (recording.isRecording) {
                // Use native system dialog
                const shouldSegment = await ask(
                    'Recording has reached 75 minutes. Long recordings may lose quality.\n\nWould you like to start a new segment?',
                    {
                        title: 'Recording Limit Reached',
                        okLabel: 'Start New Segment',
                        cancelLabel: 'Keep Recording'
                    }
                );

                if (shouldSegment) {
                    handleSegmentConfirm();
                }
            }
        });
        return () => { unlisten.then(fn => fn()); };
    }, [recording.isRecording]);

    const handleSegmentConfirm = async () => {
        // The new segment keeps the class and the time that was left (no
        // "how long?" sheet: the user already chose)
        let carry = segmentCarryOver(null, null, Date.now());
        try {
            const [status, meeting] = await Promise.all([
                getTimedRecordingStatus(),
                recording.meetingId ? getMeeting(recording.meetingId) : Promise.resolve(null),
            ]);
            carry = segmentCarryOver(status, meeting?.class_name, Date.now());
        } catch (e) {
            console.warn('Segment carry-over unavailable:', e);
        }
        // Stop current recording, then start new one
        onToggleRecording(); // stop
        setTimeout(() => {
            transcripts.clearLiveTranscripts();
            recording.startRecording(carry).catch((e) => console.error('New segment failed to start:', e));
        }, 1500); // restart after brief pause
    };

    return (
        <div className="agency-layout">
            <div className="agency-background-layer" />

            <AgencyNavbar
                activeMode={activeMode}
                onModeChange={onModeChange}
                isRecording={recording.isRecording}
                onToggleRecording={onToggleRecording}
                onOpenSettings={() => { setSettingsCategory('general'); setIsSettingsOpen(true); }}
                onOpenCommandPalette={onOpenCommandPalette}
            />

            <main className="agency-content">
                <AnimatePresence mode="wait">
                    {activeMode === 'flow' && (
                        <motion.div
                            key="flow"
                            className="agency-view-container"
                            initial={{ opacity: 0, x: -20 }}
                            animate={{ opacity: 1, x: 0 }}
                            exit={{ opacity: 0, x: 20 }}
                            transition={{ duration: 0.3 }}
                        >
                            <FlowStateView recording={recording} transcripts={transcripts} />
                        </motion.div>
                    )}

                    {activeMode === 'deck' && (
                        <motion.div
                            key="deck"
                            className="agency-view-container"
                            initial={{ opacity: 0, scale: 0.98 }}
                            animate={{ opacity: 1, scale: 1 }}
                            exit={{ opacity: 0, scale: 0.98 }}
                            transition={{ duration: 0.3 }}
                        >
                            <InsightDeckView
                                onSelectMeeting={onSelectMeeting}
                                selectedMeetingId={selectedMeetingId}
                                refreshKey={refreshKey}
                            />
                        </motion.div>
                    )}

                    {activeMode === 'zen' && (
                        <motion.div
                            key="zen"
                            className="agency-view-container"
                            initial={{ opacity: 0, y: 20 }}
                            animate={{ opacity: 1, y: 0 }}
                            exit={{ opacity: 0, y: -20 }}
                            transition={{ duration: 0.3 }}
                        >
                            <ZenFocusView recording={recording} />
                        </motion.div>
                    )}

                    {activeMode === 'vault' && (
                        <motion.div
                            key="vault"
                            className="agency-view-container"
                            initial={{ opacity: 0, scale: 0.95 }}
                            animate={{ opacity: 1, scale: 1 }}
                            exit={{ opacity: 0, scale: 0.95 }}
                            transition={{ duration: 0.3 }}
                        >
                            <VaultView onSelectMeeting={onSelectMeeting} onOpenSettings={() => { setSettingsCategory('obsidian'); setIsSettingsOpen(true); }} />
                        </motion.div>
                    )}

                    {activeMode === 'intel' && (
                        <motion.div
                            key="intel"
                            className="agency-view-container"
                            initial={{ opacity: 0, y: 10 }}
                            animate={{ opacity: 1, y: 0 }}
                            exit={{ opacity: 0, y: -10 }}
                            transition={{ duration: 0.3 }}
                        >
                            <IntelDashboard />
                        </motion.div>
                    )}

                    {activeMode === 'chat' && (
                        <motion.div
                            key="chat"
                            className="agency-view-container"
                            initial={{ opacity: 0, y: 15 }}
                            animate={{ opacity: 1, y: 0 }}
                            exit={{ opacity: 0, y: -15 }}
                            transition={{ duration: 0.3 }}
                        >
                            <DataChat />
                        </motion.div>
                    )}

                    {activeMode === 'help' && (
                        <motion.div
                            key="help"
                            className="agency-view-container"
                            initial={{ opacity: 0, y: 15 }}
                            animate={{ opacity: 1, y: 0 }}
                            exit={{ opacity: 0, y: -15 }}
                            transition={{ duration: 0.3 }}
                        >
                            <HelpView />
                        </motion.div>
                    )}

                    {activeMode === 'prompts' && (
                        <motion.div
                            key="prompts"
                            className="agency-view-container"
                            initial={{ opacity: 0, y: 15 }}
                            animate={{ opacity: 1, y: 0 }}
                            exit={{ opacity: 0, y: -15 }}
                            transition={{ duration: 0.3 }}
                        >
                            <PromptStudio />
                        </motion.div>
                    )}
                </AnimatePresence>
            </main>

            <AgencySettingsModal
                isOpen={isSettingsOpen}
                onClose={() => setIsSettingsOpen(false)}
                initialCategory={settingsCategory}
            />
        </div>
    );
};
