// The score (scripts/make-score.mjs) runs at 100 BPM: one beat = 18 frames
// at 30 fps, one bar = 72 frames = 2.4 s. Scenes are cut on bars so the
// music's hits land on the cuts.
export const FPS = 30;
export const BEAT = 18;
export const BAR = 72;

/** Hero film scenes, in bars (start, length). Total 26 bars = 62.4 s. */
export const FILM_SCENES = {
  moments: { from: 0, bars: 4 },
  record: { from: 4, bars: 3 },
  transcribe: { from: 7, bars: 3 },
  mark: { from: 10, bars: 3 },
  rewind: { from: 13, bars: 4 },
  notes: { from: 17, bars: 3 },
  offline: { from: 20, bars: 3 },
  end: { from: 23, bars: 3 },
} as const;

export const FILM_BARS = 26;
export const FILM_FRAMES = FILM_BARS * BAR; // 1872

/** App previews: 12 bars = 28.8 s (Apple: 15–30 s). */
export const PREVIEW_BARS = 12;
export const PREVIEW_FRAMES = PREVIEW_BARS * BAR; // 864
