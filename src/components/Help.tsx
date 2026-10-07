import { useState } from 'react';
import { useCapabilities } from '../lib/build';

export function HelpSection() {
    const [activeSection, setActiveSection] = useState<'start' | 'shortcuts' | 'troubleshoot'>('start');
    // Video Diagnostics lives in the Admin Console, which the App Store build hides
    const caps = useCapabilities();
    const hasAdmin = caps?.owner_infra ?? false;

    return (
        <div style={{ padding: '24px', maxWidth: '800px', margin: '0 auto', color: '#e5e7eb' }}>
            <h2 style={{ fontSize: '24px', fontWeight: 600, marginBottom: '24px' }}>Help & Documentation</h2>

            <div style={{ display: 'flex', gap: '16px', marginBottom: '32px', borderBottom: '1px solid rgba(255,255,255,0.1)' }}>
                {[
                    { id: 'start', label: 'Getting Started' },
                    { id: 'shortcuts', label: 'Shortcuts' },
                    { id: 'troubleshoot', label: 'Troubleshooting' }
                ].map(tab => (
                    <button
                        key={tab.id}
                        onClick={() => setActiveSection(tab.id as any)}
                        style={{
                            padding: '12px 16px',
                            background: 'none',
                            border: 'none',
                            borderBottom: activeSection === tab.id ? '2px solid #6366f1' : '2px solid transparent',
                            color: activeSection === tab.id ? '#fff' : '#9ca3af',
                            cursor: 'pointer',
                            fontSize: '14px',
                            fontWeight: 500
                        }}
                    >
                        {tab.label}
                    </button>
                ))}
            </div>

            <div className="help-content">
                {activeSection === 'start' && (
                    <div className="space-y-6">
                        <section>
                            <h3 style={{ fontSize: '18px', fontWeight: 600, color: '#fff', marginBottom: '12px' }}>Welcome to noFriction</h3>
                            <p style={{ lineHeight: '1.6', color: '#d1d5db' }}>
                                noFriction records and transcribes meetings, classes and everyday conversations on this Mac, with Whisper running locally by default. Add your own AI key (or use a local or Apple on-device model) for notes and chat. Your data stays on this Mac; there is no noFriction server. For the full guide, open HELP from the MORE menu.
                            </p>
                        </section>

                        <section style={{ marginTop: '24px' }}>
                            <h4 style={{ fontSize: '16px', fontWeight: 600, color: '#fff', marginBottom: '8px' }}>Core Features</h4>
                            <ul style={{ listStyle: 'none', padding: 0, display: 'grid', gap: '12px' }}>
                                {[
                                    { icon: '🎤', title: 'Live Transcription', desc: 'On-device Whisper transcription after the local model is downloaded.' },
                                    { icon: '⏮️', title: 'Rewind', desc: 'Pick a recording in REWIND to see screenshots and transcript side by side, or switch to its Notes.' },
                                    { icon: '🧠', title: 'AI Notes', desc: caps?.pro_gating
                                        ? 'Summary, key topics, decisions and action items with your AI provider (Settings → AI Engine). Requires noFriction Pro.'
                                        : 'Summary, key topics, decisions and action items with your AI provider (Settings → AI Engine).' },
                                    { icon: '✂️', title: 'Delete & Strike', desc: 'Remove words or screenshots (5-second undo), or strike them from the record with a marker.' },
                                    { icon: '⏹️', title: 'Auto-Stop', desc: 'Stops after a 30-second countdown when the meeting ends. Settings → General → Recording.' },
                                    { icon: '🔍', title: 'Search & Chat', desc: 'Search across all your past recordings, or ask questions in CHAT.' }
                                ].map((item, i) => (
                                    <li key={i} style={{ background: 'rgba(255,255,255,0.05)', padding: '16px', borderRadius: '8px' }}>
                                        <div style={{ display: 'flex', alignItems: 'center', gap: '12px', marginBottom: '4px' }}>
                                            <span>{item.icon}</span>
                                            <strong style={{ color: '#fff' }}>{item.title}</strong>
                                        </div>
                                        <p style={{ fontSize: '14px', color: '#9ca3af', margin: 0 }}>{item.desc}</p>
                                    </li>
                                ))}
                            </ul>
                        </section>
                    </div>
                )}

                {activeSection === 'shortcuts' && (
                    <div>
                        <table style={{ width: '100%', borderCollapse: 'collapse' }}>
                            <thead>
                                <tr style={{ borderBottom: '1px solid rgba(255,255,255,0.1)', textAlign: 'left' }}>
                                    <th style={{ padding: '12px', color: '#9ca3af' }}>Action</th>
                                    <th style={{ padding: '12px', color: '#9ca3af' }}>Shortcut</th>
                                </tr>
                            </thead>
                            <tbody>
                                {[
                                    { action: 'New recording', shortcut: '⌘N' },
                                    { action: 'Stop recording', shortcut: '⌘.' },
                                    { action: 'Live / Recordings', shortcut: '⌘1 / ⌘2' },
                                    { action: 'Chat with your recordings', shortcut: '⇧⌘I' },
                                    { action: 'Prompts', shortcut: '⇧⌘P' },
                                    { action: 'Command palette (search & commands)', shortcut: '⌘K' },
                                    { action: 'Settings', shortcut: '⌘,' },
                                    { action: 'Snap a screenshot (in LIVE)', shortcut: '⇧⌘S' },
                                    { action: 'Select more words in a transcript', shortcut: 'Shift-click or drag' },
                                    { action: 'Select a whole transcript line', shortcut: 'Double-click' },
                                    { action: 'Select several screenshots', shortcut: '⌘-click' },
                                    { action: 'Clear the selection', shortcut: 'Esc' },
                                    { action: 'Show/hide the graph in VAULT', shortcut: '⌘G' }
                                ].map((row, i) => (
                                    <tr key={i} style={{ borderBottom: '1px solid rgba(255,255,255,0.05)' }}>
                                        <td style={{ padding: '12px', color: '#d1d5db' }}>{row.action}</td>
                                        <td style={{ padding: '12px' }}>
                                            <code style={{ background: 'rgba(255,255,255,0.1)', padding: '4px 8px', borderRadius: '4px', fontSize: '12px' }}>{row.shortcut}</code>
                                        </td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                )}

                {activeSection === 'troubleshoot' && (
                    <div className="space-y-6">
                        <section>
                            <h3 style={{ fontSize: '18px', fontWeight: 600, color: '#fff', marginBottom: '16px' }}>Common Issues</h3>

                            <div style={{ marginBottom: '24px' }}>
                                <h4 style={{ color: '#fca5a5', fontWeight: 600, marginBottom: '8px', display: 'flex', alignItems: 'center', gap: '8px' }}>
                                    <span>⚠️</span> Screen Capture Issues
                                </h4>
                                <p style={{ fontSize: '14px', color: '#d1d5db', marginBottom: '12px', lineHeight: '1.6' }}>
                                    If screenshots are missing or show only the app window instead of the full screen, reset the Screen Recording permission.
                                </p>
                                {hasAdmin && (
                                <div style={{ background: 'rgba(139, 92, 246, 0.15)', border: '1px solid rgba(139, 92, 246, 0.3)', padding: '16px', borderRadius: '8px', marginBottom: '12px' }}>
                                    <p style={{ fontSize: '13px', color: 'var(--accent-primary-hover, #818cf8)', margin: '0 0 12px 0', fontWeight: 600 }}>
                                        📹 Quick Fix: Use Video Diagnostics
                                    </p>
                                    <ol style={{ fontSize: '13px', color: 'var(--text-secondary, #b0b5c9)', marginLeft: '20px', lineHeight: '1.8' }}>
                                        <li>Open Settings (gear icon) → Admin Console → Video Diagnostics</li>
                                        <li>Click "Test Capture Now" to verify your capture</li>
                                        <li>If dimensions don't match, follow the on-screen reset instructions</li>
                                    </ol>
                                </div>
                                )}
                                <details style={{ fontSize: '13px', color: '#9ca3af', marginTop: '12px' }}>
                                    <summary style={{ cursor: 'pointer', fontWeight: 600, color: 'var(--accent-primary-hover, #818cf8)', marginBottom: '8px' }}>Fix Steps</summary>
                                    <ol style={{ marginLeft: '20px', marginTop: '8px', lineHeight: '1.8', color: '#d1d5db' }}>
                                        <li>Open System Settings → Privacy & Security → Screen Recording</li>
                                        <li>Remove noFriction from the list</li>
                                        <li>Quit and reopen noFriction, then allow Screen Recording when asked</li>
                                    </ol>
                                </details>
                            </div>

                            <div style={{ marginBottom: '24px' }}>
                                <h4 style={{ color: '#fca5a5', fontWeight: 600, marginBottom: '8px' }}>No Audio Recording</h4>
                                <p style={{ fontSize: '14px', color: '#d1d5db', marginBottom: '8px', lineHeight: '1.6' }}>
                                    Make sure Microphone is allowed in System Settings → Privacy & Security → Microphone, and check the microphone chosen in Settings → General. To hear the other people on a call, turn on Capture System Audio (Settings → General); it needs Screen Recording permission.
                                </p>
                            </div>

                            <div style={{ marginBottom: '24px' }}>
                                <h4 style={{ color: '#fca5a5', fontWeight: 600, marginBottom: '8px' }}>Screenshots not appearing</h4>
                                <p style={{ fontSize: '14px', color: '#d1d5db', marginBottom: '8px', lineHeight: '1.6' }}>
                                    Check the Screen Recording permission above. In REWIND, select the recording again to reload its screenshots.
                                </p>
                            </div>

                            <div style={{ marginBottom: '24px' }}>
                                <h4 style={{ color: '#fca5a5', fontWeight: 600, marginBottom: '8px' }}>No transcript appears</h4>
                                <p style={{ fontSize: '14px', color: '#d1d5db', marginBottom: '8px', lineHeight: '1.6' }}>
                                    Download a Local Whisper model first in Settings → Transcription. LIVE shows "Transcription stopped" with the reason if local transcription cannot start.
                                </p>
                            </div>

                            <div style={{ marginBottom: '24px' }}>
                                <h4 style={{ color: '#fca5a5', fontWeight: 600, marginBottom: '8px' }}>No AI notes after a recording</h4>
                                <p style={{ fontSize: '14px', color: '#d1d5db', marginBottom: '8px', lineHeight: '1.6' }}>
                                    Notes are written automatically only for recordings longer than 6 minutes, and only once Apple on-device is available or your endpoint and model are configured and allowed in Settings → AI Engine.{caps?.pro_gating ? ' In the App Store version they also need noFriction Pro (Settings → Subscription).' : ''} You can always click Generate notes in the recording's Notes view in REWIND. Automatic notes can be turned off with Write notes after each recording.
                                </p>
                            </div>

                            <div style={{ marginBottom: '24px' }}>
                                <h4 style={{ color: '#fca5a5', fontWeight: 600, marginBottom: '8px' }}>Recording stopped on its own</h4>
                                <p style={{ fontSize: '14px', color: '#d1d5db', marginBottom: '8px', lineHeight: '1.6' }}>
                                    Auto-stop ended the recording when it seemed over (mic released, window closed, calendar event ended, or silence). Click Keep recording during the 30-second countdown, raise the silence time, or turn it off in Settings → General → Recording or the menu-bar icon.
                                </p>
                            </div>

                            <div style={{ background: 'linear-gradient(135deg, rgba(59, 130, 246, 0.15), rgba(99, 102, 241, 0.15))', border: '1px solid rgba(99, 102, 241, 0.3)', padding: '20px', borderRadius: '12px' }}>
                                <p style={{ fontSize: '16px', color: '#93c5fd', margin: 0, display: 'flex', alignItems: 'center', gap: '10px' }}>
                                    <span style={{ fontSize: '20px' }}>💡</span>
                                    <span><strong>Need more help?</strong> Email casey@nofriction.io or use Help → Contact Support…. Version, Privacy Policy and Terms of Use are in Settings → About.</span>
                                </p>
                            </div>
                        </section>
                    </div>
                )
                }
            </div >
        </div >
    );
}
