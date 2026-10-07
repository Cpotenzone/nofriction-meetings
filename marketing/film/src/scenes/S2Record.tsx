import React from "react";
import { AbsoluteFill, interpolate, Sequence, spring, useCurrentFrame, useVideoConfig } from "remotion";
import { Backdrop, Enter, RecordButton } from "../components/Motifs";
import { Label, Reveal } from "../components/Reveal";
import { Screen } from "../components/Screen";
import { CLIPS } from "../clips";
import { C, easeIn } from "../theme";

// 2. Tap record on iPhone, Mac or Watch. 216 frames (3 bars).
export const S2Record: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const pop = spring({ frame, fps, config: { damping: 13, stiffness: 140, mass: 0.8 } });
  const press = spring({ frame: frame - 24, fps, config: { damping: 18, stiffness: 200 } });
  const away = interpolate(frame, [50, 66], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeIn });
  const btn = interpolate(pop, [0, 1], [0.3, 1]) * (1 - away * 0.6);
  const devices = 58;
  return (
    <AbsoluteFill>
      <Backdrop glowY={60} glow={0.9} />
      <div style={{ position: "absolute", top: 96, left: 0, right: 0 }}>
        <Reveal text="Tap *record.*" at={4} size={92} align="center" />
        <Reveal text="On iPhone, Mac or Watch." at={44} stagger={3} size={60} weight={600} color={C.text2} align="center" style={{ marginTop: 14 }} />
      </div>
      {/* the button, pressed, then handing over to the devices */}
      <div style={{ position: "absolute", left: 960 - 100, top: 470, width: 200, height: 200, transform: `scale(${btn}) translateY(${away * 120}px)`, opacity: 1 - away }}>
        <RecordButton size={200} pressed={press} />
      </div>
      <Sequence from={devices} layout="none">
        <div style={{ position: "absolute", left: 104, top: 334 }}>
          <Enter at={0} dur={30} from={{ y: 120, scale: 0.94 }}>
            <Screen clip={CLIPS.macRecord.clip} from={CLIPS.macRecord.from} kind="mac" width={1060} />
            <Label at={18} color={C.text3} style={{ marginTop: 22, textAlign: "center" }}>Mac</Label>
          </Enter>
        </div>
        <div style={{ position: "absolute", left: 1226, top: 290 }}>
          <Enter at={6} dur={30} from={{ y: 160, scale: 0.94 }}>
            <Screen clip={CLIPS.iosRecord.clip} from={CLIPS.iosRecord.from} kind="phone" width={300} glow={0.6} />
            <Label at={24} color={C.text3} style={{ marginTop: 22, textAlign: "center" }}>iPhone</Label>
          </Enter>
        </div>
        <div style={{ position: "absolute", left: 1598, top: 520 }}>
          <Enter at={12} dur={30} from={{ y: 160, scale: 0.94 }}>
            <Screen clip={CLIPS.watchStart.clip} from={CLIPS.watchStart.from} kind="watch" width={196} />
            <Label at={30} color={C.text3} style={{ marginTop: 22, textAlign: "center" }}>Watch</Label>
          </Enter>
        </div>
      </Sequence>
    </AbsoluteFill>
  );
};
