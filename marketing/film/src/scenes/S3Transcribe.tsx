import React from "react";
import { AbsoluteFill, interpolate, useCurrentFrame } from "remotion";
import { CLIPS } from "../clips";
import { BoltIcon, LockIcon, WifiOffIcon } from "../components/Icons";
import { Backdrop, Chip, Enter, Waveform } from "../components/Motifs";
import { Label, Reveal } from "../components/Reveal";
import { Screen } from "../components/Screen";
import { C } from "../theme";

// 3. It's transcribed right on your device. 216 frames (3 bars).
export const S3Transcribe: React.FC = () => {
  const frame = useCurrentFrame();
  const push = interpolate(frame, [0, 216], [1, 1.04]);
  return (
    <AbsoluteFill>
      <Backdrop glowX={72} glowY={50} glow={1} />
      <div style={{ position: "absolute", left: 130, top: 300, width: 840 }}>
        <Label at={0}>Live transcript</Label>
        <Reveal text={"Transcribed\nright on your *device.*"} at={0} stagger={4} size={82} style={{ marginTop: 26 }} />
        <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-start", gap: 18, marginTop: 56 }}>
          <Chip at={44} icon={<LockIcon size={30} />}>On-device</Chip>
          <Chip at={54} icon={<WifiOffIcon size={30} />}>No internet needed</Chip>
          <Chip at={64} icon={<BoltIcon size={30} />}>Live, as you talk</Chip>
        </div>
      </div>
      <div style={{ position: "absolute", left: 1010, top: 250, transform: `scale(${push})`, transformOrigin: "70% 50%" }}>
        <Enter at={-8} dur={30} from={{ x: 80, scale: 0.96 }}>
          <div style={{ opacity: 0.92 }}>
            <Screen clip={CLIPS.macLive.clip} from={CLIPS.macLive.from} kind="mac" width={800} />
          </div>
        </Enter>
      </div>
      <div style={{ position: "absolute", left: 1480, top: 140, transform: `scale(${push})`, transformOrigin: "50% 50%" }}>
        <Enter at={-6} dur={30} from={{ y: 140, scale: 0.94 }}>
          <Screen clip={CLIPS.iosLive.clip} from={CLIPS.iosLive.from} kind="phone" width={350} glow={1} />
        </Enter>
      </div>
      <div style={{ position: "absolute", left: 130, top: 940, opacity: interpolate(frame, [20, 40], [0, 0.9], { extrapolateRight: "clamp", extrapolateLeft: "clamp" }) }}>
        <Waveform width={360} height={44} bars={36} seed="s3" color={C.yellow} />
      </div>
    </AbsoluteFill>
  );
};
