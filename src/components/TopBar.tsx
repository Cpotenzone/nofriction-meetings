// The top bar: Record · Recordings · Chat, and on the right the one
// Record/Stop control (Stop only while recording: the idle Record screen
// has the Record button) and the Settings gear. Nothing else.

import { GearIcon } from "./icons";
import type { AppMode } from "./Shell";

interface TopBarProps {
    activeMode: AppMode;
    onModeChange: (mode: AppMode) => void;
    isRecording: boolean;
    onStop: () => void;
    onOpenSettings: () => void;
}

const MODES: { mode: AppMode; label: string; shortcut: string }[] = [
    { mode: "record", label: "Record", shortcut: "⌘1" },
    { mode: "recordings", label: "Recordings", shortcut: "⌘2" },
    { mode: "chat", label: "Chat", shortcut: "⇧⌘I" },
];

export function TopBar({ activeMode, onModeChange, isRecording, onStop, onOpenSettings }: TopBarProps) {
    return (
        <nav className="topbar" aria-label="Main">
            <div className="topbar__left">
                <img className="topbar__logo" src="/trinacria.svg" alt="" />
                <span className="topbar__name">noFriction</span>
            </div>

            <div className="topbar__modes" role="tablist" aria-label="View">
                {MODES.map(({ mode, label, shortcut }) => (
                    <button
                        key={mode}
                        type="button"
                        role="tab"
                        aria-selected={activeMode === mode}
                        className={`topbar__mode ${activeMode === mode ? "is-on" : ""}`}
                        onClick={() => onModeChange(mode)}
                        title={shortcut}
                    >
                        {mode === "record" && isRecording && <span className="topbar__rec-dot" aria-hidden />}
                        {label}
                    </button>
                ))}
            </div>

            <div className="topbar__right">
                {isRecording && (
                    <button type="button" className="topbar__stop" onClick={onStop} title="Stop recording (⌘.)">
                        <span className="topbar__stop-square" aria-hidden />
                        Stop
                    </button>
                )}
                <button type="button" className="topbar__gear" onClick={onOpenSettings} title="Settings (⌘,)" aria-label="Settings">
                    <GearIcon size={17} />
                </button>
            </div>
        </nav>
    );
}
