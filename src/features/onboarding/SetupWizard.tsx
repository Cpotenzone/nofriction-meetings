// Onboarding Setup Wizard
// Collects required API keys and configures the app on first run

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { MicIcon, SpeakerIcon, FilmIcon, GearIcon, WarningIcon, SparkleIcon } from '../../components/icons';
import './SetupWizard.css';

interface SetupWizardProps {
    onComplete: () => void;
}

interface SetupState {
    transcriptionMode: 'local' | 'cloud';
    deepgramApiKey: string;
    captureVideo: boolean;
    captureMicrophone: boolean;
    captureSystemAudio: boolean;
}

interface PermissionStatus {
    screen_recording: boolean;
    microphone: boolean;
    accessibility: boolean;
    calendar: boolean;
}

export function SetupWizard({ onComplete }: SetupWizardProps) {
    const [step, setStep] = useState(1);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [state, setState] = useState<SetupState>({
        transcriptionMode: 'local', // Offline by default — nothing leaves the machine
        deepgramApiKey: '',
        captureVideo: true,  // ON by default - video recording is efficient
        captureMicrophone: true,
        captureSystemAudio: true,
    });

    // Permissions (step 3): live status, polled while the user answers
    // the macOS prompts so the chips flip green without any manual refresh
    const [perms, setPerms] = useState<PermissionStatus | null>(null);
    const [micStatus, setMicStatus] = useState<string>('not_determined');
    const [requesting, setRequesting] = useState<string | null>(null);

    const refreshPerms = async () => {
        try {
            const [p, m] = await Promise.all([
                invoke<PermissionStatus>('check_permissions'),
                invoke<string>('get_microphone_auth_status'),
            ]);
            setPerms(p);
            setMicStatus(m);
        } catch (err) {
            console.warn('Permission check unavailable:', err);
        }
    };

    useEffect(() => {
        if (step !== 3) return;
        refreshPerms();
        const interval = setInterval(refreshPerms, 1500);
        return () => clearInterval(interval);
    }, [step]);

    const handleGrant = async (permission: string) => {
        setRequesting(permission);
        try {
            await invoke('request_permission', { permissionType: permission });
        } catch (err) {
            console.warn(`Permission request failed:`, err);
        } finally {
            setRequesting(null);
            refreshPerms();
        }
    };

    const openSettings = (pane: string) => {
        invoke('open_system_settings', { pane }).catch(console.warn);
    };

    const handleNext = () => {
        setStep(step + 1);
        setError(null);
    };

    const handleBack = () => {
        setStep(step - 1);
        setError(null);
    };

    const handleFinish = async () => {
        setIsLoading(true);
        setError(null);

        try {
            if (state.transcriptionMode === 'local') {
                // Offline mode: on-device Whisper, no API key.
                await invoke('set_active_provider', { provider: 'local' });
                // Kick off the one-time model download in the background;
                // progress is visible in Settings → Transcription.
                invoke('download_whisper_model', { model: 'large-v3-turbo-q5_0' }).catch((e) =>
                    console.warn('Whisper model download deferred:', e),
                );
            } else if (state.deepgramApiKey.trim()) {
                await invoke('set_deepgram_api_key', {
                    apiKey: state.deepgramApiKey.trim(),
                });
                await invoke('set_active_provider', { provider: 'deepgram' });
            }

            // Save capture settings
            await invoke('set_capture_microphone', { enabled: state.captureMicrophone });
            await invoke('set_capture_system_audio', { enabled: state.captureSystemAudio });
            await invoke('set_capture_screen', { enabled: state.captureVideo });

            // Setup complete - store in localStorage as fallback
            localStorage.setItem('nofriction_setup_complete', 'true');

            onComplete();
        } catch (err) {
            setError(`Setup failed: ${err}`);
        } finally {
            setIsLoading(false);
        }
    };

    const totalSteps = 4;

    return (
        <div className="setup-wizard">
            <div className="setup-header">
                <h1>Welcome to noFriction Meetings</h1>
                <p className="setup-subtitle">Let's get you set up in 2 minutes</p>
                <div className="setup-progress">
                    {[1, 2, 3, 4].map((s) => (
                        <div
                            key={s}
                            className={`progress-dot ${s === step ? 'active' : ''} ${s < step ? 'complete' : ''}`}
                        />
                    ))}
                </div>
            </div>

            <div className="setup-content">
                {/* Step 1: Deepgram API Key */}
                {step === 1 && (
                    <div className="setup-step">
                        <div className="step-icon"><MicIcon size={22} /></div>
                        <h2>Real-Time Transcription</h2>
                        <p className="step-description">
                            Choose how speech gets turned into text. You can switch providers
                            any time in Settings → Transcription.
                        </p>

                        <div className="mode-choice">
                            <label className={`mode-card ${state.transcriptionMode === 'local' ? 'selected' : ''}`}>
                                <input
                                    type="radio"
                                    name="stt-mode"
                                    checked={state.transcriptionMode === 'local'}
                                    onChange={() => setState({ ...state, transcriptionMode: 'local' })}
                                />
                                <div>
                                    <span className="mode-title">Private &amp; Offline</span>
                                    <span className="mode-hint">
                                        Whisper runs on this Mac. No API key, no internet, nothing
                                        leaves your machine. Downloads a 547 MB model once.
                                    </span>
                                </div>
                            </label>
                            <label className={`mode-card ${state.transcriptionMode === 'cloud' ? 'selected' : ''}`}>
                                <input
                                    type="radio"
                                    name="stt-mode"
                                    checked={state.transcriptionMode === 'cloud'}
                                    onChange={() => setState({ ...state, transcriptionMode: 'cloud' })}
                                />
                                <div>
                                    <span className="mode-title">Cloud (Deepgram)</span>
                                    <span className="mode-hint">
                                        Highest accuracy with speaker labels. Needs a free API key;
                                        audio is sent to Deepgram.
                                    </span>
                                </div>
                            </label>
                        </div>

                        {state.transcriptionMode === 'cloud' && (
                            <div className="api-key-section">
                                <label htmlFor="deepgram-key">Deepgram API Key</label>
                                <input
                                    id="deepgram-key"
                                    type="password"
                                    placeholder="Enter your Deepgram API key"
                                    value={state.deepgramApiKey}
                                    onChange={(e) => setState({ ...state, deepgramApiKey: e.target.value })}
                                    className="setup-input"
                                />
                                <a
                                    href="https://console.deepgram.com"
                                    target="_blank"
                                    rel="noopener noreferrer"
                                    className="get-key-link"
                                >
                                    Get a free API key →
                                </a>
                                <p className="key-hint">
                                    Free tier includes $200 credit (~100 hours of transcription)
                                </p>
                                {!state.deepgramApiKey.trim() && (
                                    <div className="warning-box">
                                        <WarningIcon size={16} />
                                        <span>Without an API key, cloud transcription won't work. You can add one later in Settings.</span>
                                    </div>
                                )}
                            </div>
                        )}
                    </div>
                )}

                {/* Step 2: Capture Settings */}
                {step === 2 && (
                    <div className="setup-step">
                        <div className="step-icon"><GearIcon size={22} /></div>
                        <h2>Capture Settings</h2>
                        <p className="step-description">
                            Configure what noFriction Meetings captures during your meetings.
                        </p>

                        <div className="capture-options">
                            <label className="capture-option">
                                <input
                                    type="checkbox"
                                    checked={state.captureMicrophone}
                                    onChange={(e) => setState({ ...state, captureMicrophone: e.target.checked })}
                                />
                                <div className="option-content">
                                    <span className="option-icon"><MicIcon size={18} /></span>
                                    <span className="option-label">Microphone Audio</span>
                                    <span className="option-hint">Your voice (required for transcription)</span>
                                </div>
                            </label>

                            <label className="capture-option">
                                <input
                                    type="checkbox"
                                    checked={state.captureSystemAudio}
                                    onChange={(e) => setState({ ...state, captureSystemAudio: e.target.checked })}
                                />
                                <div className="option-content">
                                    <span className="option-icon"><SpeakerIcon size={18} /></span>
                                    <span className="option-label">System Audio</span>
                                    <span className="option-hint">Capture Zoom/Teams/Meet audio</span>
                                </div>
                            </label>

                            <label className="capture-option">
                                <input
                                    type="checkbox"
                                    checked={state.captureVideo}
                                    onChange={(e) => setState({ ...state, captureVideo: e.target.checked })}
                                />
                                <div className="option-content">
                                    <span className="option-icon"><FilmIcon size={18} /></span>
                                    <span className="option-label">Video Recording</span>
                                    <span className="option-hint">Continuous screen capture as video (efficient)</span>
                                </div>
                            </label>
                        </div>

                        <div className="performance-note">
                            Screen capture takes one frame per second and skips unchanged screens,
                            so even long meetings stay light on disk.
                        </div>
                    </div>
                )}

                {/* Step 3: macOS Permissions */}
                {step === 3 && (
                    <div className="setup-step">
                        <div className="step-icon"><WarningIcon size={22} /></div>
                        <h2>macOS Permissions</h2>
                        <p className="step-description">
                            Recording needs two system permissions. Grant them now so your
                            first capture just works — the chips turn green as macOS confirms.
                        </p>

                        <div className="perm-list">
                            <div className="perm-row">
                                <span className={`perm-dot ${perms?.microphone ? 'ok' : 'pending'}`} />
                                <div className="perm-info">
                                    <span className="perm-name">Microphone</span>
                                    <span className="perm-hint">
                                        {perms?.microphone
                                            ? 'Granted — your voice will be transcribed'
                                            : micStatus === 'denied' || micStatus === 'restricted'
                                                ? 'Denied earlier — enable it in System Settings, then come back'
                                                : 'Needed to transcribe what is said'}
                                    </span>
                                </div>
                                {!perms?.microphone && (
                                    micStatus === 'denied' || micStatus === 'restricted' ? (
                                        <button className="setup-btn secondary perm-btn" onClick={() => openSettings('microphone')}>
                                            Open Settings
                                        </button>
                                    ) : (
                                        <button
                                            className="setup-btn primary perm-btn"
                                            onClick={() => handleGrant('microphone')}
                                            disabled={requesting === 'microphone'}
                                        >
                                            {requesting === 'microphone' ? 'Asking…' : 'Grant'}
                                        </button>
                                    )
                                )}
                            </div>

                            <div className="perm-row">
                                <span className={`perm-dot ${perms?.screen_recording ? 'ok' : 'pending'}`} />
                                <div className="perm-info">
                                    <span className="perm-name">Screen Recording</span>
                                    <span className="perm-hint">
                                        {perms?.screen_recording
                                            ? 'Granted — screenshots and system audio enabled'
                                            : 'Needed for screenshots and Zoom/Meet audio. macOS requires quitting and reopening the app after granting.'}
                                    </span>
                                </div>
                                {!perms?.screen_recording && (
                                    <div style={{ display: 'flex', gap: 8 }}>
                                        <button
                                            className="setup-btn primary perm-btn"
                                            onClick={() => handleGrant('screen_recording')}
                                            disabled={requesting === 'screen_recording'}
                                        >
                                            {requesting === 'screen_recording' ? 'Asking…' : 'Grant'}
                                        </button>
                                        <button className="setup-btn secondary perm-btn" onClick={() => openSettings('screen_recording')}>
                                            Settings
                                        </button>
                                    </div>
                                )}
                            </div>
                        </div>

                        {perms && !perms.microphone && (
                            <div className="warning-box">
                                <WarningIcon size={16} />
                                <span>
                                    Without microphone access the app records your screen but can't
                                    hear anything — you'd get screenshots with no transcript.
                                </span>
                            </div>
                        )}
                    </div>
                )}

                {/* Step 4: Ready */}
                {step === 4 && (
                    <div className="setup-step">
                        <div className="step-icon"><SparkleIcon size={22} /></div>
                        <h2>You're All Set</h2>
                        <p className="step-description">
                            Here's a summary of your setup:
                        </p>

                        <div className="setup-summary">
                            <div className="summary-item">
                                <span className="summary-label">Transcription</span>
                                <span className={`summary-value ${state.transcriptionMode === 'local' || state.deepgramApiKey.trim() ? 'success' : 'warning'}`}>
                                    {state.transcriptionMode === 'local'
                                        ? 'On-device (offline)'
                                        : state.deepgramApiKey.trim() ? 'Deepgram (cloud)' : 'Cloud — key missing'}
                                </span>
                            </div>
                            <div className="summary-item">
                                <span className="summary-label">Microphone</span>
                                <span className={`summary-value ${state.captureMicrophone ? 'success' : 'off'}`}>{state.captureMicrophone ? 'On' : 'Off'}</span>
                            </div>
                            <div className="summary-item">
                                <span className="summary-label">System audio</span>
                                <span className={`summary-value ${state.captureSystemAudio ? 'success' : 'off'}`}>{state.captureSystemAudio ? 'On' : 'Off'}</span>
                            </div>
                            <div className="summary-item">
                                <span className="summary-label">Screen recording</span>
                                <span className={`summary-value ${state.captureVideo ? 'success' : 'off'}`}>
                                    {state.captureVideo ? 'On' : 'Off'}
                                </span>
                            </div>
                        </div>

                        <div className="quick-start">
                            <h3>Quick Start</h3>
                            <ul>
                                <li>Press <kbd>⌘N</kbd> to start a new recording</li>
                                <li>Press <kbd>⌘.</kbd> to stop recording</li>
                                <li>Press <kbd>⌘K</kbd> to open command palette</li>
                                <li>Press <kbd>⌘,</kbd> to change settings anytime</li>
                            </ul>
                        </div>
                    </div>
                )}
            </div>

            {error && <div className="setup-error">{error}</div>}

            <div className="setup-actions">
                {step > 1 && (
                    <button className="setup-btn secondary" onClick={handleBack} disabled={isLoading}>
                        Back
                    </button>
                )}
                <div className="spacer" />
                {step < totalSteps ? (
                    <button className="setup-btn primary" onClick={handleNext}>
                        Continue
                    </button>
                ) : (
                    <button
                        className="setup-btn primary"
                        onClick={handleFinish}
                        disabled={isLoading}
                    >
                        {isLoading ? 'Saving...' : 'Start Using noFriction Meetings'}
                    </button>
                )}
            </div>
        </div>
    );
}

// Hook to check if setup is needed
export function useSetupRequired() {
    const [isRequired, setIsRequired] = useState<boolean | null>(null);

    useEffect(() => {
        const checkSetup = () => {
            // Check localStorage for setup complete flag
            const setupComplete = localStorage.getItem('nofriction_setup_complete');
            setIsRequired(setupComplete !== 'true');
        };
        checkSetup();
    }, []);

    return isRequired;
}

export default SetupWizard;
