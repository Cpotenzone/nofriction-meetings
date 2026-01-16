// Onboarding Setup Wizard
// Collects required API keys and configures the app on first run

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface SetupWizardProps {
    onComplete: () => void;
}

interface SetupState {
    deepgramApiKey: string;
    captureVideo: boolean;
    captureMicrophone: boolean;
    captureSystemAudio: boolean;
}

export function SetupWizard({ onComplete }: SetupWizardProps) {
    const [step, setStep] = useState(1);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [state, setState] = useState<SetupState>({
        deepgramApiKey: '',
        captureVideo: true,  // ON by default - video recording is efficient
        captureMicrophone: true,
        captureSystemAudio: true,
    });

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
            // Save Deepgram API key
            if (state.deepgramApiKey.trim()) {
                await invoke('set_deepgram_api_key', {
                    apiKey: state.deepgramApiKey.trim(),
                });
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

    const totalSteps = 3;

    return (
        <div className="setup-wizard">
            <div className="setup-header">
                <h1>Welcome to noFriction Meetings</h1>
                <p className="setup-subtitle">Let's get you set up in 2 minutes</p>
                <div className="setup-progress">
                    {[1, 2, 3].map((s) => (
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
                        <div className="step-icon">🎙️</div>
                        <h2>Real-Time Transcription</h2>
                        <p className="step-description">
                            noFriction uses Deepgram for live speech-to-text. You'll need a free API key.
                        </p>

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
                        </div>

                        {!state.deepgramApiKey.trim() && (
                            <div className="warning-box">
                                ⚠️ Without an API key, transcription won't work. You can add it later in Settings.
                            </div>
                        )}
                    </div>
                )}

                {/* Step 2: Capture Settings */}
                {step === 2 && (
                    <div className="setup-step">
                        <div className="step-icon">⚙️</div>
                        <h2>Capture Settings</h2>
                        <p className="step-description">
                            Configure what noFriction captures during your meetings.
                        </p>

                        <div className="capture-options">
                            <label className="capture-option">
                                <input
                                    type="checkbox"
                                    checked={state.captureMicrophone}
                                    onChange={(e) => setState({ ...state, captureMicrophone: e.target.checked })}
                                />
                                <div className="option-content">
                                    <span className="option-icon">🎤</span>
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
                                    <span className="option-icon">🔊</span>
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
                                    <span className="option-icon">🎬</span>
                                    <span className="option-label">Video Recording</span>
                                    <span className="option-hint">Continuous screen capture as video (efficient)</span>
                                </div>
                            </label>
                        </div>

                        <div className="performance-note">
                            💡 <strong>New:</strong> Video recording captures every frame efficiently as video, not individual images. Long meetings are now stable!
                        </div>
                    </div>
                )}

                {/* Step 3: Ready */}
                {step === 3 && (
                    <div className="setup-step">
                        <div className="step-icon">🚀</div>
                        <h2>You're All Set!</h2>
                        <p className="step-description">
                            Here's a summary of your setup:
                        </p>

                        <div className="setup-summary">
                            <div className="summary-item">
                                <span className="summary-label">Deepgram API</span>
                                <span className={`summary-value ${state.deepgramApiKey.trim() ? 'success' : 'warning'}`}>
                                    {state.deepgramApiKey.trim() ? '✅ Configured' : '⚠️ Not configured'}
                                </span>
                            </div>
                            <div className="summary-item">
                                <span className="summary-label">Microphone</span>
                                <span className="summary-value">{state.captureMicrophone ? '✅ On' : '❌ Off'}</span>
                            </div>
                            <div className="summary-item">
                                <span className="summary-label">System Audio</span>
                                <span className="summary-value">{state.captureSystemAudio ? '✅ On' : '❌ Off'}</span>
                            </div>
                            <div className="summary-item">
                                <span className="summary-label">Video Recording</span>
                                <span className="summary-value">
                                    {state.captureVideo ? '✅ On (efficient video)' : '❌ Off'}
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
                        {isLoading ? 'Saving...' : 'Start Using noFriction'}
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
