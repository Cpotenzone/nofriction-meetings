import React from "react";
import { AbsoluteFill, interpolate, useCurrentFrame } from "remotion";
import { CLIPS } from "../clips";
import { LockIcon, ShieldIcon, WifiOffIcon } from "../components/Icons";
import { Backdrop, Chip } from "../components/Motifs";
import { Reveal } from "../components/Reveal";
import { Screen } from "../components/Screen";
import { C, easeInOut, easeOut } from "../theme";

// 7. Works offline. Nothing leaves your device unless you want it to.
// 216 frames (3 bars), over the Mac home screen's own privacy promise.
export const S7Offline: React.FC = () => {
  const frame = useCurrentFrame();
  const dim = interpolate(frame, [26, 60], [1, 0.16], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeInOut });
  const zoom = interpolate(frame, [0, 216], [1.0, 1.12], { easing: easeInOut });
  const blur = interpolate(frame, [26, 60], [0, 6], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const lock = interpolate(frame, [36, 76], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeOut });
  return (
    <AbsoluteFill>
      <Backdrop glowY={45} glow={0.8} />
      <div style={{ position: "absolute", left: 960 - 760, top: 110, opacity: dim, filter: blur > 0.1 ? `blur(${blur}px)` : undefined, transform: `scale(${zoom})`, transformOrigin: "50% 45%" }}>
        <Screen clip={CLIPS.macHome.clip} from={CLIPS.macHome.from} kind="mac" width={1520} />
      </div>
      <div style={{ position: "absolute", left: 0, right: 0, top: 170, display: "flex", justifyContent: "center", color: C.yellow, opacity: lock, transform: `scale(${0.8 + 0.2 * lock})` }}>
        <div style={{ width: 132, height: 132, borderRadius: 36, background: "rgba(250,204,21,0.1)", border: "1px solid rgba(250,204,21,0.3)", display: "flex", alignItems: "center", justifyContent: "center", boxShadow: "0 0 80px rgba(250,204,21,0.2)" }}>
          <LockIcon size={76} stroke={1.6} draw={lock} />
        </div>
      </div>
      <div style={{ position: "absolute", left: 0, right: 0, top: 350 }}>
        <Reveal text="Works *offline.*" at={44} size={124} align="center" />
        <Reveal text={"Nothing leaves your device\nunless you want it to."} at={74} stagger={3} size={64} weight={600} color={C.text2} align="center" lineHeight={1.16} style={{ marginTop: 28 }} />
      </div>
      <div style={{ position: "absolute", left: 0, right: 0, top: 800, display: "flex", justifyContent: "center", gap: 18 }}>
        <Chip at={118} tone="yellow" icon={<ShieldIcon size={30} color="#0a0a0a" />}>Data Not Collected</Chip>
        <Chip at={128} icon={<LockIcon size={30} />}>No account</Chip>
        <Chip at={138} icon={<WifiOffIcon size={30} />}>No noFriction servers</Chip>
      </div>
    </AbsoluteFill>
  );
};
