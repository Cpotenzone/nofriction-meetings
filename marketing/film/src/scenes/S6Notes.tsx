import React from "react";
import { AbsoluteFill, interpolate, useCurrentFrame } from "remotion";
import { CLIPS } from "../clips";
import { CardsIcon, KeyTermIcon, NotesIcon, QuizIcon } from "../components/Icons";
import { Backdrop, Chip, Enter } from "../components/Motifs";
import { Label, Reveal } from "../components/Reveal";
import { Screen } from "../components/Screen";
import { C, F } from "../theme";

// 6. Turn it into notes and a review guide. 216 frames (3 bars).
export const S6Notes: React.FC = () => {
  const frame = useCurrentFrame();
  const drift = interpolate(frame, [0, 216], [0, -20]);
  return (
    <AbsoluteFill>
      <Backdrop glowX={65} glowY={55} glow={1} />
      <div style={{ position: "absolute", left: 120, top: 96, width: 1600 }}>
        <Label at={0}>Notes · Review guide</Label>
        <Reveal text="Turn it into *notes* and a review guide." at={0} stagger={3} size={76} style={{ marginTop: 22 }} />
      </div>
      <div style={{ position: "absolute", left: 120, top: 330, display: "flex", flexDirection: "column", alignItems: "flex-start", gap: 16 }}>
        <Chip at={40} size={28} icon={<NotesIcon size={28} />}>Summary</Chip>
        <Chip at={50} size={28} icon={<KeyTermIcon size={28} />}>Key terms</Chip>
        <Chip at={60} size={28} icon={<CardsIcon size={28} />}>Flashcards</Chip>
        <Chip at={70} size={28} tone="yellow" icon={<QuizIcon size={28} color="#0a0a0a" />}>Practice quiz</Chip>
        <div style={{ marginTop: 26, width: 330, fontFamily: F.mono, fontSize: 17, lineHeight: 1.6, letterSpacing: "0.04em", color: C.text3, opacity: interpolate(frame, [70, 90], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }) }}>
          AI notes and guides with noFriction Pro: Apple on-device, or your own endpoint.
        </div>
      </div>
      <div style={{ position: "absolute", left: 790, top: 290 + drift * 0.5 }}>
        <Enter at={-8} dur={30} from={{ x: 100, scale: 0.96 }}>
          <Screen clip={CLIPS.macReview.clip} from={CLIPS.macReview.from} kind="mac" width={1010} />
        </Enter>
      </div>
      <div style={{ position: "absolute", left: 500, top: 300 + drift }}>
        <Enter at={-4} dur={30} from={{ y: 160, scale: 0.94 }}>
          <Screen clip={CLIPS.iosLecture.clip} from={CLIPS.iosLecture.from} kind="phone" width={330} glow={0.7} />
        </Enter>
      </div>
    </AbsoluteFill>
  );
};
