// noFriction Meetings - in-app navigation requests
//
// Any component can ask the shell to open Settings at a category (e.g. the
// "Add an AI key" notices open Settings → AI Engine) without prop drilling.
// AgencyLayout listens and opens the Settings overlay.

export type SettingsCategory = "general" | "transcription" | "obsidian" | "ai" | "subscription" | "data" | "about";

const OPEN_SETTINGS_EVENT = "nf:open-settings";

export function openSettings(category: SettingsCategory = "general"): void {
    window.dispatchEvent(new CustomEvent<SettingsCategory>(OPEN_SETTINGS_EVENT, { detail: category }));
}

export function onOpenSettings(fn: (category: SettingsCategory) => void): () => void {
    const handler = (e: Event) => fn((e as CustomEvent<SettingsCategory>).detail ?? "general");
    window.addEventListener(OPEN_SETTINGS_EVENT, handler);
    return () => window.removeEventListener(OPEN_SETTINGS_EVENT, handler);
}

/** Key in localStorage that marks the first-run setup as done. */
export const SETUP_COMPLETE_KEY = "nofriction_setup_complete";

/** Show the setup assistant again (Settings → General). Nothing is reset. */
export function rerunSetupAssistant(): void {
    try {
        localStorage.removeItem(SETUP_COMPLETE_KEY);
    } catch {
        /* storage unavailable: the reload still shows the app */
    }
    window.location.reload();
}
