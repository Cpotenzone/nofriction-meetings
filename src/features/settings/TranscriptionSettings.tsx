import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface WhisperModelInfo { name: string; size_mb: number; description: string; installed: boolean; active: boolean; }
interface LocalSttStatus { ready: boolean; resolved_model: string | null; models: WhisperModelInfo[]; }
interface WhisperDownloadProgress { model: string; downloaded_bytes: number; total_bytes: number | null; done: boolean; error: string | null; }

export function TranscriptionSettings({ onSave }: { onSave?: () => void }) {
    const [localStatus, setLocalStatus] = useState<LocalSttStatus | null>(null);
    const [downloading, setDownloading] = useState<string | null>(null);
    const [downloadPct, setDownloadPct] = useState(0);
    const [status, setStatus] = useState<string | null>(null);
    const loadLocalStatus = async () => {
        try { setLocalStatus(await invoke<LocalSttStatus>("get_local_stt_status")); }
        catch (e) { setStatus(String(e)); }
    };
    useEffect(() => {
        void loadLocalStatus();
        let disposed = false;
        let unlisten: (() => void) | undefined;
        listen<WhisperDownloadProgress>("whisper_download_progress", ({ payload: p }) => {
            if (p.error) { setStatus(p.error); setDownloading(null); }
            else if (p.done) { setDownloading(null); setDownloadPct(0); void loadLocalStatus(); }
            else if (p.total_bytes) setDownloadPct(Math.round(p.downloaded_bytes / p.total_bytes * 100));
        }).then((stop) => { if (disposed) stop(); else unlisten = stop; });
        return () => { disposed = true; unlisten?.(); };
    }, []);
    const handleDownloadModel = async (model: string) => {
        setDownloading(model); setDownloadPct(0);
        try { await invoke("download_whisper_model", { model }); }
        catch (e) { setStatus(String(e)); setDownloading(null); }
        await loadLocalStatus();
    };
    const handleSelectModel = async (model: string) => {
        try { await invoke("set_local_whisper_model", { model }); await loadLocalStatus(); onSave?.(); }
        catch (e) { setStatus(String(e)); }
    };
    return <div className="settings-content-panel fade-in">
        <p className="section-desc">Download a speech model once. Recordings are transcribed on this Mac; no cloud transcription service or API key is used.</p>
                    {/* Local Whisper — offline, first-class */}
                    <div className="provider-card active">
                        <div className="provider-header">
                            <span className="name">Speech model (Whisper)</span>
                            <span className="badge">In use</span>
                        </div>
                        <p className="provider-desc">
                            Runs on this Mac. After the download, transcription works offline
                            and nothing leaves this Mac.
                        </p>
                        {localStatus && (
                            <div className="input-group">
                                <label>
                                    {localStatus.ready ? "Models" : "Models: none downloaded yet"}
                                </label>
                                {localStatus.models.map((m) => (
                                    <div
                                        key={m.name}
                                        style={{
                                            display: "flex",
                                            alignItems: "center",
                                            gap: 8,
                                            padding: "6px 0",
                                            fontSize: 12,
                                        }}
                                    >
                                        <input
                                            type="radio"
                                            name="whisper-model"
                                            checked={m.active}
                                            disabled={!m.installed}
                                            onChange={() => handleSelectModel(m.name)}
                                            aria-label={`Use ${m.name}`}
                                            style={{ accentColor: "var(--hazard-yellow)" }}
                                        />
                                        <span style={{ minWidth: 110, fontWeight: 600 }}>{m.name}</span>
                                        <span style={{ flex: 1, opacity: 0.7 }}>
                                            {m.description} ({m.size_mb} MB)
                                        </span>
                                        {m.installed ? (
                                            <span className="badge">Installed</span>
                                        ) : downloading === m.name ? (
                                            <span style={{ minWidth: 90 }}>{downloadPct}%…</span>
                                        ) : (
                                            <button
                                                className="btn-secondary"
                                                style={{ padding: "2px 10px", fontSize: 11 }}
                                                disabled={downloading !== null}
                                                onClick={() => handleDownloadModel(m.name)}
                                            >
                                                Download
                                            </button>
                                        )}
                                    </div>
                                ))}
                            </div>
                        )}
                    </div>

        {status && <p role="status">{status}</p>}
    </div>;
}
