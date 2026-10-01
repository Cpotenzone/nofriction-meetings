// noFriction Meetings - Live Transcript
//
// One job: make what's being said readable the instant it's said.
// Text is set as calm paragraphs in a single reading column. The words
// still being spoken appear in place, dimmed, and firm up as Whisper
// finalizes them — nothing jumps, nothing re-flows below the fold.

import { useEffect, useRef, useMemo } from "react";
import type { LiveTranscript } from "../hooks/useTranscripts";
import { MicIcon } from "./icons";
import "./LiveTranscript.css";

interface LiveTranscriptProps {
    transcripts: LiveTranscript[];
    isRecording: boolean;
    onStartRecording?: () => void;
}

interface Paragraph {
    id: string;
    speaker: string | null;
    start: Date;
    finals: { id: string; text: string }[];
    interim: string | null;
}

/** A pause this long (or a speaker change) starts a new paragraph. */
const PARAGRAPH_GAP_MS = 6000;
/** Paragraphs are also capped so the timestamp gutter stays useful. */
const PARAGRAPH_MAX_MS = 45000;

function groupParagraphs(transcripts: LiveTranscript[]): Paragraph[] {
    const out: Paragraph[] = [];
    let cur: Paragraph | null = null;
    let lastAt = 0;

    for (const t of transcripts) {
        const at = t.timestamp.getTime();
        const newPara =
            !cur ||
            cur.speaker !== t.speaker ||
            at - lastAt > PARAGRAPH_GAP_MS ||
            at - cur.start.getTime() > PARAGRAPH_MAX_MS;

        if (newPara) {
            cur = { id: t.id, speaker: t.speaker, start: t.timestamp, finals: [], interim: null };
            out.push(cur);
        }
        if (t.isFinal) {
            cur!.finals.push({ id: t.id, text: t.text });
        } else {
            cur!.interim = t.text;
        }
        lastAt = Math.max(lastAt, at);
    }
    return out;
}

const formatTime = (date: Date) =>
    date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });

export function LiveTranscriptView({ transcripts, isRecording, onStartRecording }: LiveTranscriptProps) {
    const containerRef = useRef<HTMLDivElement>(null);
    const followRef = useRef(true);

    const paragraphs = useMemo(() => groupParagraphs(transcripts), [transcripts]);

    // Follow the live edge unless the reader has scrolled up
    useEffect(() => {
        const el = containerRef.current;
        if (!el || !followRef.current) return;
        requestAnimationFrame(() => {
            el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
        });
    }, [transcripts]);

    const handleScroll = () => {
        const el = containerRef.current;
        if (!el) return;
        followRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
    };

    if (paragraphs.length === 0) {
        return (
            <div className="lt-empty">
                <div className={`lt-empty__mark ${isRecording ? "is-listening" : ""}`}>
                    <MicIcon size={28} strokeWidth={1.5} />
                </div>
                <p className="lt-empty__title">{isRecording ? "Listening" : "Ready when you are"}</p>
                <p className="lt-empty__hint">
                    {isRecording
                        ? "Words appear here as they're spoken."
                        : "Transcribed on this Mac. Nothing leaves it."}
                </p>
                {!isRecording && onStartRecording && (
                    <button className="lt-empty__action" onClick={onStartRecording} type="button">
                        Start recording
                    </button>
                )}
            </div>
        );
    }

    return (
        <div ref={containerRef} className="lt" onScroll={handleScroll} aria-live="polite">
            {paragraphs.map((p) => (
                <section key={p.id} className="lt-para">
                    <time className="lt-para__time" dateTime={p.start.toISOString()}>
                        {formatTime(p.start)}
                    </time>
                    <div className="lt-para__body">
                        {p.speaker && <div className="lt-para__speaker">{p.speaker}</div>}
                        <p className="lt-para__text">
                            {p.finals.map((f) => (
                                <span key={f.id} className="lt-final">
                                    {f.text}{" "}
                                </span>
                            ))}
                            {p.interim && (
                                <span className="lt-interim">
                                    {p.interim}
                                    <span className="lt-caret" aria-hidden />
                                </span>
                            )}
                        </p>
                    </div>
                </section>
            ))}
        </div>
    );
}
