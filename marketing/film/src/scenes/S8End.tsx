import React from "react";
import { AbsoluteFill, Img, interpolate, spring, staticFile, useCurrentFrame, useVideoConfig } from "remotion";
import { Backdrop } from "../components/Motifs";
import { Reveal } from "../components/Reveal";
import { C, F, easeOut } from "../theme";

// 8. End card: noFriction: Record your Life · nofriction.io.
// 216 frames (3 bars); `fadeOut` darkens the last frames so the website
// loop restarts from black.
export const EndCard: React.FC<{ fadeOut?: boolean; compact?: boolean }> = ({ fadeOut = true, compact = false }) => {
  const frame = useCurrentFrame();
  const { fps, durationInFrames, width } = useVideoConfig();
  const pop = spring({ frame, fps, config: { damping: 12, stiffness: 110, mass: 0.9 } });
  const ring = interpolate(frame, [0, 40], [0, 1], { extrapolateRight: "clamp", easing: easeOut });
  const url = interpolate(frame, [46, 66], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeOut });
  const end = fadeOut ? interpolate(frame, [durationInFrames - 16, durationInFrames - 1], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }) : 0;
  const k = compact ? width / 1920 : 1;
  const icon = 220 * (compact ? 1.5 * k : 1);
  return (
    <AbsoluteFill>
      <Backdrop glowY={compact ? 40 : 34} glow={1.4} />
      <AbsoluteFill style={{ alignItems: "center", justifyContent: "center", flexDirection: "column" }}>
        <div style={{ position: "relative", width: icon, height: icon, marginTop: compact ? -80 : -40 }}>
          <div
            style={{
              position: "absolute",
              inset: -icon * 0.5 * ring,
              borderRadius: "50%",
              border: `2px solid rgba(250,204,21,${0.45 * (1 - ring)})`,
            }}
          />
          <Img
            src={staticFile("app-icon.png")}
            style={{
              width: icon,
              height: icon,
              borderRadius: icon * 0.225,
              transform: `scale(${0.55 + 0.45 * pop}) rotate(${(1 - pop) * -40}deg)`,
              opacity: Math.min(1, pop * 1.8),
              boxShadow: `0 30px 90px rgba(0,0,0,0.7), 0 0 120px rgba(250,204,21,${0.22 * pop}), 0 0 0 1px rgba(255,255,255,0.08)`,
            }}
          />
        </div>
        <div style={{ marginTop: 54 * (compact ? 1.4 * k : 1) }}>
          <Reveal text="noFriction" at={10} size={compact ? 150 * k * 1.4 : 150} weight={800} align="center" tracking="-0.045em" />
        </div>
        <div style={{ marginTop: 6 }}>
          <Reveal text="Record your *Life*" at={20} stagger={4} size={compact ? 66 * k * 1.5 : 66} weight={700} color={C.text} align="center" tracking="-0.02em" />
        </div>
        <div style={{ marginTop: 22 }}>
          <Reveal text="Transcribe, summarize, rewind" at={32} stagger={3} size={compact ? 34 * k * 1.6 : 34} weight={500} color={C.text2} align="center" tracking="0" />
        </div>
        <div
          style={{
            marginTop: 52,
            opacity: url,
            transform: `translateY(${(1 - url) * 16}px)`,
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            gap: 22,
          }}
        >
          <div style={{ fontFamily: F.mono, fontWeight: 500, fontSize: compact ? 30 * k * 1.6 : 32, color: C.yellow, padding: "14px 34px", border: "1px solid rgba(250,204,21,0.45)", borderRadius: 999, background: "rgba(250,204,21,0.06)", letterSpacing: "0.04em" }}>
            nofriction.io
          </div>
          <div style={{ fontFamily: F.mono, fontSize: compact ? 16 * k * 1.6 : 18, letterSpacing: "0.24em", color: C.text3, textTransform: "uppercase" }}>
            iPhone · iPad · Mac · Apple Watch
          </div>
        </div>
      </AbsoluteFill>
      <AbsoluteFill style={{ background: "#000", opacity: end }} />
    </AbsoluteFill>
  );
};

export const S8End: React.FC = () => <EndCard />;
