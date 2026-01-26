
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

export function SystemStatus({ onComplete, onRetry }: { onComplete?: () => void, onRetry: () => void }) {
    const [steps, setSteps] = useState<{ msg: string, time: string }[]>([]);
    const [status, setStatus] = useState<"initializing" | "ready" | "failed">("initializing");
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        const unlisten = Promise.all([
            listen<string>("init-step", (e) => {
                setSteps(prev => [...prev, { msg: e.payload, time: new Date().toLocaleTimeString() }]);
            }),
            listen<string>("init-error", (e) => {
                setError(e.payload);
                setStatus("failed");
            }),
            listen("app-ready", () => {
                setStatus("ready");
                setTimeout(() => onComplete?.(), 500); // Brief pause to show green
            })
        ]);

        return () => {
            unlisten.then(listeners => listeners.forEach(u => u()));
        };
    }, []);

    // Initial check in case we missed events
    useEffect(() => {
        invoke<{ "Ready": null } | { "Failed": string } | "Initializing">("check_init_status")
            .then(s => {
                if ((s as string) === 'Ready') {
                    // Already ready
                    setStatus("ready");
                    onComplete?.();
                } else if (typeof s === 'object' && 'Ready' in s) {
                    setStatus("ready");
                    onComplete?.();
                } else if (typeof s === 'object' && 'Failed' in s) {
                    // @ts-ignore
                    setError(s.Failed);
                    setStatus("failed");
                }
            });
    }, []);

    return (
        <div style={{
            display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
            height: '100vh', width: '100vw', background: '#0f1115', color: '#e5e7eb', fontFamily: 'Inter, system-ui'
        }}>
            <div style={{ width: '450px', padding: '32px', background: '#1a1d29', borderRadius: '12px', boxShadow: '0 20px 50px rgba(0,0,0,0.5)' }}>
                <div style={{ display: 'flex', alignItems: 'center', marginBottom: '24px' }}>
                    {status === 'initializing' && <div className="loading-spinner" style={{ width: 20, height: 20, marginRight: 12 }} />}
                    {status === 'failed' && <div style={{ fontSize: 24, marginRight: 12 }}>❌</div>}
                    {status === 'ready' && <div style={{ fontSize: 24, marginRight: 12 }}>✅</div>}

                    <h2 style={{ margin: 0, fontSize: 18, fontWeight: 600 }}>
                        {status === 'initializing' ? 'System Initialization' : status === 'failed' ? 'Startup Failed' : 'System Ready'}
                    </h2>
                </div>

                <div style={{
                    background: '#0a0c10', padding: '16px', borderRadius: '8px',
                    height: '200px', overflowY: 'auto', fontFamily: 'monospace', fontSize: '13px',
                    display: 'flex', flexDirection: 'column', gap: '8px', border: '1px solid #333'
                }}>
                    {steps.length === 0 && <span style={{ color: '#6b7280' }}>Waiting for system events...</span>}
                    {steps.map((s, i) => (
                        <div key={i} style={{ display: 'flex', gap: '12px' }}>
                            <span style={{ color: '#6b7280' }}>[{s.time}]</span>
                            <span style={{ color: '#10b981' }}>✓</span>
                            <span>{s.msg}</span>
                        </div>
                    ))}
                    {error && (
                        <div style={{ display: 'flex', gap: '12px', color: '#ef4444', fontWeight: 'bold' }}>
                            <span style={{ color: '#ef4444' }}>!</span>
                            <span>ERROR: {error}</span>
                        </div>
                    )}
                </div>

                {status === 'failed' && (
                    <button
                        onClick={onRetry}
                        style={{
                            marginTop: '24px', width: '100%', padding: '10px', background: '#4f46e5',
                            color: 'white', border: 'none', borderRadius: '6px', fontWeight: 600, cursor: 'pointer'
                        }}
                    >
                        Retry Startup
                    </button>
                )}
            </div>

            <div style={{ position: 'fixed', bottom: 20, color: '#4b5563', fontSize: 12 }}>
                noFriction Meetings v1.0.0-rc.24
            </div>
        </div>
    );
}
