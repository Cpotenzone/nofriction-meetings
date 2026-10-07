import React from "react";
import { P } from "../clips";
import { Preview, Shot } from "./Preview";

// iPhone 6.9" app preview: 886x1920 portrait, 30 fps, 28.8 s (864 frames).
// One continuous story in the real app: set up a class, record it, mark a
// moment, find it in its notebook, read the notes, review.
const SHOTS: Shot[] = [
  { ...P.iosRecord, label: "Record", caption: "Meetings, classes\nand everyday *life*" },
  { ...P.iosStart, label: "On-device", caption: "Transcribed on your\niPhone, even *offline*" },
  { ...P.iosLive },
  { ...P.iosMark, label: "Marks", caption: "Mark what\n*matters*" },
  { ...P.iosLibrary, label: "Notebooks", caption: "Every recording\nin its *notebook*" },
  { ...P.iosLecture, label: "Notes", caption: "Notes in the\nright *style*" },
  { ...P.iosCards, label: "Review", caption: "Flashcards and a quiz\nfrom any *recording*" },
  { ...P.iosQuiz },
];

const W = 720;
const H = Math.round(W / (1320 / 2868));

export const PreviewIPhone: React.FC<{ music: boolean }> = ({ music }) => (
  <Preview
    shots={SHOTS}
    screen={{ left: (886 - W) / 2, top: 300, width: W, height: H, radius: W * 0.13 }}
    captionTop={72}
    captionSize={60}
    labelSize={24}
    music={music}
  />
);
