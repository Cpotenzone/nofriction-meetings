// Flashcards: flip with a click or Space, then Known (→ or K) or Again
// (← or A); "Again" cards come back next round. Shuffle with S.
// Card text is model output: rendered as text only.

import { useEffect, useMemo, useState } from "react";
import {
    currentCard,
    deckDone,
    deckProgress,
    flip,
    markCard,
    newDeck,
    shuffleDeck,
    type Card,
    type DeckState,
} from "../../lib/studyLogic";

export function FlashcardDeck({ cards }: { cards: Card[] }) {
    const key = useMemo(() => cards.map((c) => c.front).join("\u0000"), [cards]);
    const [deck, setDeck] = useState<DeckState>(() => newDeck(cards.length));
    useEffect(() => setDeck(newDeck(cards.length)), [key, cards.length]);

    const idx = currentCard(deck);
    const card = idx === null ? null : cards[idx];
    const p = deckProgress(deck, cards.length);

    const onKey = (e: React.KeyboardEvent) => {
        if (e.metaKey || e.ctrlKey || e.altKey) return;
        const k = e.key.toLowerCase();
        // Space/Enter on another button (Known, Shuffle…) presses that button
        const onCard = e.target === e.currentTarget || (e.target as HTMLElement).classList.contains("study-card");
        if ((k === " " || k === "enter") && onCard) {
            e.preventDefault();
            setDeck(flip);
        } else if ((k === "arrowright" || k === "k") && deck.flipped) {
            e.preventDefault();
            setDeck((d) => markCard(d, "known"));
        } else if ((k === "arrowleft" || k === "a") && deck.flipped) {
            e.preventDefault();
            setDeck((d) => markCard(d, "again"));
        } else if (k === "s") {
            e.preventDefault();
            setDeck((d) => shuffleDeck(d));
        }
    };

    if (cards.length === 0) return null;

    return (
        <div className="study-deck" tabIndex={0} onKeyDown={onKey} aria-label="Flashcards. Space flips, K known, A again, S shuffles">
            <div className="study-deck__meta">
                <span>
                    {p.known} of {p.total} known{deck.round > 1 ? ` · round ${deck.round}` : ""}
                </span>
                <div className="study-deck__meta-actions">
                    <button type="button" className="rd-btn rd-btn-ghost" onClick={() => setDeck((d) => shuffleDeck(d))} disabled={deckDone(deck)} title="Shuffle the cards left (S)">
                        Shuffle
                    </button>
                    <button type="button" className="rd-btn rd-btn-ghost" onClick={() => setDeck(newDeck(cards.length))} title="Start over with every card">
                        Start over
                    </button>
                </div>
            </div>
            <div className="study-progress" aria-hidden>
                <div style={{ width: `${(p.known / Math.max(1, p.total)) * 100}%` }} />
            </div>

            {card ? (
                <>
                    <button
                        type="button"
                        className={`study-card${deck.flipped ? " is-flipped" : ""}`}
                        onClick={() => setDeck(flip)}
                        aria-live="polite"
                        aria-label={deck.flipped ? `Answer: ${card.back}. Click to see the question.` : `Question: ${card.front}. Click to see the answer.`}
                    >
                        <span className="study-card__side">{deck.flipped ? "Answer" : "Question"}</span>
                        <span className="study-card__text">{deck.flipped ? card.back : card.front}</span>
                        <span className="study-card__hint">{deck.flipped ? "" : "Click or press Space to flip"}</span>
                    </button>
                    <div className="study-deck__actions">
                        <button type="button" className="mn-btn" onClick={() => setDeck((d) => markCard(d, "again"))} disabled={!deck.flipped} title="Show it again later (A or ←)">
                            Again
                        </button>
                        <button type="button" className="mn-btn primary" onClick={() => setDeck((d) => markCard(d, "known"))} disabled={!deck.flipped} title="I knew it (K or →)">
                            Known
                        </button>
                    </div>
                </>
            ) : (
                <div className="study-done" role="status">
                    <strong>All {cards.length} cards known.</strong>
                    <button type="button" className="mn-btn" onClick={() => setDeck(newDeck(cards.length))}>
                        Study again
                    </button>
                </div>
            )}
        </div>
    );
}
