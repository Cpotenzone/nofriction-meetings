import React from "react";

// Stroke icons in the app's style (src/components/icons.tsx: 24x24 viewBox,
// currentColor, round caps). Drawn here so the film has no emoji.
type P = { size?: number; color?: string; stroke?: number; draw?: number; style?: React.CSSProperties };

const Svg: React.FC<P & { children: React.ReactNode; fill?: string }> = ({ size = 24, color = "currentColor", stroke = 1.75, children, style, fill = "none" }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill={fill} stroke={color} strokeWidth={stroke} strokeLinecap="round" strokeLinejoin="round" style={style}>
    {children}
  </svg>
);

/** `draw` 0..1 animates the stroke (pathLength trick). */
const dash = (draw?: number): React.SVGProps<SVGPathElement> =>
  draw === undefined ? {} : { pathLength: 1, strokeDasharray: 1, strokeDashoffset: 1 - draw };

export const LockIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <rect x="4.5" y="10.5" width="15" height="10" rx="2.2" {...(dash(p.draw) as object)} />
    <path d="M8 10.5V7.5a4 4 0 0 1 8 0v3" {...dash(p.draw)} />
    <path d="M12 14.5v2.5" {...dash(p.draw)} />
  </Svg>
);
export const StarIcon: React.FC<P & { filled?: boolean }> = ({ filled, ...p }) => (
  <Svg {...p} fill={filled ? p.color ?? "currentColor" : "none"}>
    <path d="M12 3.5l2.6 5.3 5.9.9-4.25 4.1 1 5.85L12 16.9l-5.25 2.75 1-5.85L3.5 9.7l5.9-.9z" />
  </Svg>
);
export const QuestionIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M9.6 9.3a2.5 2.5 0 0 1 4.8.9c0 1.7-2.4 2.2-2.4 3.8" />
    <path d="M12 17.2h.01" />
  </Svg>
);
export const PencilIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <path d="M4 20l4.2-1 10.3-10.3a2.1 2.1 0 0 0-3-3L5.2 16 4 20z" />
    <path d="M13.8 7.2l3 3" />
  </Svg>
);
export const MicIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <rect x="9" y="3" width="6" height="11.5" rx="3" />
    <path d="M5.5 11.5a6.5 6.5 0 0 0 13 0M12 18v3" />
  </Svg>
);
export const WifiOffIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <path d="M3 3l18 18" />
    <path d="M8.5 16.4a5 5 0 0 1 6.2-.6M5 12.9a10 10 0 0 1 4.4-2.4M19 12.9a10 10 0 0 0-2.6-1.8M1.9 9.2a15 15 0 0 1 4.3-2.6M22.1 9.2A15 15 0 0 0 11 5" />
    <path d="M12 20h.01" />
  </Svg>
);
export const BoltIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <path d="M13 2.5L4.5 13.5H12l-1 8 8.5-11H12l1-8z" />
  </Svg>
);
export const ShieldIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <path d="M12 3l7.5 3v5.5c0 4.6-3.2 8.3-7.5 9.5-4.3-1.2-7.5-4.9-7.5-9.5V6L12 3z" {...dash(p.draw)} />
    <path d="M8.7 12.2l2.3 2.3 4.4-4.6" {...dash(p.draw)} />
  </Svg>
);
export const PeopleIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <circle cx="9" cy="8" r="3.2" />
    <path d="M3 20c.6-3.4 3-5.4 6-5.4s5.4 2 6 5.4" />
    <circle cx="17" cy="9" r="2.5" />
    <path d="M16.5 14.6c2.3.2 4 1.9 4.5 4.6" />
  </Svg>
);
export const BookIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <path d="M12 6.5C10 5 7 4.5 3.5 5v13.5c3.5-.5 6.5 0 8.5 1.5 2-1.5 5-2 8.5-1.5V5C17 4.5 14 5 12 6.5z" />
    <path d="M12 6.5V20" />
  </Svg>
);
export const HeartIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <path d="M12 20s-7.5-4.4-7.5-10A4.3 4.3 0 0 1 12 7.3 4.3 4.3 0 0 1 19.5 10c0 5.6-7.5 10-7.5 10z" />
    <path d="M7.5 12h2.2l1.3-2.2 2 4 1.2-1.8h2.3" />
  </Svg>
);
export const NotesIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <rect x="5" y="3.5" width="14" height="17" rx="2" />
    <path d="M8.5 8h7M8.5 11.5h7M8.5 15h4.5" />
  </Svg>
);
export const CardsIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <rect x="3.5" y="7" width="13" height="13" rx="2" />
    <path d="M7.5 7V5.5a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-2" />
  </Svg>
);
export const QuizIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <circle cx="12" cy="12" r="9" />
    <path d="M8.3 12.3l2.5 2.5 5-5.3" />
  </Svg>
);
export const KeyTermIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <path d="M4 6h10M4 12h16M4 18h7" />
  </Svg>
);
export const BookmarkIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <path d="M7 3.5h10a1 1 0 0 1 1 1V21l-6-4-6 4V4.5a1 1 0 0 1 1-1z" />
  </Svg>
);
export const FlagIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <path d="M5 21V4M5 4h11l-2 4 2 4H5" />
  </Svg>
);
export const RewindIcon: React.FC<P> = (p) => (
  <Svg {...p}>
    <path d="M11 6L4 12l7 6V6zM20 6l-7 6 7 6V6z" />
  </Svg>
);
