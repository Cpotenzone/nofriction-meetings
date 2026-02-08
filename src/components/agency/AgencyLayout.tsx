import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { AgencyNavbar } from './AgencyNavbar';
import { FlowStateView } from './views/FlowStateView';
import { InsightDeckView } from './views/InsightDeckView';
import { ZenFocusView } from './views/ZenFocusView';
import { VaultView } from './views/VaultView';
import { useRecording } from '../../hooks/useRecording';
import { useTranscripts } from '../../hooks/useTranscripts';
import { AgencySettingsModal } from './AgencySettingsModal';

export type AgencyMode = 'flow' | 'deck' | 'zen' | 'vault';

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
}

export const AgencyLayout: React.FC<AgencyLayoutProps> = ({
    activeMode,
    onModeChange,
    recording,
    transcripts,
    onSelectMeeting,
    selectedMeetingId,
    onToggleRecording,
    refreshKey
}) => {
    const [isSettingsOpen, setIsSettingsOpen] = React.useState(false);

    return (
        <div className="agency-layout">
            <div className="agency-background-layer" />

            <AgencyNavbar
                activeMode={activeMode}
                onModeChange={onModeChange}
                isRecording={recording.isRecording}
                onToggleRecording={onToggleRecording}
                onOpenSettings={() => setIsSettingsOpen(true)}
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
                            <VaultView onSelectMeeting={onSelectMeeting} />
                        </motion.div>
                    )}
                </AnimatePresence>
            </main>

            <AgencySettingsModal
                isOpen={isSettingsOpen}
                onClose={() => setIsSettingsOpen(false)}
            />
        </div>
    );
};
