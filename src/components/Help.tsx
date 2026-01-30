import { useState } from 'react';

export function HelpSection() {
    const [activeSection, setActiveSection] = useState<'start' | 'shortcuts' | 'troubleshoot'>('start');

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
                            borderBottom: activeSection === tab.id ? '2px solid #7c3aed' : '2px solid transparent',
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
                            <h3 style={{ fontSize: '18px', fontWeight: 600, color: '#fff', marginBottom: '12px' }}>Welcome to noFriction Meetings</h3>
                            <p style={{ lineHeight: '1.6', color: '#d1d5db' }}>
                                noFriction Meetings is your AI-powered companion for smarter meetings. It records, transcribes, and analyzes your meetings in real-time, providing you with searchable transcripts, visual timelines, and deep insights.
                            </p>
                        </section>

                        <section style={{ marginTop: '24px' }}>
                            <h4 style={{ fontSize: '16px', fontWeight: 600, color: '#fff', marginBottom: '8px' }}>Core Features</h4>
                            <ul style={{ listStyle: 'none', padding: 0, display: 'grid', gap: '12px' }}>
                                {[
                                    { icon: '🎤', title: 'Live Transcription', desc: 'Real-time speech-to-text for all your meetings.' },
                                    { icon: '⏮️', title: 'Rewind', desc: 'Visual playback with synchronized screenshots and audio.' },
                                    { icon: '🧠', title: 'Deep Intel', desc: 'AI-generated summaries, action items, and insights.' },
                                    { icon: '🔍', title: 'Knowledge Base', desc: 'Search across all your past meetings instanly.' }
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
                                    { action: 'Wait for next release', shortcut: 'Coming Soon' },
                                    { action: 'Currently keyboard shortcuts are in beta', shortcut: '-' }
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
                                    If screen recordings show only the app window instead of the full screen, you need to reset macOS permissions.
                                </p>
                                <div style={{ background: 'rgba(139, 92, 246, 0.15)', border: '1px solid rgba(139, 92, 246, 0.3)', padding: '16px', borderRadius: '8px', marginBottom: '12px' }}>
                                    <p style={{ fontSize: '13px', color: '#c4b5fd', margin: '0 0 12px 0', fontWeight: 600 }}>
                                        📹 Quick Fix: Use Video Diagnostics
                                    </p>
                                    <ol style={{ fontSize: '13px', color: '#e9d5ff', marginLeft: '20px', lineHeight: '1.8' }}>
                                        <li>Go to Admin → Video Diagnostics (in the tab bar)</li>
                                        <li>Click "Test Capture Now" to verify your capture</li>
                                        <li>If dimensions don't match, follow the on-screen reset instructions</li>
                                    </ol>
                                </div>
                                <details style={{ fontSize: '13px', color: '#9ca3af', marginTop: '12px' }}>
                                    <summary style={{ cursor: 'pointer', fontWeight: 600, color: '#a78bfa', marginBottom: '8px' }}>Manual Fix Steps</summary>
                                    <ol style={{ marginLeft: '20px', marginTop: '8px', lineHeight: '1.8', color: '#d1d5db' }}>
                                        <li>Open System Settings → Privacy & Security → Screen Recording</li>
                                        <li>Remove noFriction Meetings from the list</li>
                                        <li>Restart the app to trigger permission prompt</li>
                                    </ol>
                                </details>
                            </div>

                            <div style={{ marginBottom: '24px' }}>
                                <h4 style={{ color: '#fca5a5', fontWeight: 600, marginBottom: '8px' }}>No Audio Recording</h4>
                                <p style={{ fontSize: '14px', color: '#d1d5db', marginBottom: '8px', lineHeight: '1.6' }}>
                                    Ensure you have granted Microphone permissions in System Settings → Privacy & Security → Microphone.
                                </p>
                            </div>

                            <div style={{ marginBottom: '24px' }}>
                                <h4 style={{ color: '#fca5a5', fontWeight: 600, marginBottom: '8px' }}>Screenshots not appearing</h4>
                                <p style={{ fontSize: '14px', color: '#d1d5db', marginBottom: '8px', lineHeight: '1.6' }}>
                                    Check Screen Recording permissions above. If using Rewind, try clicking on a specific meeting to refresh the gallery.
                                </p>
                            </div>

                            <div style={{ background: 'linear-gradient(135deg, rgba(59, 130, 246, 0.15), rgba(99, 102, 241, 0.15))', border: '1px solid rgba(99, 102, 241, 0.3)', padding: '20px', borderRadius: '12px' }}>
                                <p style={{ fontSize: '16px', color: '#93c5fd', margin: 0, display: 'flex', alignItems: 'center', gap: '10px' }}>
                                    <span style={{ fontSize: '20px' }}>💡</span>
                                    <span><strong>Need more help?</strong> Contact support at support@nofriction.ai</span>
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
