// noFriction Meetings - Permissions Diagnostics Component
// Shows macOS permission status with live testing capability

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface PermissionStatus {
    screen_recording: boolean;
    microphone: boolean;
    accessibility: boolean;
}

interface ScreenTestResult {
    success: boolean;
    frame_width?: number;
    frame_height?: number;
    error?: string;
}

interface MicTestResult {
    success: boolean;
    device_name?: string;
    sample_rate?: number;
    channels?: number;
    error?: string;
}

interface AccessibilityTestResult {
    success: boolean;
    is_trusted: boolean;
    app_name?: string;
    text_sample?: string;
    text_length?: number;
    error?: string;
}

export function PermissionsStatus() {
    const [permissions, setPermissions] = useState<PermissionStatus | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [testResults, setTestResults] = useState<{
        screen?: ScreenTestResult;
        mic?: MicTestResult;
        accessibility?: AccessibilityTestResult;
    }>({});
    const [testing, setTesting] = useState<{
        screen: boolean;
        mic: boolean;
        accessibility: boolean;
    }>({ screen: false, mic: false, accessibility: false });

    useEffect(() => {
        checkPermissions();
    }, []);

    const checkPermissions = async () => {
        setIsLoading(true);
        try {
            const status = await invoke<PermissionStatus>("check_permissions");
            setPermissions(status);
        } catch (err) {
            console.error("Failed to check permissions:", err);
            setPermissions({
                screen_recording: false,
                microphone: false,
                accessibility: false,
            });
        } finally {
            setIsLoading(false);
        }
    };

    const testScreen = async () => {
        setTesting(prev => ({ ...prev, screen: true }));
        try {
            const result = await invoke<ScreenTestResult>("test_screen_capture");
            setTestResults(prev => ({ ...prev, screen: result }));
            // Refresh permissions after test
            await checkPermissions();
        } catch (err) {
            setTestResults(prev => ({
                ...prev,
                screen: { success: false, error: String(err) }
            }));
        } finally {
            setTesting(prev => ({ ...prev, screen: false }));
        }
    };

    const testMic = async () => {
        setTesting(prev => ({ ...prev, mic: true }));
        try {
            const result = await invoke<MicTestResult>("test_microphone");
            setTestResults(prev => ({ ...prev, mic: result }));
            await checkPermissions();
        } catch (err) {
            setTestResults(prev => ({
                ...prev,
                mic: { success: false, error: String(err) }
            }));
        } finally {
            setTesting(prev => ({ ...prev, mic: false }));
        }
    };

    const testAccessibility = async () => {
        setTesting(prev => ({ ...prev, accessibility: true }));
        try {
            const result = await invoke<AccessibilityTestResult>("test_accessibility");
            setTestResults(prev => ({ ...prev, accessibility: result }));
            await checkPermissions();
        } catch (err) {
            setTestResults(prev => ({
                ...prev,
                accessibility: { success: false, is_trusted: false, error: String(err) }
            }));
        } finally {
            setTesting(prev => ({ ...prev, accessibility: false }));
        }
    };

    const requestPermission = async (type: "screen_recording" | "microphone" | "accessibility") => {
        try {
            await invoke("request_permission", { permissionType: type });
            await checkPermissions();
        } catch (err) {
            console.error(`Failed to request ${type} permission:`, err);
        }
    };

    const openSystemSettings = async (pane: string) => {
        try {
            await invoke("open_system_settings", { pane });
        } catch (_err) {
            const url = `x-apple.systempreferences:com.apple.preference.security?Privacy_${pane}`;
            window.open(url, "_blank");
        }
    };

    const StatusIcon = ({ granted }: { granted: boolean }) => (
        <span style={{
            color: granted ? "var(--success, #10b981)" : "var(--error, #ef4444)",
            marginRight: "8px",
            fontSize: "1.1em"
        }}>
            {granted ? "✓" : "✗"}
        </span>
    );

    if (isLoading) {
        return (
            <section className="settings-section">
                <h3>
                    <span className="icon">🔐</span>
                    Permissions Diagnostics
                </h3>
                <div style={{ padding: "var(--spacing-md)", color: "var(--text-secondary)" }}>
                    Checking permissions...
                </div>
            </section>
        );
    }

    return (
        <section className="settings-section">
            <h3>
                <span className="icon">🔐</span>
                Permissions Diagnostics
            </h3>
            <p style={{ fontSize: "0.75rem", color: "var(--text-tertiary)", marginBottom: "var(--spacing-md)" }}>
                Test and verify each permission is working correctly. Click "Test" to actively check functionality.
            </p>

            {/* Screen Recording */}
            <div className="settings-row" style={{ flexDirection: "column", alignItems: "stretch", gap: "8px" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                    <div className="settings-label">
                        <span className="label-main">
                            <StatusIcon granted={permissions?.screen_recording ?? false} />
                            Screen Recording
                        </span>
                        <span className="label-sub">Required for capturing screen frames</span>
                    </div>
                    <div style={{ display: "flex", gap: "8px" }}>
                        <button
                            className="btn btn-secondary"
                            onClick={testScreen}
                            disabled={testing.screen}
                            style={{ fontSize: "0.75rem", minWidth: "60px" }}
                        >
                            {testing.screen ? "..." : "Test"}
                        </button>
                        <button
                            className="btn btn-secondary"
                            onClick={() => openSystemSettings("ScreenCapture")}
                            style={{ fontSize: "0.75rem" }}
                        >
                            Settings
                        </button>
                    </div>
                </div>
                {testResults.screen && (
                    <div style={{
                        padding: "8px 12px",
                        borderRadius: "6px",
                        backgroundColor: testResults.screen.success
                            ? "rgba(16, 185, 129, 0.1)"
                            : "rgba(239, 68, 68, 0.1)",
                        fontSize: "0.75rem",
                        color: testResults.screen.success ? "#10b981" : "#ef4444"
                    }}>
                        {testResults.screen.success ? (
                            <>✓ Captured frame: {testResults.screen.frame_width}x{testResults.screen.frame_height}px</>
                        ) : (
                            <>✗ {testResults.screen.error || "Failed to capture"}</>
                        )}
                    </div>
                )}
            </div>

            {/* Microphone */}
            <div className="settings-row" style={{ flexDirection: "column", alignItems: "stretch", gap: "8px" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                    <div className="settings-label">
                        <span className="label-main">
                            <StatusIcon granted={permissions?.microphone ?? false} />
                            Microphone
                        </span>
                        <span className="label-sub">Required for audio transcription</span>
                    </div>
                    <div style={{ display: "flex", gap: "8px" }}>
                        <button
                            className="btn btn-secondary"
                            onClick={testMic}
                            disabled={testing.mic}
                            style={{ fontSize: "0.75rem", minWidth: "60px" }}
                        >
                            {testing.mic ? "..." : "Test"}
                        </button>
                        <button
                            className="btn btn-secondary"
                            onClick={() => openSystemSettings("Microphone")}
                            style={{ fontSize: "0.75rem" }}
                        >
                            Settings
                        </button>
                    </div>
                </div>
                {testResults.mic && (
                    <div style={{
                        padding: "8px 12px",
                        borderRadius: "6px",
                        backgroundColor: testResults.mic.success
                            ? "rgba(16, 185, 129, 0.1)"
                            : "rgba(239, 68, 68, 0.1)",
                        fontSize: "0.75rem",
                        color: testResults.mic.success ? "#10b981" : "#ef4444"
                    }}>
                        {testResults.mic.success ? (
                            <>✓ {testResults.mic.device_name} ({testResults.mic.sample_rate}Hz, {testResults.mic.channels}ch)</>
                        ) : (
                            <>✗ {testResults.mic.error || "Failed to access microphone"}</>
                        )}
                    </div>
                )}
            </div>

            {/* Accessibility */}
            <div className="settings-row" style={{ flexDirection: "column", alignItems: "stretch", gap: "8px" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                    <div className="settings-label">
                        <span className="label-main">
                            <StatusIcon granted={permissions?.accessibility ?? false} />
                            Accessibility
                        </span>
                        <span className="label-sub">Required for reading on-screen text</span>
                    </div>
                    <div style={{ display: "flex", gap: "8px" }}>
                        <button
                            className="btn btn-secondary"
                            onClick={testAccessibility}
                            disabled={testing.accessibility}
                            style={{ fontSize: "0.75rem", minWidth: "60px" }}
                        >
                            {testing.accessibility ? "..." : "Test"}
                        </button>
                        <button
                            className="btn btn-secondary"
                            onClick={() => requestPermission("accessibility")}
                            style={{ fontSize: "0.75rem" }}
                        >
                            Request
                        </button>
                        <button
                            className="btn btn-secondary"
                            onClick={() => openSystemSettings("Accessibility")}
                            style={{ fontSize: "0.75rem" }}
                        >
                            Settings
                        </button>
                    </div>
                </div>
                {testResults.accessibility && (
                    <div style={{
                        padding: "8px 12px",
                        borderRadius: "6px",
                        backgroundColor: testResults.accessibility.success
                            ? "rgba(16, 185, 129, 0.1)"
                            : "rgba(239, 68, 68, 0.1)",
                        fontSize: "0.75rem",
                        color: testResults.accessibility.success ? "#10b981" : "#ef4444"
                    }}>
                        {testResults.accessibility.success ? (
                            <div>
                                <p style={{ margin: 0 }}>
                                    ✓ App: {testResults.accessibility.app_name || "Unknown"} ({testResults.accessibility.text_length} chars)
                                </p>
                                {testResults.accessibility.text_sample && (
                                    <p style={{
                                        margin: "4px 0 0 0",
                                        opacity: 0.8,
                                        fontStyle: "italic",
                                        whiteSpace: "pre-wrap",
                                        maxHeight: "60px",
                                        overflow: "hidden"
                                    }}>
                                        "{testResults.accessibility.text_sample}"
                                    </p>
                                )}
                            </div>
                        ) : (
                            <>✗ {testResults.accessibility.error || "Failed to extract text"}</>
                        )}
                    </div>
                )}
            </div>

            <div style={{ marginTop: "var(--spacing-md)", display: "flex", gap: "8px" }}>
                <button className="btn btn-secondary" onClick={checkPermissions}>
                    🔄 Refresh Status
                </button>
                <button
                    className="btn btn-primary"
                    onClick={async () => {
                        await testScreen();
                        await testMic();
                        await testAccessibility();
                    }}
                >
                    🧪 Test All
                </button>
            </div>
        </section>
    );
}
