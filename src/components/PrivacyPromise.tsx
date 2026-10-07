// noFriction Meetings - the home screen's privacy promise
//
// "Recording and transcription work offline. Nothing leaves this Mac unless
// you want it to." Shown large on the empty home screen and as a small badge
// once a transcript fills it. The copy (src/lib/privacyPromise.ts) only claims
// offline transcription when a Whisper model is installed.

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LockIcon } from "./icons";
import { openSettings } from "../lib/navigation";
import { promiseCopy } from "../lib/privacyPromise";
import "./PrivacyPromise.css";

/** Whether a transcription model is installed (null while unknown). */
export function useOfflineReady(): boolean | null {
    const [ready, setReady] = useState<boolean | null>(null);
    const refresh = useCallback(() => {
        invoke<{ ready: boolean }>("get_local_stt_status")
            .then((s) => setReady(Boolean(s.ready)))
            .catch(() => setReady(null));
    }, []);
    useEffect(() => {
        refresh();
        let disposed = false;
        let unlisten: (() => void) | undefined;
        // A model finished downloading (Settings or setup): offline now
        listen<{ done: boolean }>("whisper_download_progress", (e) => {
            if (e.payload.done) refresh();
        })
            .then((fn) => {
                if (disposed) fn();
                else unlisten = fn;
            })
            .catch(() => {});
        window.addEventListener("focus", refresh);
        return () => {
            disposed = true;
            unlisten?.();
            window.removeEventListener("focus", refresh);
        };
    }, [refresh]);
    return ready;
}

/** Large: the empty home screen, under "Ready when you are". */
export function PrivacyPromise() {
    const copy = promiseCopy(useOfflineReady());
    return (
        <div className="nf-promise" title={copy.explain}>
            <span className="nf-promise__icon">
                <LockIcon size={16} strokeWidth={1.75} />
            </span>
            <div className="nf-promise__text">
                <p className="nf-promise__headline">{copy.headline}</p>
                <p className="nf-promise__detail">{copy.detail}</p>
                {copy.needsModel && (
                    <button className="nf-promise__link" type="button" onClick={() => openSettings("transcription")}>
                        Download a transcription model
                    </button>
                )}
            </div>
        </div>
    );
}

/** Small: pinned to the bottom of the home transcript while it has text. */
export function PrivacyPromiseBadge() {
    const copy = promiseCopy(useOfflineReady());
    return (
        <div className="nf-promise-badge" title={copy.explain} role="note">
            <LockIcon size={12} strokeWidth={2} />
            <span>{copy.pill}</span>
        </div>
    );
}
