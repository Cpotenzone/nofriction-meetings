import React from "react";
import { AbsoluteFill, interpolate, random, spring, useCurrentFrame, useVideoConfig } from "remotion";
import { C, F, easeOut } from "../theme";

/** Matte black with a faint HUD grid, vignette and a soft yellow bloom. */
export const Backdrop: React.FC<{ glowX?: number; glowY?: number; glow?: number; grid?: number }> = ({
  glowX = 50,
  glowY = 50,
  glow = 0.6,
  grid = 1,
}) => {
  const frame = useCurrentFrame();
  const drift = Math.sin(frame / 90) * 1.5;
  return (
    <AbsoluteFill style={{ background: C.bg }}>
      <AbsoluteFill
        style={{
          opacity: 0.55 * grid,
          backgroundImage:
            "linear-gradient(rgba(255,255,255,0.028) 1px, transparent 1px), linear-gradient(90deg, rgba(255,255,255,0.028) 1px, transparent 1px)",
          backgroundSize: "64px 64px",
          backgroundPosition: `${drift}px ${frame * 0.25}px`,
          maskImage: "radial-gradient(ellipse 70% 65% at 50% 50%, black 30%, transparent 100%)",
          WebkitMaskImage: "radial-gradient(ellipse 70% 65% at 50% 50%, black 30%, transparent 100%)",
        }}
      />
      <AbsoluteFill
        style={{
          background: `radial-gradient(ellipse 45% 40% at ${glowX + drift}% ${glowY}%, rgba(250,204,21,${0.11 * glow}) 0%, rgba(250,204,21,0) 70%)`,
        }}
      />
      <AbsoluteFill style={{ background: "radial-gradient(ellipse 85% 80% at 50% 50%, transparent 55%, rgba(0,0,0,0.75) 100%)" }} />
    </AbsoluteFill>
  );
};

/** Pill with an icon, like the app's chips and mark buttons. */
export const Chip: React.FC<{
  icon?: React.ReactNode;
  children: React.ReactNode;
  at?: number;
  tone?: "yellow" | "dark" | "outline";
  size?: number;
  style?: React.CSSProperties;
}> = ({ icon, children, at = 0, tone = "dark", size = 30, style }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const s = spring({ frame: frame - at, fps, config: { damping: 14, stiffness: 170, mass: 0.7 } });
  const bg = tone === "yellow" ? C.yellow : tone === "dark" ? "rgba(23,23,23,0.92)" : "transparent";
  const fg = tone === "yellow" ? "#0a0a0a" : C.text;
  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: size * 0.4,
        padding: `${size * 0.42}px ${size * 0.72}px`,
        borderRadius: 999,
        background: bg,
        border: tone === "yellow" ? "none" : `1px solid ${tone === "outline" ? C.borderBright : C.border}`,
        color: fg,
        fontFamily: F.sans,
        fontWeight: 600,
        fontSize: size,
        letterSpacing: "-0.01em",
        transform: `translateY(${(1 - s) * 24}px) scale(${0.85 + 0.15 * s})`,
        opacity: Math.min(1, s * 1.4),
        boxShadow: tone === "yellow" ? "0 10px 40px rgba(250,204,21,0.25)" : "0 10px 30px rgba(0,0,0,0.45)",
        whiteSpace: "nowrap",
        ...style,
      }}
    >
      {icon && <span style={{ display: "inline-flex", color: tone === "yellow" ? "#0a0a0a" : C.yellow }}>{icon}</span>}
      {children}
    </div>
  );
};

/** Live audio bars: deterministic pseudo-speech envelope. */
export const Waveform: React.FC<{
  bars?: number;
  width: number;
  height: number;
  color?: string;
  seed?: string;
  active?: number;
  style?: React.CSSProperties;
}> = ({ bars = 28, width, height, color = C.yellow, seed = "w", active = 1, style }) => {
  const frame = useCurrentFrame();
  const gap = width / bars;
  return (
    <div style={{ width, height, display: "flex", alignItems: "center", gap: gap * 0.35, ...style }}>
      {new Array(bars).fill(0).map((_, i) => {
        const syll = 0.5 + 0.5 * Math.sin(frame / 5.5 + random(`${seed}-p-${i}`) * 6.28);
        const word = 0.55 + 0.45 * Math.sin(frame / 17 + i * 0.35);
        const env = Math.sin((Math.PI * (i + 0.5)) / bars) ** 0.6;
        const h = Math.max(0.08, (0.15 + 0.85 * syll * word) * env * active + random(`${seed}-${i}-${Math.floor(frame / 3)}`) * 0.12 * active);
        return <div key={i} style={{ flex: 1, height: `${h * 100}%`, borderRadius: 99, background: color, opacity: 0.35 + 0.65 * h }} />;
      })}
    </div>
  );
};

/** The record button: a red dot inside a ring, as in the apps. */
export const RecordButton: React.FC<{ size: number; pressed?: number; pulse?: boolean }> = ({ size, pressed = 0, pulse = true }) => {
  const frame = useCurrentFrame();
  const ring = size * 0.06;
  const inner = interpolate(pressed, [0, 1], [size * 0.78, size * 0.42]);
  const radius = interpolate(pressed, [0, 1], [inner / 2, inner * 0.18]);
  const waves = pulse ? [0, 1, 2] : [];
  return (
    <div style={{ width: size, height: size, position: "relative" }}>
      {waves.map((w) => {
        const t = ((frame + w * 20) % 60) / 60;
        return (
          <div
            key={w}
            style={{
              position: "absolute",
              inset: 0,
              borderRadius: "50%",
              border: `2px solid rgba(240,69,69,${0.5 * (1 - t) * pressed})`,
              transform: `scale(${1 + t * 0.9})`,
            }}
          />
        );
      })}
      <div style={{ position: "absolute", inset: 0, borderRadius: "50%", border: `${ring}px solid rgba(255,255,255,0.92)` }} />
      <div
        style={{
          position: "absolute",
          left: (size - inner) / 2,
          top: (size - inner) / 2,
          width: inner,
          height: inner,
          borderRadius: radius,
          background: C.recRed,
          boxShadow: `0 0 ${size * 0.5}px rgba(240,69,69,${0.25 + 0.3 * pressed})`,
        }}
      />
    </div>
  );
};

const pad = (n: number) => String(Math.floor(n)).padStart(2, "0");
export const clock = (sec: number) => `${pad(sec / 3600)}:${pad((sec / 60) % 60)}:${pad(sec % 60)}`;

/**
 * The timeline motif (Rewind's scrubber): ticks, timecodes, a playhead.
 * `t` is the playhead time in seconds; the strip scrolls under a fixed head.
 */
export const Timeline: React.FC<{
  width: number;
  t: number;
  pxPerSec?: number;
  draw?: number;
  marks?: { at: number; color?: string; icon?: React.ReactNode; label?: string; appear: number }[];
  showHead?: boolean;
  style?: React.CSSProperties;
}> = ({ width, t, pxPerSec = 6, draw = 1, marks = [], showHead = true, style }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const half = width / 2;
  const span = half / pxPerSec;
  const ticks: number[] = [];
  const step = 10; // a tick every 10 s, a label every 60 s
  for (let s = Math.floor((t - span) / step) * step; s <= t + span; s += step) if (s >= 0) ticks.push(s);
  return (
    <div style={{ width, height: 160, position: "relative", ...style }}>
      <div
        style={{
          position: "absolute",
          top: 80,
          left: half - half * draw,
          width: width * draw,
          height: 2,
          background: `linear-gradient(90deg, transparent, ${C.borderBright} 12%, ${C.borderBright} 88%, transparent)`,
        }}
      />
      {ticks.map((s) => {
        const x = half + (s - t) * pxPerSec;
        const major = s % 60 === 0;
        const fade = Math.max(0, 1 - Math.abs(x - half) / half) * draw;
        return (
          <div key={s} style={{ position: "absolute", left: x, top: major ? 66 : 73, opacity: fade }}>
            <div style={{ width: 2, height: major ? 30 : 16, background: major ? "#5a5a5a" : "#333", marginLeft: -1 }} />
            {major && (
              <div style={{ position: "absolute", top: 40, left: -60, width: 120, textAlign: "center", fontFamily: F.mono, fontSize: 17, color: C.text3, letterSpacing: "0.05em" }}>
                {clock(s)}
              </div>
            )}
          </div>
        );
      })}
      {marks.map((m, i) => {
        const x = half + (m.at - t) * pxPerSec;
        const s = spring({ frame: frame - m.appear, fps, config: { damping: 12, stiffness: 180, mass: 0.6 } });
        if (frame < m.appear) return null;
        const fade = Math.max(0, 1 - Math.abs(x - half) / (half * 1.05));
        return (
          <div key={i} style={{ position: "absolute", left: x, top: 0, opacity: fade, transform: `translateY(${(1 - s) * -60}px)` }}>
            <div
              style={{
                position: "absolute",
                left: -26,
                top: 4,
                width: 52,
                height: 52,
                borderRadius: 16,
                background: m.color ?? C.yellow,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: "#0a0a0a",
                boxShadow: `0 8px 30px ${m.color ?? C.yellow}55`,
                transform: `scale(${s})`,
              }}
            >
              {m.icon}
            </div>
            <div style={{ position: "absolute", left: -1, top: 58, width: 2, height: 24, background: m.color ?? C.yellow, opacity: s }} />
          </div>
        );
      })}
      {showHead && (
        <div style={{ position: "absolute", left: half - 1, top: 40, opacity: draw }}>
          <div style={{ width: 2, height: 84, background: C.yellow, boxShadow: `0 0 18px ${C.yellow}` }} />
          <div style={{ position: "absolute", top: -8, left: -7, width: 16, height: 16, borderRadius: 4, background: C.yellow, transform: "rotate(45deg)" }} />
        </div>
      )}
    </div>
  );
};

/** Fade/slide wrapper for whole elements. */
export const Enter: React.FC<{
  at?: number;
  dur?: number;
  from?: { x?: number; y?: number; scale?: number; blur?: number };
  exitAt?: number;
  exitDur?: number;
  exitTo?: { x?: number; y?: number; scale?: number };
  style?: React.CSSProperties;
  children: React.ReactNode;
}> = ({ at = 0, dur = 24, from = { y: 40 }, exitAt, exitDur = 14, exitTo = { y: -30 }, style, children }) => {
  const frame = useCurrentFrame();
  const p = interpolate(frame, [at, at + dur], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeOut });
  const q = exitAt === undefined ? 0 : interpolate(frame, [exitAt, exitAt + exitDur], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: (x) => x * x });
  const x = (1 - p) * (from.x ?? 0) + q * (exitTo.x ?? 0);
  const y = (1 - p) * (from.y ?? 0) + q * (exitTo.y ?? 0);
  const sc = (1 - p) * (from.scale ?? 1) + p * 1 + q * ((exitTo.scale ?? 1) - 1);
  const blur = (1 - p) * (from.blur ?? 0);
  return (
    <div style={{ transform: `translate(${x}px, ${y}px) scale(${sc})`, opacity: p * (1 - q), filter: blur > 0.05 ? `blur(${blur}px)` : undefined, ...style }}>
      {children}
    </div>
  );
};
