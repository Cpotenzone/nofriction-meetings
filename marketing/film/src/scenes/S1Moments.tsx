import React from "react";
import { AbsoluteFill, interpolate, spring, useCurrentFrame, useVideoConfig } from "remotion";
import { BookIcon, HeartIcon, PeopleIcon } from "../components/Icons";
import { Backdrop, Waveform } from "../components/Motifs";
import { Label, Reveal } from "../components/Reveal";
import { C, F, easeIn, easeInOut, easeOut } from "../theme";

// 1. Life is full of moments: a meeting, a lecture, a doctor's visit.
// 288 frames (4 bars).

const MOMENTS = [
  { type: "Meeting", title: "A meeting.", when: "Tue · 10:00", icon: PeopleIcon, at: 92 },
  { type: "Class", title: "A lecture.", when: "Wed · 09:00", icon: BookIcon, at: 128 },
  { type: "Personal", title: "A doctor's visit.", when: "Thu · 15:30", icon: HeartIcon, at: 164 },
];

const Card: React.FC<{ m: (typeof MOMENTS)[number]; i: number }> = ({ m, i }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const s = spring({ frame: frame - m.at, fps, config: { damping: 16, stiffness: 120, mass: 0.8 } });
  // collapse into the line, then into the record dot (end of scene)
  const out = interpolate(frame, [246 + i * 3, 272], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeIn });
  const Icon = m.icon;
  const x = (i - 1) * 540;
  return (
    <div
      style={{
        position: "absolute",
        left: 960 + x * (1 - out) - 240,
        top: 470,
        width: 480,
        height: 300,
        borderRadius: 26,
        background: "linear-gradient(180deg, #121212 0%, #0b0b0b 100%)",
        border: `1px solid ${C.border}`,
        boxShadow: "0 30px 80px rgba(0,0,0,0.6)",
        padding: 34,
        transform: `translateY(${(1 - s) * 120}px) scale(${(0.9 + 0.1 * s) * (1 - out * 0.85)})`,
        opacity: Math.min(1, s * 1.5) * (1 - out),
        display: "flex",
        flexDirection: "column",
        justifyContent: "space-between",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
          <div style={{ width: 52, height: 52, borderRadius: 14, background: "rgba(250,204,21,0.12)", display: "flex", alignItems: "center", justifyContent: "center", color: C.yellow }}>
            <Icon size={30} />
          </div>
          <div style={{ fontFamily: F.mono, fontSize: 19, letterSpacing: "0.18em", textTransform: "uppercase", color: C.yellow }}>{m.type}</div>
        </div>
        <div style={{ fontFamily: F.mono, fontSize: 18, color: C.text3 }}>{m.when}</div>
      </div>
      <div style={{ fontFamily: F.sans, fontWeight: 700, fontSize: 50, letterSpacing: "-0.03em", color: C.text }}>{m.title}</div>
      <Waveform width={412} height={56} seed={m.type} active={interpolate(frame, [m.at, m.at + 20], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })} color={C.yellow} />
    </div>
  );
};

export const S1Moments: React.FC = () => {
  const frame = useCurrentFrame();
  // the hairline draws out from the center, then gathers into a dot
  const draw = interpolate(frame, [4, 46], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeOut });
  const gather = interpolate(frame, [252, 286], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeInOut });
  const lineW = 1500 * draw * (1 - gather);
  const dot = interpolate(frame, [262, 288], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeOut });
  const headOut = interpolate(frame, [240, 262], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeIn });
  return (
    <AbsoluteFill>
      <Backdrop glowY={58} glow={interpolate(frame, [0, 120], [0.2, 0.8], { extrapolateRight: "clamp" })} />
      <div style={{ position: "absolute", top: 250, left: 0, right: 0, display: "flex", justifyContent: "center", opacity: 1 - headOut, transform: `translateY(${-headOut * 40}px)` }}>
        <Label at={12} style={{ letterSpacing: "0.32em" }}>
          Every day
        </Label>
      </div>
      <div style={{ position: "absolute", top: 302, left: 0, right: 0, opacity: 1 - headOut, transform: `translateY(${-headOut * 40}px)` }}>
        <Reveal text="Life is full of *moments.*" at={18} stagger={4} size={112} align="center" />
      </div>
      {/* the line every moment sits on */}
      <div style={{ position: "absolute", top: 820, left: 960 - lineW / 2, width: lineW, height: 2, background: `linear-gradient(90deg, transparent, ${C.yellow} 20%, ${C.yellow} 80%, transparent)`, opacity: 0.8 }} />
      {MOMENTS.map((m, i) => {
        const s = interpolate(frame, [m.at, m.at + 14], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
        return (
          <div key={i} style={{ position: "absolute", left: 960 + (i - 1) * 540 * (1 - gather) - 7, top: 813, width: 16, height: 16, borderRadius: 8, background: C.yellow, opacity: s * (1 - gather), boxShadow: `0 0 20px ${C.yellow}` }} />
        );
      })}
      {MOMENTS.map((m, i) => (
        <Card key={m.type} m={m} i={i} />
      ))}
      {/* the dot that becomes the record button in scene 2 */}
      <div
        style={{
          position: "absolute",
          left: 960 - 30 * dot,
          top: 821 - 30 * dot - interpolate(dot, [0, 1], [0, 281]),
          width: 60 * dot,
          height: 60 * dot,
          borderRadius: "50%",
          background: C.recRed,
          opacity: dot,
          boxShadow: `0 0 60px rgba(240,69,69,0.5)`,
        }}
      />
    </AbsoluteFill>
  );
};
