import React from "react";
import { AbsoluteFill, Html5Audio, interpolate, Sequence, staticFile, useCurrentFrame, useVideoConfig } from "remotion";
import { Footage, Zoom } from "../components/Screen";
import { Reveal } from "../components/Reveal";
import { C, F, easeOut } from "../theme";

// App Store app previews. Apple 2.3.4: previews may only use screen
// captures of the app itself, with text overlays allowed. So every frame
// here is real app footage plus a caption: no fake UI, no prices, no device
// artwork, nothing outside the app.

export type Shot = {
  clip: string;
  from?: number;
  rate?: number;
  /** frames */
  dur: number;
  /** omit to keep the previous shot's caption on screen */
  caption?: string;
  label?: string;
  /** a gentle push toward a point (0..1 of the frame) so small UI reads */
  zoom?: Zoom;
};

const CROSS = 8;

/** One shot: the footage, with a short cross-dissolve in. */
const ShotView: React.FC<{ shot: Shot; first: boolean }> = ({ shot, first }) => {
  const frame = useCurrentFrame();
  const o = first ? 1 : interpolate(frame, [0, CROSS], [0, 1], { extrapolateRight: "clamp", easing: easeOut });
  const s = first ? 1 : interpolate(frame, [0, CROSS * 2], [1.015, 1], { extrapolateRight: "clamp", easing: easeOut });
  return (
    <AbsoluteFill style={{ opacity: o, transform: `scale(${s})` }}>
      <Footage clip={shot.clip} from={shot.from} rate={shot.rate} zoom={shot.zoom} />
    </AbsoluteFill>
  );
};

const Caption: React.FC<{ text: string; label?: string; size: number; exitAt?: number; labelSize: number }> = ({ text, label, size, exitAt, labelSize }) => {
  const frame = useCurrentFrame();
  const lo = interpolate(frame, [0, 12], [0, 1], { extrapolateRight: "clamp" }) * (exitAt === undefined ? 1 : interpolate(frame, [exitAt, exitAt + 8], [1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: labelSize * 0.9 }}>
      {label && (
        <div style={{ fontFamily: F.mono, fontSize: labelSize, letterSpacing: "0.22em", textTransform: "uppercase", color: C.yellow, opacity: lo }}>{label}</div>
      )}
      <Reveal text={text} at={2} stagger={2} dur={18} size={size} align="center" exitAt={exitAt} lineHeight={1.08} />
    </div>
  );
};

/**
 * Layout: caption band on top, the app's screen below at its native
 * aspect (slightly scaled down; a shot may push in gently where the UI is small).
 */
export const Preview: React.FC<{
  shots: Shot[];
  screen: { left: number; top: number; width: number; height: number; radius: number };
  captionTop: number;
  captionSize: number;
  labelSize: number;
  music: boolean;
}> = ({ shots, screen, captionTop, captionSize, labelSize, music }) => {
  const { durationInFrames } = useVideoConfig();
  let t = 0;
  const placed = shots.map((s) => {
    const from = t;
    t += s.dur;
    return { s, from };
  });
  // a shot without a caption keeps the previous one on screen
  const groups: { from: number; dur: number; caption: string; label?: string }[] = [];
  for (const { s, from } of placed) {
    if (s.caption === undefined && groups.length) groups[groups.length - 1].dur += s.dur;
    else groups.push({ from, dur: s.dur, caption: s.caption ?? "", label: s.label });
  }
  return (
    <AbsoluteFill style={{ background: "#09090a" }}>
      <AbsoluteFill style={{ background: "radial-gradient(ellipse 70% 45% at 50% 55%, rgba(250,204,21,0.07), transparent 70%)" }} />
      <div
        style={{
          position: "absolute",
          left: screen.left,
          top: screen.top,
          width: screen.width,
          height: screen.height,
          borderRadius: screen.radius,
          overflow: "hidden",
          background: "#000",
          boxShadow: "0 30px 90px rgba(0,0,0,0.7), 0 0 0 1px rgba(255,255,255,0.08)",
        }}
      >
        {placed.map(({ s, from }, i) => (
          <Sequence key={i} from={from} durationInFrames={i === placed.length - 1 ? durationInFrames - from : s.dur + CROSS}>
            <ShotView shot={s} first={i === 0} />
          </Sequence>
        ))}
      </div>
      {groups.map((g, i) => (
        <Sequence key={`c${i}`} from={g.from} durationInFrames={g.dur}>
          <div style={{ position: "absolute", left: 0, right: 0, top: captionTop }}>
            <Caption text={g.caption} label={g.label} size={captionSize} labelSize={labelSize} exitAt={i < groups.length - 1 ? g.dur - 9 : undefined} />
          </div>
        </Sequence>
      ))}
      {music && <Html5Audio src={staticFile("audio/score-preview.wav")} />}
    </AbsoluteFill>
  );
};
