import React, { useEffect, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { FullSettings } from '../../features/settings/FullSettings';
import { AdminConsole } from '../AdminConsole';
import { HelpSection } from '../Help';
import { useCapabilities } from '../../lib/build';
import type { SettingsCategory } from '../../lib/navigation';

interface AgencySettingsModalProps {
    isOpen: boolean;
    onClose: () => void;
    initialTab?: 'settings' | 'admin' | 'help';
    /** Settings category to show when opened */
    initialCategory?: SettingsCategory;
}

type ModalTab = 'settings' | 'admin' | 'help';

export const AgencySettingsModal: React.FC<AgencySettingsModalProps> = ({
    isOpen,
    onClose,
    initialTab = 'settings',
    initialCategory = 'general',
}) => {
    const [activeTab, setActiveTab] = useState<ModalTab>(initialTab);
    // Opening for a category (e.g. "Open AI Engine") always lands on Settings
    useEffect(() => {
        if (isOpen) setActiveTab('settings');
    }, [isOpen, initialCategory]);
    // m14: the admin console (dev tools, video diagnostics) is
    // owner infrastructure; hidden in the Mac App Store build
    const caps = useCapabilities();
    const showAdmin = caps?.owner_infra ?? false;

    return (
        <AnimatePresence>
            {isOpen && (
                <>
                    <motion.div
                        className="agency-modal-backdrop"
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        onClick={onClose}
                    />
                    <motion.div
                        className="agency-modal-content"
                        initial={{ opacity: 0, scale: 0.95, y: 20 }}
                        animate={{ opacity: 1, scale: 1, y: 0 }}
                        exit={{ opacity: 0, scale: 0.95, y: 20 }}
                    >
                        <div className="agency-modal-sidebar">
                            <div className="modal-title">SYSTEM</div>
                            <button
                                className={`modal-nav-btn ${activeTab === 'settings' ? 'active' : ''}`}
                                onClick={() => setActiveTab('settings')}
                            >
                                <span className="icon">⚙️</span> Settings
                            </button>
                            {showAdmin && <button
                                className={`modal-nav-btn ${activeTab === 'admin' ? 'active' : ''}`}
                                onClick={() => setActiveTab('admin')}
                            >
                                <span className="icon">🛡️</span> Admin Console
                            </button>}
                            <button
                                className={`modal-nav-btn ${activeTab === 'help' ? 'active' : ''}`}
                                onClick={() => setActiveTab('help')}
                            >
                                <span className="icon">❓</span> Help & Docs
                            </button>

                            <div className="modal-spacer" />

                            <button className="modal-close-btn" onClick={onClose}>
                                Close Overlay
                            </button>
                        </div>

                        <div className="agency-modal-body">
                            {activeTab === 'settings' && <FullSettings key={initialCategory} initialCategory={initialCategory} />}
                            {activeTab === 'admin' && showAdmin && <AdminConsole />}
                            {activeTab === 'help' && <HelpSection />}
                        </div>
                    </motion.div>
                </>
            )}
        </AnimatePresence>
    );
};
