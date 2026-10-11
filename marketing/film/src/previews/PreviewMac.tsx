import React from "react";
import { P } from "../clips";
import { Preview, Shot } from "./Preview";

// Mac app preview: 1920x1080, 30 fps, 28.8 s (864 frames). The 3.7 window,
// one take after another: record a class, mark it, rewind a meeting, the
// study guide, Chat, Sync.
const SHOTS: Shot[] = [
  { ...P.macRecord, label: "Record", caption: "Meetings, classes and everyday *life*" },
  { ...P.macLive, label: "On-device", caption: "Transcribed right on your *Mac*" },
  { ...P.macMark, label: "Marks", caption: "Mark what *matters*" },
  { ...P.macRewind, label: "Rewind", caption: "Every screen, next to what was *said*" },
  { ...P.macReview, label: "Review guide", caption: "Notes and a review guide from any *recording*" },
  { ...P.macChat, label: "Chat", caption: "Ask your recordings. Answers cite the *moment*" },
  { ...P.macSync, label: "Sync", caption: "iPhone and Mac in sync, with no *server*" },
];

const W = 1568;
const H = Math.round((W * 9) / 16);

export const PreviewMac: React.FC<{ music: boolean }> = ({ music }) => (
  <Preview
    shots={SHOTS}
    screen={{ left: (1920 - W) / 2, top: 172, width: W, height: H, radius: 14 }}
    captionTop={30}
    captionSize={56}
    labelSize={20}
    music={music}
  />
);
