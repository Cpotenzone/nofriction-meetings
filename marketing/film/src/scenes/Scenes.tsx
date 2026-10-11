import React from "react";
import { AbsoluteFill, interpolate, Sequence, spring, useCurrentFrame, useVideoConfig } from "remotion";
import { CLIPS } from "../clips";
import { LockIcon, RewindIcon, ShieldIcon, WifiOffIcon } from "../components/Icons";
import { Backdrop, Chip, Enter } from "../components/Motifs";
import { Label, Reveal } from "../components/Reveal";
import { Screen } from "../components/Screen";
import { C, F, easeInOut, easeOut } from "../theme";

// The hero film's scenes, on the bar grid (timing.ts). Every screen is the
// real app (clips.ts): the Mac UI filmed in the harness, iPhone and Apple
// Watch as their App Store screenshots. Words in *stars* are yellow.

/** The scene's small label and headline, top left. A second headline can
 *  replace the first at `swap`. */
const Head: React.FC<{ label: string; text: string; swap?: number; then?: string; size?: number }> = ({ label, text, swap, then, size = 72 }) => (
  <>
    <div style={{ position: "absolute", left: 120, top: 58 }}>
      <Label at={0} size={22}>
        {label}
      </Label>
    </div>
    <div style={{ position: "absolute", left: 120, top: 100 }}>
      <Reveal text={text} at={2} size={size} exitAt={swap !== undefined ? swap - 12 : undefined} />
    </div>
    {swap !== undefined && then && (
      <div style={{ position: "absolute", left: 120, top: 100 }}>
        <Reveal text={then} at={swap} size={size} />
      </div>
    )}
  </>
);

/** A big Mac window landing from below, with a slow push. */
const MacShot: React.FC<{ use: { clip: string; from: number; rate?: number; zoom?: any }; left: number; top: number; width: number; glow?: number; push?: number; streak?: boolean }> = ({
  use,
  left,
  top,
  width,
  glow = 0.6,
  push = 1.04,
  streak = false,
}) => {
  const frame = useCurrentFrame();
  const { fps, durationInFrames } = useVideoConfig();
  const land = spring({ frame: frame + 4, fps, config: { damping: 20, stiffness: 90, mass: 1 } });
  const p = interpolate(frame, [10, durationInFrames], [1, push], { easing: easeInOut, extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const s = interpolate(frame, [0, 22], [-0.3, 1.3], { extrapolateRight: "clamp" });
  return (
    <div style={{ position: "absolute", left, top, perspective: 2200 }}>
      <div
        style={{
          transform: `rotateX(${interpolate(land, [0, 1], [12, 0])}deg) scale(${(0.92 + 0.08 * land) * p}) translateY(${(1 - land) * 120}px)`,
          transformOrigin: "50% 0%",
          opacity: Math.min(1, land * 1.6),
          position: "relative",
        }}
      >
        <Screen clip={use.clip} from={use.from} rate={use.rate} zoom={use.zoom} kind="mac" width={width} glow={glow} title="noFriction" />
        {streak && (
          <div
            style={{
              position: "absolute",
              inset: 0,
              pointerEvents: "none",
              background: `linear-gradient(105deg, transparent ${s * 100 - 12}%, rgba(250,204,21,0.16) ${s * 100}%, transparent ${s * 100 + 12}%)`,
              mixBlendMode: "screen",
            }}
          />
        )}
      </div>
    </div>
  );
};

/** 1. Record (2 bars): the Record sheet on the Mac, with iPhone and Apple Watch beside it. */
export const SRecord: React.FC = () => (
  <AbsoluteFill>
    <Backdrop glowY={60} glow={0.9} />
    <Head label="Record" text="Tap *Record.*" swap={76} then="Meetings, classes and *life.*" />
    <MacShot use={CLIPS.macRecord} left={100} top={250} width={1240} />
    <div style={{ position: "absolute", left: 1410, top: 236 }}>
      <Enter at={10} dur={28} from={{ y: 140, scale: 0.94 }}>
        <Screen clip={CLIPS.phoneRecord.clip} kind="phone" width={290} glow={0.5} />
        <Label at={30} color={C.text3} size={18} style={{ marginTop: 18, textAlign: "center" }}>
          iPhone
        </Label>
      </Enter>
    </div>
    <div style={{ position: "absolute", left: 1730, top: 560 }}>
      <Enter at={18} dur={28} from={{ y: 140, scale: 0.94 }}>
        <Screen clip={CLIPS.watchRecord.clip} kind="watch" width={150} />
        <Label at={38} color={C.text3} size={18} style={{ marginTop: 18, textAlign: "center" }}>
          Apple Watch
        </Label>
      </Enter>
    </div>
  </AbsoluteFill>
);

/** 2. Mark (2 bars): while recording, Mark → On the test → a note; then Stop. */
export const SMark: React.FC = () => (
  <AbsoluteFill>
    <Backdrop glowX={70} glowY={40} glow={0.9} />
    <Head label="Mark" text="Mark what *matters.*" swap={100} then="Then *Stop.*" />
    <MacShot use={CLIPS.macMark} left={240} top={214} width={1440} push={1.02} />
  </AbsoluteFill>
);

/** 3. Rewind (2 bars): the signature shot, scrubbing a meeting's timeline. */
export const SRewind: React.FC = () => (
  <AbsoluteFill>
    <Backdrop glowY={62} glow={1.1} grid={0.6} />
    <div style={{ position: "absolute", left: 120, top: 58, display: "flex", alignItems: "center", gap: 14, color: C.yellow }}>
      <RewindIcon size={30} stroke={2} />
    </div>
    <div style={{ position: "absolute", left: 166, top: 58 }}>
      <Label at={0} size={22}>
        Rewind
      </Label>
    </div>
    <div style={{ position: "absolute", left: 120, top: 100 }}>
      <Reveal text="Rewind to any *moment.*" at={0} size={72} exitAt={64} />
    </div>
    <div style={{ position: "absolute", left: 120, top: 100 }}>
      <Reveal text="The screen, next to what was *said.*" at={76} size={72} />
    </div>
    <MacShot use={CLIPS.macRewind} left={240} top={214} width={1440} glow={0.9} push={1.05} streak />
  </AbsoluteFill>
);

/** 4. Notes and the review guide (2 bars): a meeting's notes, then a class's quiz. */
export const SNotes: React.FC = () => {
  const frame = useCurrentFrame();
  const swap = 66;
  const a = interpolate(frame, [swap, swap + 14], [1, 0], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <AbsoluteFill>
      <Backdrop glowX={60} glowY={55} glow={1} />
      <Head label="Notes · Review guide" text="Make *notes* and a review *guide.*" />
      <div style={{ opacity: a }}>
        <MacShot use={CLIPS.macNotes} left={240} top={214} width={1440} push={1.02} />
      </div>
      <Sequence from={swap} layout="none">
        <div style={{ position: "absolute", left: 240, top: 214 }}>
          <Enter at={0} dur={22} from={{ x: 80, scale: 0.98 }}>
            <Screen clip={CLIPS.macReview.clip} from={CLIPS.macReview.from} rate={CLIPS.macReview.rate} kind="mac" width={1440} glow={0.6} title="noFriction" />
          </Enter>
        </div>
      </Sequence>
      <div
        style={{
          position: "absolute",
          right: 120,
          top: 70,
          fontFamily: F.sans,
          fontSize: 22,
          fontWeight: 500,
          color: C.text3,
          textAlign: "right",
          opacity: interpolate(frame, [30, 46], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }),
        }}
      >
        With noFriction Pro
        <br />
        Apple on-device AI or your own endpoint
      </div>
    </AbsoluteFill>
  );
};

/** 5. Chat (2 bars): ask across recordings; the answer cites the moments. */
export const SChat: React.FC = () => (
  <AbsoluteFill>
    <Backdrop glowY={50} glow={0.9} />
    <Head label="Chat" text="Ask your *recordings.*" swap={84} then="Every answer cites the *moment.*" />
    <MacShot use={CLIPS.macChat} left={240} top={214} width={1440} push={1.02} />
  </AbsoluteFill>
);

/** 6. Sync (1 bar): pair the iPhone with the Mac; no server. */
export const SSync: React.FC = () => (
  <AbsoluteFill>
    <Backdrop glowX={70} glowY={55} glow={1} />
    <Head label="Sync · Pro" text="*Sync* iPhone and Mac." />
    <div style={{ position: "absolute", left: 120, top: 186 }}>
      <Reveal text="Directly on your Wi-Fi. No server." at={10} stagger={2} size={34} weight={500} color={C.text2} tracking="-0.01em" />
    </div>
    <MacShot use={CLIPS.macSync} left={120} top={268} width={1260} push={1.02} />
    <div style={{ position: "absolute", left: 1480, top: 230 }}>
      <Enter at={6} dur={24} from={{ x: 120, scale: 0.95 }}>
        <Screen clip={CLIPS.phoneRecordings.clip} kind="phone" width={330} glow={0.6} />
      </Enter>
    </div>
  </AbsoluteFill>
);

/** 7. Privacy (1 bar), over the idle Record view's own privacy promise. */
export const SPrivacy: React.FC = () => {
  const frame = useCurrentFrame();
  const dim = interpolate(frame, [4, 20], [1, 0.16], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeInOut });
  const blur = interpolate(frame, [4, 20], [0, 6], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  const zoom = interpolate(frame, [0, 72], [1.0, 1.06], { easing: easeInOut });
  const lock = interpolate(frame, [8, 28], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeOut });
  return (
    <AbsoluteFill>
      <Backdrop glowY={45} glow={0.8} />
      <div style={{ position: "absolute", left: 960 - 760, top: 110, opacity: dim, filter: blur > 0.1 ? `blur(${blur}px)` : undefined, transform: `scale(${zoom})`, transformOrigin: "50% 45%" }}>
        <Screen clip={CLIPS.macHome.clip} from={CLIPS.macHome.from} kind="mac" width={1520} />
      </div>
      <div style={{ position: "absolute", left: 0, right: 0, top: 150, display: "flex", justifyContent: "center", color: C.yellow, opacity: lock, transform: `scale(${0.8 + 0.2 * lock})` }}>
        <div style={{ width: 120, height: 120, borderRadius: 34, background: "rgba(250,204,21,0.1)", border: "1px solid rgba(250,204,21,0.3)", display: "flex", alignItems: "center", justifyContent: "center", boxShadow: "0 0 80px rgba(250,204,21,0.2)" }}>
          <LockIcon size={68} stroke={1.6} draw={lock} />
        </div>
      </div>
      <div style={{ position: "absolute", left: 0, right: 0, top: 320 }}>
        <Reveal text="Works *offline.*" at={6} stagger={2} size={116} align="center" />
        <Reveal text={"Nothing leaves your device\nunless you want it to."} at={16} stagger={2} dur={18} size={58} weight={600} color={C.text2} align="center" lineHeight={1.16} style={{ marginTop: 24 }} />
      </div>
      <div style={{ position: "absolute", left: 0, right: 0, top: 760, display: "flex", justifyContent: "center", gap: 18 }}>
        <Chip at={30} tone="yellow" icon={<ShieldIcon size={28} color="#0a0a0a" />} size={28}>
          Data Not Collected
        </Chip>
        <Chip at={36} icon={<LockIcon size={28} />} size={28}>
          No account
        </Chip>
        <Chip at={42} icon={<WifiOffIcon size={28} />} size={28}>
          No noFriction servers
        </Chip>
      </div>
    </AbsoluteFill>
  );
};
