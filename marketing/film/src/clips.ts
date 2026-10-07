// Which piece of real app footage each shot uses, and where in the clip it
// starts (seconds). Files live in marketing/out/footage (gitignored), made
// by capture/ios/capture.sh and mac-harness/capture.mjs. The cut notes
// below are from those captures; re-check them after a re-capture.
import type { Zoom } from "./components/Screen";

type Use = { clip: string; from: number; rate?: number; zoom?: Zoom };

const ios = (name: string) => `ios/${name}.mp4`;
const mac = (name: string) => `mac/${name}.mp4`;
const watch = (name: string) => `watch/${name}.mp4`;

// Cut notes (s):
// ios-01-record-sheet  1.3 Record tapped, sheet rises · 4.6 Class · 6.3 60 min · 7.0 BIO 101 chip
// ios-01b-record-start 0.8 Start · 1.0 live screen · 2.8 first words
// ios-03-mark          2.3 Mark, ★ Important · 4.0 On the test · 6.0 Question
// ios-04-library       1.6 BIO 101 chip · 2.2 filtered list
// ios-05-lecture       1.5 lecture opens · 3.5-6 scroll down · 6.5-8.5 back up
// ios-06-review        1.4 study guide · 3.2 flashcards · 4.7 flip (answer to 6.35) · 6.35 quiz · 8.3 answer + explanation
// watch-01-start       0-3.3 Record · 3.4 How long? · 5.5 Notebook · 7.6 recording · 9.8 ★ marked
// mac-01-rewind        1.25 roadmap · 2.85 ★ pin (roadmap, 3:30 Important) · 4.8 sign-ups · 6.4 ? pin · 8.15 ✎ pin
// mac-04-record-picker 1.05 sheet · 2.25 Class · 3.1 60 min · 4.9 BIO 101 · 6.05 Start · 7.4 first words
// mac-05-mark          1.5 Mark → ★ Important · 3.1 On the test · 4.2-5.6 note · 6.4 closes
// mac-06-review        0-1.6 summary · 1.63 flashcards · 2.93 flip · 4.87 quiz · 6.38 answer + explanation

/** Hero film shots (scene timing is in timing.ts and the scenes). */
export const CLIPS = {
  // iPhone (iOS Simulator, scripted UI test, demo data)
  iosRecord: { clip: ios("ios-01-record-sheet"), from: 1.2, rate: 1.15 },
  iosLive: { clip: ios("ios-02-live"), from: 0.4 },
  iosMark: { clip: ios("ios-03-mark"), from: 1.0 },
  iosLecture: { clip: ios("ios-05-lecture"), from: 1.6 },
  // Apple Watch (watchOS Simulator, -NFWatchDemo)
  watchStart: { clip: watch("watch-01-start"), from: 0.4 },
  watchClass: { clip: watch("watch-02-class"), from: 0 },
  // Mac (the real React UI in Chromium, mocked Tauri backend, demo data)
  macRewind: { clip: mac("mac-01-rewind"), from: 0.6 },
  macLive: { clip: mac("mac-02-live"), from: 0 },
  macHome: { clip: mac("mac-03-home"), from: 0 },
  macRecord: { clip: mac("mac-04-record-picker"), from: 0.9 },
  macReview: { clip: mac("mac-06-review"), from: 0.8 },
} satisfies Record<string, Use>;

/** App preview shots: footage, in-point and length in frames (30 fps). */
type PUse = Use & { dur: number };
export const P = {
  // iPhone preview: 864 frames
  iosRecord: { clip: ios("ios-01-record-sheet"), from: 1.0, dur: 186 },
  iosStart: { clip: ios("ios-01b-record-start"), from: 0.4, dur: 72 },
  iosLive: { clip: ios("ios-02b-live-class"), from: 2.0, dur: 96 },
  iosMark: { clip: ios("ios-03-mark"), from: 2.0, dur: 126 },
  iosLibrary: { clip: ios("ios-04-library"), from: 1.0, dur: 81 },
  iosLecture: { clip: ios("ios-05-lecture"), from: 1.3, dur: 90 },
  iosCards: { clip: ios("ios-06-review"), from: 3.0, dur: 96 },
  iosQuiz: { clip: ios("ios-06-review"), from: 6.3, dur: 117 },
  // Mac preview: 864 frames
  macHome: { clip: mac("mac-03-home"), from: 0, dur: 90 },
  macRecord: { clip: mac("mac-04-record-picker"), from: 0.8, dur: 165 },
  macLive: { clip: mac("mac-02-live"), from: 0.5, dur: 126 },
  macMark: { clip: mac("mac-05-mark"), from: 1.0, dur: 168, zoom: { scale: 1.4, x: 0.86, y: 0.12, at: 4, dur: 36 } },
  macRewind: { clip: mac("mac-01-rewind"), from: 1.0, dur: 192 },
  macReview: { clip: mac("mac-06-review"), from: 3.6, dur: 123 },
} satisfies Record<string, PUse>;
