// One safety model for every delete in the app: the item disappears at
// once, a toast offers Undo for a few seconds, then the delete is applied
// (the same shape as the transcript's Delete toast in Redaction.tsx).

import { useCallback, useEffect, useRef, useState } from "react";
import "./redaction/Redaction.css";

interface Pending {
    label: string;
    until: number;
    apply: () => Promise<void>;
    restore: () => void;
}

export const UNDO_SECONDS = 5;

/**
 * Keeps at most one pending delete. `start` hides the item (the caller
 * already did) and schedules `apply`; `Undo` calls `restore` instead. A
 * newer delete applies the older one at once.
 */
export function useUndoDelete(onError?: (e: unknown) => void) {
    const [pending, setPending] = useState<Pending | null>(null);
    const [now, setNow] = useState(() => Date.now());
    const pendingRef = useRef<Pending | null>(null);
    pendingRef.current = pending;

    const commit = useCallback(
        async (p: Pending) => {
            try {
                await p.apply();
            } catch (e) {
                p.restore();
                onError?.(e);
            }
        },
        [onError],
    );

    const start = useCallback(
        (label: string, apply: () => Promise<void>, restore: () => void) => {
            const prev = pendingRef.current;
            if (prev) void commit(prev);
            setPending({ label, until: Date.now() + UNDO_SECONDS * 1000, apply, restore });
            setNow(Date.now());
        },
        [commit],
    );

    useEffect(() => {
        if (!pending) return;
        const t = window.setInterval(() => {
            const n = Date.now();
            setNow(n);
            if (n >= pending.until) {
                setPending(null);
                void commit(pending);
            }
        }, 250);
        return () => window.clearInterval(t);
    }, [pending, commit]);

    // Leaving the view applies the delete (never silently dropped)
    useEffect(
        () => () => {
            const p = pendingRef.current;
            if (p) void commit(p);
        },
        [commit],
    );

    const undo = useCallback(() => {
        const p = pendingRef.current;
        if (!p) return;
        setPending(null);
        p.restore();
    }, []);

    const applyNow = useCallback(() => {
        const p = pendingRef.current;
        if (!p) return;
        setPending(null);
        void commit(p);
    }, [commit]);

    const toast = pending ? (
        <div className="rd-toast" role="status" aria-live="polite">
            <span>{pending.label}</span>
            <button onClick={undo}>Undo</button>
            <span className="rd-toast-count">{Math.max(0, Math.ceil((pending.until - now) / 1000))}s</span>
            <button onClick={applyNow} aria-label="Apply now" title="Apply now">
                ✕
            </button>
        </div>
    ) : null;

    return { start, toast, pendingDelete: !!pending };
}
