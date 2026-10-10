// Settings → Sync: pair an iPhone, the paired devices with Forget, and the
// "Last synced" line (docs/SYNC.md). A noFriction Pro feature: device to
// device on the local network, no server.

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { withPro, requestPaywall } from "../../lib/build";
import { countdown, lastSyncedLabel } from "../../lib/syncLogic";

interface SyncDevice {
    id: string;
    name: string;
    paired_at: string;
    last_sync_at: string | null;
    last_error: string | null;
}

interface SyncStatus {
    enabled: boolean;
    running: boolean;
    pro: boolean;
    devices: SyncDevice[];
    last_synced_at: string | null;
}

interface PairingInfo {
    link: string;
    qr_svg: string;
    expires_in_secs: number;
}

const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function SyncSettings() {
    const [status, setStatus] = useState<SyncStatus | null>(null);
    const [pairing, setPairing] = useState<PairingInfo | null>(null);
    const [expiresAt, setExpiresAt] = useState(0);
    const [now, setNow] = useState(Date.now());
    const [error, setError] = useState<string | null>(null);
    const [copied, setCopied] = useState(false);
    const [forgetting, setForgetting] = useState<SyncDevice | null>(null);

    const refresh = useCallback(() => {
        invoke<SyncStatus>("sync_status").then(setStatus).catch((e) => setError(errText(e)));
    }, []);

    useEffect(() => {
        refresh();
        let off: (() => void) | undefined;
        let disposed = false;
        listen("sync-changed", () => refresh()).then((u) => {
            if (disposed) u();
            else off = u;
        });
        return () => {
            disposed = true;
            off?.();
        };
    }, [refresh]);

    // Pairing countdown; a paired device closes the code
    useEffect(() => {
        if (!pairing) return;
        const t = setInterval(() => setNow(Date.now()), 1000);
        return () => clearInterval(t);
    }, [pairing]);
    const secondsLeft = pairing ? (expiresAt - now) / 1000 : 0;
    useEffect(() => {
        if (pairing && secondsLeft <= 0) setPairing(null);
    }, [pairing, secondsLeft]);
    const deviceCount = status?.devices.length ?? 0;
    const [pairedCountAtStart, setPairedCountAtStart] = useState(0);
    useEffect(() => {
        if (pairing && deviceCount > pairedCountAtStart) setPairing(null);
    }, [pairing, deviceCount, pairedCountAtStart]);

    const setEnabled = async (enabled: boolean) => {
        setError(null);
        try {
            const s = await withPro(() => invoke<SyncStatus>("sync_set_enabled", { enabled }), "sync");
            setStatus(s);
            if (!enabled) setPairing(null);
        } catch (e) {
            setError(errText(e));
        }
    };

    const pair = async () => {
        setError(null);
        setCopied(false);
        try {
            const info = await withPro(() => invoke<PairingInfo>("sync_pair_start"), "sync");
            setPairedCountAtStart(status?.devices.length ?? 0);
            setPairing(info);
            setExpiresAt(Date.now() + info.expires_in_secs * 1000);
            setNow(Date.now());
            refresh();
        } catch (e) {
            setError(errText(e));
        }
    };

    const cancelPairing = () => {
        invoke("sync_pair_cancel").catch(() => {});
        setPairing(null);
    };

    const copyLink = async () => {
        if (!pairing) return;
        try {
            await navigator.clipboard.writeText(pairing.link);
            setCopied(true);
        } catch (e) {
            setError(errText(e));
        }
    };

    const forget = async (d: SyncDevice) => {
        setForgetting(null);
        try {
            setStatus(await invoke<SyncStatus>("sync_forget", { deviceId: d.id }));
        } catch (e) {
            setError(errText(e));
        }
    };

    if (status === null) {
        return <div className="loading-spinner" style={{ margin: "16px auto" }} />;
    }

    if (!status.pro) {
        return (
            <section className="settings-section">
                <h3>Sync with your iPhone</h3>
                <p className="section-desc">
                    Keep the same recordings, transcripts, notes, marks and links on your iPhone and this Mac. They sync directly
                    over your Wi-Fi, encrypted, with no server in between. Sync is part of noFriction Pro.
                </p>
                <button className="btn-primary" onClick={() => requestPaywall("sync")}>
                    See noFriction Pro
                </button>
            </section>
        );
    }

    return (
        <>
            <section className="settings-section">
                <h3>Sync with your iPhone</h3>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Sync on this Mac</span>
                        <span className="label-sub">
                            Your iPhone and this Mac talk directly on the same Wi-Fi, encrypted. Nothing goes through a server.
                        </span>
                    </div>
                    <div
                        className={`toggle-switch ${status.enabled ? "active" : ""}`}
                        onClick={() => setEnabled(!status.enabled)}
                        role="switch"
                        aria-checked={status.enabled}
                        aria-label="Sync on this Mac"
                        tabIndex={0}
                        onKeyDown={(e) => (e.key === " " || e.key === "Enter") && setEnabled(!status.enabled)}
                    >
                        <div className="toggle-knob" />
                    </div>
                </div>
                <p className="section-desc">
                    {lastSyncedLabel(status.last_synced_at)}. Your iPhone syncs when you open noFriction on it, when a recording
                    stops, and when you tap Sync now. Keep noFriction open on this Mac.
                </p>
            </section>

            <section className="settings-section">
                <h3>Devices</h3>
                {status.devices.length === 0 && <p className="section-desc">No iPhone is paired yet.</p>}
                {status.devices.map((d) => (
                    <div className="settings-row" key={d.id}>
                        <div className="settings-label">
                            <span className="label-main">{d.name}</span>
                            <span className="label-sub">
                                {lastSyncedLabel(d.last_sync_at)}
                                {d.last_error ? ` · ${d.last_error}` : ""}
                            </span>
                        </div>
                        {forgetting?.id === d.id ? (
                            <div className="about-links">
                                <button className="btn-secondary" onClick={() => setForgetting(null)}>
                                    Cancel
                                </button>
                                <button className="btn-secondary" onClick={() => forget(d)}>
                                    Forget
                                </button>
                            </div>
                        ) : (
                            <button className="btn-secondary" onClick={() => setForgetting(d)}>
                                Forget…
                            </button>
                        )}
                    </div>
                ))}
                {forgetting && (
                    <p className="section-desc">
                        {forgetting.name} will stop syncing with this Mac. Recordings already on it stay. To sync again, pair it again.
                    </p>
                )}

                {pairing ? (
                    <div className="sync-pairing">
                        <p className="section-desc">
                            On your iPhone, open noFriction, go to Settings → Sync → Pair with your Mac, and scan this code. It works
                            once, for {countdown(secondsLeft)}.
                        </p>
                        <div
                            className="sync-qr"
                            role="img"
                            aria-label="Pairing code"
                            // The SVG is generated by the app (qrcode crate), never from user input
                            dangerouslySetInnerHTML={{ __html: pairing.qr_svg }}
                        />
                        <div className="about-links">
                            <button className="btn-secondary" onClick={copyLink}>
                                {copied ? "Copied" : "Copy pairing link"}
                            </button>
                            <button className="btn-secondary" onClick={cancelPairing}>
                                Done
                            </button>
                        </div>
                        <p className="section-desc">
                            Can't scan? Copy the link, and on your iPhone tap Paste pairing link (Universal Clipboard).
                        </p>
                    </div>
                ) : (
                    <div className="settings-row">
                        <div className="settings-label">
                            <span className="label-main">Pair a device</span>
                            <span className="label-sub">Show a code to scan with noFriction on your iPhone.</span>
                        </div>
                        <button className="btn-secondary" onClick={pair}>
                            Pair a device
                        </button>
                    </div>
                )}
            </section>

            <section className="settings-section">
                <h3>What syncs</h3>
                <p className="section-desc">
                    Recordings (title, type, notebook, calendar details), transcripts, notes, marks, links and topics. Delete and
                    Strike from the record apply on both devices. Photos, screens, audio, chats and review guides stay on the device
                    that made them.
                </p>
            </section>
            {error && <p className="ai-error-text">{error}</p>}
        </>
    );
}
