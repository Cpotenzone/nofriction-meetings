// Settings: one window, six sections (Recording, Transcription, AI, Sync,
// Subscription, About). Opened from the gear, ⌘, and the menu bar.

import { useEffect, useState } from "react";
import { Settings } from "../features/settings/Settings";
import type { SettingsCategory } from "../lib/navigation";
import "./SettingsWindow.css";

interface SettingsWindowProps {
    isOpen: boolean;
    onClose: () => void;
    /** Section to open on (e.g. "ai" from a "Set up AI" notice) */
    initialCategory?: SettingsCategory;
}

export function SettingsWindow({ isOpen, onClose, initialCategory = "recording" }: SettingsWindowProps) {
    const [opened, setOpened] = useState(0);
    useEffect(() => {
        if (isOpen) setOpened((n) => n + 1);
    }, [isOpen, initialCategory]);

    useEffect(() => {
        if (!isOpen) return;
        const onKey = (e: KeyboardEvent) => {
            if (e.key === "Escape") onClose();
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [isOpen, onClose]);

    if (!isOpen) return null;
    return (
        <div className="win__scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
            <div className="win win--settings" role="dialog" aria-modal="true" aria-label="Settings">
                <button type="button" className="win__close" onClick={onClose} aria-label="Close" title="Close (Esc)">
                    ✕
                </button>
                <Settings key={opened} initialCategory={initialCategory} />
            </div>
        </div>
    );
}
