import React from "react";
import { AbsoluteFill, interpolate, spring, useCurrentFrame, useVideoConfig } from "remotion";
import { CLIPS } from "../clips";
import { RewindIcon } from "../components/Icons";
import { Backdrop } from "../components/Motifs";
import { Label, Reveal } from "../components/Reveal";
import { Screen } from "../components/Screen";
import { C, easeInOut } from "../theme";

// 5. Rewind to any moment, screens included, on Mac. 288 frames (4 bars).
// The signature shot: the real Rewind view, big.
export const S5Rewind: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const land = spring({ frame: frame + 6, fps, config: { damping: 20, stiffness: 90, mass: 1 } });
  const tilt = interpolate(land, [0, 1], [16, 0]);
  const push = interpolate(frame, [20, 288], [1, 1.07], { easing: easeInOut, extrapolateLeft: "clamp" });
  const swap = 146;
  // a streak of light across the window as it lands (the "rewind")
  const streak = interpolate(frame, [0, 22], [-0.3, 1.3], { extrapolateRight: "clamp" });
  return (
    <AbsoluteFill>
      <Backdrop glowY={62} glow={1.1} grid={0.6} />
      <div style={{ position: "absolute", left: 120, top: 62, display: "flex", alignItems: "center", gap: 18 }}>
        <div style={{ color: C.yellow, display: "flex" }}>
          <RewindIcon size={44} stroke={2} />
        </div>
        <Label at={2} size={24}>Rewind · Mac</Label>
      </div>
      <div style={{ position: "absolute", left: 120, top: 112 }}>
        <Reveal text="Rewind to any *moment.*" at={0} size={76} exitAt={swap - 12} />
      </div>
      <div style={{ position: "absolute", left: 120, top: 112 }}>
        <Reveal text="Screens included, next to what was *said.*" at={swap} size={76} />
      </div>
      <div
        style={{
          position: "absolute",
          left: 960 - 700,
          top: 236,
          perspective: 2200,
        }}
      >
        <div
          style={{
            transform: `rotateX(${tilt}deg) scale(${(0.9 + 0.1 * land) * push}) translateY(${(1 - land) * 140}px)`,
            transformOrigin: "50% 0%",
            opacity: Math.min(1, land * 1.6),
            position: "relative",
          }}
        >
          <Screen clip={CLIPS.macRewind.clip} from={CLIPS.macRewind.from} kind="mac" width={1400} glow={0.8} title="noFriction Meetings" />
          <div
            style={{
              position: "absolute",
              inset: 0,
              pointerEvents: "none",
              background: `linear-gradient(105deg, transparent ${streak * 100 - 12}%, rgba(250,204,21,0.16) ${streak * 100}%, transparent ${streak * 100 + 12}%)`,
              mixBlendMode: "screen",
            }}
          />
        </div>
      </div>
    </AbsoluteFill>
  );
};
