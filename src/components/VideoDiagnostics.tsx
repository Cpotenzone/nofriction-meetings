import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import styles from './VideoDiagnostics.module.css';

interface MonitorInfo {
    id: number;
    name: string;
    width: number;
    height: number;
    is_primary: boolean;
}

interface CaptureDiagnostics {
    monitors: MonitorInfo[];
    current_monitor_id: number | null;
    frame_interval_ms: number;
    is_recording: boolean;
    screen_permission: boolean;
    mic_permission: boolean;
}

interface TestCaptureResult {
    image_base64: string;
    actual_width: number;
    actual_height: number;
    expected_width: number;
    expected_height: number;
    monitor_name: string;
    dimensions_match: boolean;
}

export const VideoDiagnostics: React.FC = () => {
    const [diagnostics, setDiagnostics] = useState<CaptureDiagnostics | null>(null);
    const [testResult, setTestResult] = useState<TestCaptureResult | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        loadDiagnostics();
    }, []);

    const loadDiagnostics = async () => {
        try {
            const diag = await invoke<CaptureDiagnostics>('get_capture_diagnostics');
            setDiagnostics(diag);
        } catch (err) {
            setError(`Failed to load diagnostics: ${err}`);
            console.error('Diagnostics error:', err);
        }
    };

    const runTestCapture = async () => {
        setLoading(true);
        setError(null);
        try {
            const result = await invoke<TestCaptureResult>('test_live_capture');
            setTestResult(result);
        } catch (err) {
            setError(`Test capture failed: ${err}`);
            console.error('Test capture error:', err);
        } finally {
            setLoading(false);
        }
    };

    const primaryMonitor = diagnostics?.monitors.find(m => m.is_primary) || diagnostics?.monitors[0];

    return (
        <div className={styles.container}>
            <h2 className={styles.title}>Video Diagnostics</h2>

            {/* Monitor Information */}
            <section className={styles.section}>
                <h3 className={styles.sectionTitle}>Monitor Information</h3>
                {primaryMonitor ? (
                    <div className={styles.info}>
                        <div className={styles.infoRow}>
                            <span className={styles.label}>Name:</span>
                            <span className={styles.value}>{primaryMonitor.name}</span>
                        </div>
                        <div className={styles.infoRow}>
                            <span className={styles.label}>Dimensions:</span>
                            <span className={styles.value}>{primaryMonitor.width} × {primaryMonitor.height}</span>
                        </div>
                        <div className={styles.infoRow}>
                            <span className={styles.label}>Status:</span>
                            <span className={`${styles.value} ${styles.statusGood}`}>
                                ✓ {primaryMonitor.is_primary ? 'Primary Display' : 'Secondary Display'}
                            </span>
                        </div>
                    </div>
                ) : (
                    <p className={styles.loading}>Loading monitor info...</p>
                )}
            </section>

            {/* Capture Status */}
            <section className={styles.section}>
                <h3 className={styles.sectionTitle}>Capture Status</h3>
                {diagnostics ? (
                    <div className={styles.info}>
                        <div className={styles.infoRow}>
                            <span className={styles.label}>Screen Recording:</span>
                            <span className={`${styles.value} ${diagnostics.screen_permission ? styles.statusGood : styles.statusBad}`}>
                                {diagnostics.screen_permission ? '✓ Granted' : '⚠️ Not Granted or Limited'}
                            </span>
                        </div>
                        <div className={styles.infoRow}>
                            <span className={styles.label}>Microphone:</span>
                            <span className={`${styles.value} ${diagnostics.mic_permission ? styles.statusGood : styles.statusBad}`}>
                                {diagnostics.mic_permission ? '✓ Granted' : '⚠️ Not Granted'}
                            </span>
                        </div>
                        <div className={styles.infoRow}>
                            <span className={styles.label}>Recording:</span>
                            <span className={styles.value}>{diagnostics.is_recording ? 'Yes' : 'No'}</span>
                        </div>
                        <div className={styles.infoRow}>
                            <span className={styles.label}>Capture Interval:</span>
                            <span className={styles.value}>{diagnostics.frame_interval_ms}ms ({(1000 / diagnostics.frame_interval_ms).toFixed(1)} FPS)</span>
                        </div>
                    </div>
                ) : (
                    <p className={styles.loading}>Loading status...</p>
                )}
            </section>

            {/* Test Capture Button */}
            <section className={styles.section}>
                <button
                    className={styles.testButton}
                    onClick={runTestCapture}
                    disabled={loading}
                >
                    {loading ? 'Capturing...' : 'Test Capture Now'}
                </button>
            </section>

            {/* Error Display */}
            {error && (
                <div className={styles.error}>
                    <strong>Error:</strong> {error}
                </div>
            )}

            {/* Test Results */}
            {testResult && (
                <section className={styles.section}>
                    <h3 className={styles.sectionTitle}>Test Capture Result</h3>

                    {/* Preview Image */}
                    <div className={styles.preview}>
                        <img
                            src={`data:image/jpeg;base64,${testResult.image_base64}`}
                            alt="Test capture preview"
                            className={styles.previewImage}
                        />
                    </div>

                    {/* Dimension Analysis */}
                    <div className={styles.analysis}>
                        <div className={styles.infoRow}>
                            <span className={styles.label}>Monitor:</span>
                            <span className={styles.value}>{testResult.monitor_name}</span>
                        </div>
                        <div className={styles.infoRow}>
                            <span className={styles.label}>Expected Dimensions:</span>
                            <span className={styles.value}>{testResult.expected_width} × {testResult.expected_height}</span>
                        </div>
                        <div className={styles.infoRow}>
                            <span className={styles.label}>Captured Dimensions:</span>
                            <span className={styles.value}>{testResult.actual_width} × {testResult.actual_height}</span>
                        </div>
                        <div className={styles.infoRow}>
                            <span className={styles.label}>Status:</span>
                            <span className={`${styles.value} ${testResult.dimensions_match ? styles.statusGood : styles.statusBad}`}>
                                {testResult.dimensions_match ? (
                                    <>✓ Dimensions Match - Full Screen Capture Working</>
                                ) : (
                                    <>❌ MISMATCH DETECTED - Limited Capture</>
                                )}
                            </span>
                        </div>
                    </div>

                    {/* Guidance if mismatch */}
                    {!testResult.dimensions_match && (
                        <div className={styles.guidance}>
                            <strong>⚠️ Screen Recording Permission Issue</strong>
                            <p>
                                The captured dimensions are smaller than expected. This typically means macOS is limiting screen recording to the app window only.
                            </p>
                            <ol>
                                <li>Open <strong>System Settings → Privacy & Security → Screen Recording</strong></li>
                                <li>Find "noFriction Meetings" and toggle it OFF, then ON</li>
                                <li>Quit and relaunch the app completely</li>
                                <li>Run this test again to verify</li>
                            </ol>
                        </div>
                    )}
                </section>
            )}
        </div>
    );
};
