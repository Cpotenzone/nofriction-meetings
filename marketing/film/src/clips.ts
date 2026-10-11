// Which piece of real app footage each shot uses, and where in the clip it
// starts (seconds). Files live in marketing/out/footage (gitignored):
//   mac/     mac-harness/capture.mjs (the 3.7 Mac UI, demo data)
//   stills/  the 1.0.1 App Store screenshots (iPhone 6.9", Apple Watch 46 mm),
//            copied in by scripts/link-media.mjs
//   ios/, watch/  capture/ios/capture.sh (the iOS 1.0.2 UI in the Simulator;
//            only the iPhone app preview uses them, see README)
// The cut notes below are from the captures; re-check them after a re-capture.
import type { Zoom } from "./components/Screen";

type Use = { clip: string; from: number; rate?: number; zoom?: Zoom };

const ios = (name: string) => `ios/${name}.mp4`;
const mac = (name: string) => `mac/${name}.mp4`;
const still = (name: string) => `stills/${name}.png`;

// Cut notes (s, page time = clip time):
// mac-01-record     1.0 sheet · 2.5 Class · 3.5 60 min · 4.0-4.5 notebook "BIO" · 5.0 BIO 101 · 6.0 Start recording, "Listening" · 7.5 first words
// mac-02-mark-stop  0-1.5 lines streaming · 2.0 Mark (card) · 4.0 On the test · 4.5-6.0 note typed · 6.5 Enter · 8.3 Stop → the lecture in Recordings (Rewind) · 10.5 the On the test pin
// mac-03-rewind     0.6-2.6 playhead dragged 0:10 → roadmap · 3.9 ★ Important pin (3:34) · 6.3 sign-ups 4:36 · 7.7 ? pin
// mac-04-notes      0-1.5 meeting notes · 1.5-4.5 scroll to the end
// mac-05-review     0-1.5 summary · 2.0 Flashcards · 3.0 flip · 5.0 Practice quiz · 6.5 answer + explanation
// mac-06-chat       1.0-3.3 typing · 3.5 answer with citations · 5.0 sources open
// mac-07-sync       1.1 Settings · 2.5 Sync · 3.8 Pair a device · 4.0 QR code
// mac-08-home       idle Record view: "Ready when you are", the privacy promise
// mac-09-notebooks  1.5 BIO 101 chip · 2.9 BIO 101: Cell Biology opens

/** Hero film shots (scene timing is in timing.ts and the scenes). */
export const CLIPS = {
  macRecord: { clip: mac("mac-01-record"), from: 0.9, rate: 1.2 },
  macMark: { clip: mac("mac-02-mark-stop"), from: 1.0, rate: 1.7, zoom: { scale: 1.45, x: 0.9, y: 0, at: 10, dur: 24, out: 96 } },
  macRewind: { clip: mac("mac-03-rewind"), from: 0.4, rate: 1.3 },
  macNotes: { clip: mac("mac-04-notes"), from: 0.6, rate: 1.4 },
  macReview: { clip: mac("mac-05-review"), from: 4.0, rate: 1.4 },
  macChat: { clip: mac("mac-06-chat"), from: 1.0, rate: 1.35, zoom: { scale: 1.4, x: 0.2, y: 0.4, at: 62, dur: 30 } },
  macSync: { clip: mac("mac-07-sync"), from: 3.0, rate: 1.2 },
  macHome: { clip: mac("mac-08-home"), from: 0 },
  // Real app screenshots (App Store 1.0.1), as stills
  phoneRecord: { clip: still("iphone-01-record"), from: 0 },
  phoneRecordings: { clip: still("iphone-06-meetings"), from: 0 },
  watchRecord: { clip: still("watch-02-record"), from: 0 },
  watchClass: { clip: still("watch-04-class"), from: 0 },
} satisfies Record<string, Use>;

/** App preview shots: footage, in-point and length in frames (30 fps). */
type PUse = Use & { dur: number };
export const P = {
  // iPhone preview: 864 frames (capture/ios/capture.sh iphone, the 1.0.2 app)
  // ios-01-record-sheet  1.0 sheet · 2.0 Class · 3.5 60 min · 4.5 BIO 101 · 6.5 Record · 7.0 recording · 8.0 first words
  // ios-03-mark          0-2.5 lines arriving · 2.5 Mark this moment (★ Important) · 4.0 On the test · 6.5 Stop → Ready
  // ios-04-library       1.0 BIO 101 chip · 3.0 search · 4.5-7.5 "exam", letter by letter → the matching line · 12.5 tap → 13.5 the line, highlighted
  // ios-08-meeting       1.0 Notes with Make again · Follow-up email · 3.0 scroll · 6.0 tap · 6.5 the email
  // ios-06-review        1.0 study guide · 3.0 flashcard · 3.5 flipped · 5.0 Practice quiz · 6.0 answered
  // ios-09-chat          1.0 a suggested question · 2.0 the cited answer · 4.5 a citation opens Lecture 7
  // ios-10-sync          1.0 Sync with your Mac · 1.5 the pairing screen
  iosRecord: { clip: ios("ios-01-record-sheet"), from: 0.7, rate: 1.75, dur: 132 },
  iosMark: { clip: ios("ios-03-mark"), from: 0.8, rate: 1.45, dur: 126 },
  iosChip: { clip: ios("ios-04-library"), from: 0.6, rate: 1.5, dur: 48 },
  iosSearch: { clip: ios("ios-04-library"), from: 4.3, rate: 2.0, dur: 60 },
  iosJump: { clip: ios("ios-04-library"), from: 12.2, dur: 66 },
  iosNotes: { clip: ios("ios-08-meeting"), from: 1.0, dur: 42 },
  iosEmail: { clip: ios("ios-08-meeting"), from: 2.9, rate: 1.7, dur: 90 },
  iosReview: { clip: ios("ios-06-review"), from: 1.0, rate: 1.6, dur: 120 },
  iosChat: { clip: ios("ios-09-chat"), from: 0.7, rate: 1.25, dur: 102 },
  iosSync: { clip: ios("ios-10-sync"), from: 0.4, dur: 78 },
  // Mac preview: 864 frames
  macRecord: { clip: mac("mac-01-record"), from: 0.8, dur: 165 },
  macLive: { clip: mac("mac-02-mark-stop"), from: 0.2, dur: 48 },
  macMark: { clip: mac("mac-02-mark-stop"), from: 1.8, dur: 150, zoom: { scale: 1.4, x: 0.9, y: 0, at: 2, dur: 30, out: 120 } },
  macRewind: { clip: mac("mac-03-rewind"), from: 0.4, dur: 180 },
  macReview: { clip: mac("mac-05-review"), from: 3.8, dur: 132 },
  macChat: { clip: mac("mac-06-chat"), from: 2.6, dur: 111, zoom: { scale: 1.35, x: 0.2, y: 0.4, at: 22, dur: 30 } },
  macSync: { clip: mac("mac-07-sync"), from: 3.0, dur: 78 },
} satisfies Record<string, PUse>;
