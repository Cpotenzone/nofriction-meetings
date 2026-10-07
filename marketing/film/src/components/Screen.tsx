import React from "react";
import { Freeze, Img, interpolate, OffthreadVideo, staticFile, useCurrentFrame, useVideoConfig } from "remotion";
import manifest from "../footage.json";
import { C, F, easeInOut } from "../theme";

export type Kind = "phone" | "mac" | "watch" | "plain";

type Entry = { w: number; h: number; fps: number; dur: number };
const FOOTAGE = manifest as Record<string, Entry>;

const FALLBACK_ASPECT: Record<Kind, number> = {
  phone: 1320 / 2868,
  mac: 16 / 9,
  watch: 416 / 496,
  plain: 16 / 9,
};

export function footageAspect(clip: string, kind: Kind): number {
  const e = FOOTAGE[clip];
  return e ? e.w / e.h : FALLBACK_ASPECT[kind];
}

export function footageSeconds(clip: string): number {
  return FOOTAGE[clip]?.dur ?? 0;
}

/**
 * Real app footage (simulator / browser capture), playing from `from`
 * seconds at `rate`, holding its last frame when it runs out.
 * `clip` is relative to public/footage, e.g. "ios/ios-02-live.mp4"; a .png
 * shows a still.
 */
export type Zoom = { scale: number; x: number; y: number; at?: number; dur?: number };

export const Footage: React.FC<{
  clip: string;
  from?: number;
  rate?: number;
  style?: React.CSSProperties;
  /** push in toward (x, y) in 0..1 of the frame, from `at` over `dur` frames */
  zoom?: Zoom;
}> = ({ clip, from = 0, rate = 1, style, zoom }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const entry = FOOTAGE[clip];
  const fill: React.CSSProperties = { width: "100%", height: "100%", objectFit: "cover", display: "block", ...style };
  if (!entry) {
    return (
      <div style={{ ...fill, background: C.well, color: C.text3, fontFamily: F.mono, fontSize: 18, display: "flex", alignItems: "center", justifyContent: "center", textAlign: "center", padding: 20 }}>
        missing footage
        <br />
        {clip}
      </div>
    );
  }
  if (clip.endsWith(".png") || clip.endsWith(".jpg")) {
    return <Img src={staticFile(`footage/${clip}`)} style={fill} />;
  }
  const startFrame = Math.round(from * fps);
  // last safe local frame of this clip, from `from`, at `rate`
  const last = Math.max(0, Math.floor(((entry.dur - from) * fps - 2) / rate));
  const video = <OffthreadVideo src={staticFile(`footage/${clip}`)} trimBefore={startFrame} playbackRate={rate} muted style={fill} />;
  const body = frame >= last ? <Freeze frame={last}>{video}</Freeze> : video;
  if (!zoom) return body;
  const z = interpolate(frame, [zoom.at ?? 0, (zoom.at ?? 0) + (zoom.dur ?? 45)], [1, zoom.scale], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: easeInOut,
  });
  return (
    <div style={{ width: "100%", height: "100%", overflow: "hidden" }}>
      <div style={{ width: "100%", height: "100%", transform: `scale(${z})`, transformOrigin: `${zoom.x * 100}% ${zoom.y * 100}%` }}>{body}</div>
    </div>
  );
};

/**
 * Footage in a simple device-agnostic frame: a rounded screen with a thin
 * dark bezel (no Apple product artwork). Width in px; height follows the clip.
 */
export const Screen: React.FC<{
  clip: string;
  kind: Kind;
  width: number;
  from?: number;
  rate?: number;
  style?: React.CSSProperties;
  glow?: number;
  title?: string;
  zoom?: Zoom;
}> = ({ clip, kind, width, from, rate, style, glow = 0, title, zoom }) => {
  const aspect = footageAspect(clip, kind);
  const screenH = width / aspect;
  const shadow = `0 ${width * 0.05}px ${width * 0.16}px rgba(0,0,0,0.65), 0 0 0 1px rgba(255,255,255,0.07)${
    glow > 0 ? `, 0 0 ${width * 0.22}px rgba(250,204,21,${0.16 * glow})` : ""
  }`;
  if (kind === "mac") {
    const bar = Math.max(18, width * 0.024);
    const r = Math.max(8, width * 0.0085);
    return (
      <div style={{ width, borderRadius: r, overflow: "hidden", background: "#111", boxShadow: shadow, ...style }}>
        <div style={{ height: bar, background: "#161616", borderBottom: "1px solid #232323", display: "flex", alignItems: "center", paddingLeft: bar * 0.6, gap: bar * 0.32, position: "relative" }}>
          {[0, 1, 2].map((i) => (
            <div key={i} style={{ width: bar * 0.36, height: bar * 0.36, borderRadius: "50%", background: "#3a3a3a" }} />
          ))}
          {title && (
            <div style={{ position: "absolute", left: 0, right: 0, textAlign: "center", fontFamily: F.sans, fontWeight: 500, fontSize: bar * 0.48, color: "#8a8a8a" }}>{title}</div>
          )}
        </div>
        <div style={{ width, height: screenH }}>
          <Footage clip={clip} from={from} rate={rate} zoom={zoom} />
        </div>
      </div>
    );
  }
  if (kind === "plain") {
    return (
      <div style={{ width, height: screenH, overflow: "hidden", ...style }}>
        <Footage clip={clip} from={from} rate={rate} zoom={zoom} />
      </div>
    );
  }
  const bezel = kind === "phone" ? width * 0.028 : width * 0.075;
  const radius = kind === "phone" ? width * 0.135 : width * 0.25;
  return (
    <div
      style={{
        width: width + bezel * 2,
        padding: bezel,
        borderRadius: radius + bezel,
        background: "linear-gradient(160deg, #1d1d1f 0%, #0b0b0c 40%, #121214 100%)",
        boxShadow: shadow,
        ...style,
      }}
    >
      <div style={{ width, height: screenH, borderRadius: radius, overflow: "hidden", background: "#000" }}>
        <Footage clip={clip} from={from} rate={rate} zoom={zoom} />
      </div>
    </div>
  );
};
