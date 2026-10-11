// The score (scripts/make-score.mjs) runs at 100 BPM: one beat = 18 frames
// at 30 fps, one bar = 72 frames = 2.4 s. Scenes are cut on bars so the
// music's hits land on the cuts.
export const FPS = 30;
export const BEAT = 18;
export const BAR = 72;

/** Hero film scenes, in bars (start, length). Total 14 bars = 33.6 s.
 *  Keep in step with filmArrangement() in scripts/make-score.mjs. */
export const FILM_SCENES = {
  record: { from: 0, bars: 2 },
  mark: { from: 2, bars: 2 },
  rewind: { from: 4, bars: 2 },
  notes: { from: 6, bars: 2 },
  chat: { from: 8, bars: 2 },
  sync: { from: 10, bars: 1 },
  privacy: { from: 11, bars: 1 },
  end: { from: 12, bars: 2 },
} as const;

export const FILM_BARS = 14;
export const FILM_FRAMES = FILM_BARS * BAR; // 1008

/** App previews: 12 bars = 28.8 s (Apple: 15–30 s). */
export const PREVIEW_BARS = 12;
export const PREVIEW_FRAMES = PREVIEW_BARS * BAR; // 864
