import React from "react";
import { interpolate, useCurrentFrame } from "remotion";
import { C, F, easeIn, easeOut } from "../theme";

type Props = {
  /** Words; "\n" starts a new line. Wrap a word in *stars* to color it yellow. */
  text: string;
  at?: number;
  stagger?: number;
  dur?: number;
  /** Frame (local) where the words slide out; omit to stay. */
  exitAt?: number;
  size?: number;
  weight?: number;
  color?: string;
  align?: "left" | "center" | "right";
  lineHeight?: number;
  tracking?: string;
  style?: React.CSSProperties;
  font?: "sans" | "mono";
};

/** Kinetic type: each word rises out of a mask, one after another. */
export const Reveal: React.FC<Props> = ({
  text,
  at = 0,
  stagger = 3,
  dur = 22,
  exitAt,
  size = 120,
  weight = 700,
  color = C.text,
  align = "left",
  lineHeight = 1.04,
  tracking = "-0.035em",
  style,
  font = "sans",
}) => {
  const frame = useCurrentFrame();
  const lines = text.split("\n").map((l) => l.split(" ").filter(Boolean));
  let index = 0;
  return (
    <div
      style={{
        fontFamily: font === "mono" ? F.mono : F.sans,
        fontSize: size,
        fontWeight: weight,
        color,
        letterSpacing: tracking,
        lineHeight,
        textAlign: align,
        ...style,
      }}
    >
      {lines.map((words, li) => (
        <div key={li} style={{ display: "block", whiteSpace: "nowrap" }}>
          {words.map((raw, wi) => {
            const i = index++;
            const accent = raw.startsWith("*") && raw.replace(/[.,:;!?]$/, "").endsWith("*");
            const word = raw.replace(/\*/g, "");
            const start = at + i * stagger;
            const p = interpolate(frame, [start, start + dur], [0, 1], {
              extrapolateLeft: "clamp",
              extrapolateRight: "clamp",
              easing: easeOut,
            });
            const out =
              exitAt === undefined
                ? 0
                : interpolate(frame, [exitAt + i * 1.5, exitAt + i * 1.5 + 12], [0, 1], {
                    extrapolateLeft: "clamp",
                    extrapolateRight: "clamp",
                    easing: easeIn,
                  });
            return (
              <span
                key={wi}
                style={{
                  display: "inline-block",
                  overflow: "hidden",
                  verticalAlign: "top",
                  paddingBottom: "0.12em",
                  marginBottom: "-0.12em",
                  marginRight: wi < words.length - 1 ? "0.25em" : 0,
                }}
              >
                <span
                  style={{
                    display: "inline-block",
                    transform: `translateY(${(1 - p) * 105 - out * 105}%)`,
                    opacity: Math.min(1, p * 1.6) * (1 - out),
                    color: accent ? C.yellow : undefined,
                  }}
                >
                  {word}
                </span>
              </span>
            );
          })}
        </div>
      ))}
    </div>
  );
};

/** Small uppercase mono label, the app's HUD style. */
export const Label: React.FC<{
  children: React.ReactNode;
  at?: number;
  color?: string;
  size?: number;
  style?: React.CSSProperties;
}> = ({ children, at = 0, color = C.yellow, size = 20, style }) => {
  const frame = useCurrentFrame();
  const p = interpolate(frame, [at, at + 16], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeOut });
  return (
    <div
      style={{
        fontFamily: F.mono,
        fontSize: size,
        fontWeight: 500,
        letterSpacing: "0.2em",
        textTransform: "uppercase",
        color,
        opacity: p,
        transform: `translateY(${(1 - p) * 10}px)`,
        ...style,
      }}
    >
      {children}
    </div>
  );
};
