import React from "react";
import { P } from "../clips";
import { Preview, Shot } from "./Preview";

// iPhone 6.9" app preview: 886x1920, 30 fps, 28.8 s (864 frames). The
// redesigned app (Record · Recordings · Chat · Settings), one take after
// another: the Record sheet, Mark and Stop, a search that opens at the line,
// a meeting's Notes and follow-up email, a class's study guide, Chat with
// citations, Sync. Captions in sentence case, no brand names but noFriction.
const SHOTS: Shot[] = [
  { ...P.iosRecord, label: "Record", caption: "Meetings, classes\nand everyday *life*" },
  { ...P.iosMark, label: "Mark", caption: "Mark what *matters*,\nthen Stop" },
  { ...P.iosChip, label: "Recordings", caption: "Search every word.\nJump to the *moment*" },
  { ...P.iosSearch },
  { ...P.iosJump },
  { ...P.iosNotes, label: "Notes", caption: "Notes and a\nfollow-up *email*" },
  { ...P.iosEmail },
  { ...P.iosReview, label: "Review guide", caption: "Flashcards and a quiz\nfrom any *recording*" },
  { ...P.iosChat, label: "Chat", caption: "Ask your recordings.\nAnswers cite the *moment*" },
  { ...P.iosSync, label: "Sync", caption: "Device to device,\nwith no *server*" },
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
