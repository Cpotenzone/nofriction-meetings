import React, { useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { FullSettings } from '../../features/settings/FullSettings';
import { AdminConsole } from '../AdminConsole';
import { HelpSection } from '../Help';

interface AgencySettingsModalProps {
    isOpen: boolean;
    onClose: () => void;
    initialTab?: 'settings' | 'admin' | 'help';
}

type ModalTab = 'settings' | 'admin' | 'help';

export const AgencySettingsModal: React.FC<AgencySettingsModalProps> = ({
    isOpen,
    onClose,
    initialTab = 'settings'
}) => {
    const [activeTab, setActiveTab] = useState<ModalTab>(initialTab);

    // Sync internal state if initialTab changes when opening? 
    // For now, simple state is fine.

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
                            <button
                                className={`modal-nav-btn ${activeTab === 'admin' ? 'active' : ''}`}
                                onClick={() => setActiveTab('admin')}
                            >
                                <span className="icon">🛡️</span> Admin Console
                            </button>
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
                            {activeTab === 'settings' && <FullSettings />}
                            {activeTab === 'admin' && <AdminConsole />}
                            {activeTab === 'help' && <HelpSection />}
                        </div>
                    </motion.div>
                </>
            )}
        </AnimatePresence>
    );
};
