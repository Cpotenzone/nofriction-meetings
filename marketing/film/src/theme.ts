import { Easing } from "remotion";

// The app's own tokens: DESIGN.md / src/App.css (Mac) and Theme in
// ios/NoFriction/App/NoFrictionApp.swift. Hazard yellow on matte black.
export const C = {
  bg: "#050505",
  card: "#0d0d0d",
  well: "#171717",
  border: "#262626",
  borderBright: "#404040",
  text: "#f5f5f5",
  text2: "#a3a3a3",
  text3: "#6b6b6b",
  yellow: "#facc15",
  yellowDeep: "#eab308",
  yellowDim: "rgba(250, 204, 21, 0.5)",
  red: "#ef4444",
  recRed: "#f04545",
  green: "#34d399",
  ai: "#5a8ce6",
};

// The app's own font stacks (DESIGN.md), resolved from the fonts installed
// on the rendering Mac: nothing is fetched at render time. Install Inter
// (SIL OFL, rsms.me/inter) for the exact look; without it the type falls
// back to the system font. Mono falls back to SF Mono / Menlo, as in the app.
export const F = {
  sans: "'Inter', -apple-system, BlinkMacSystemFont, 'Helvetica Neue', sans-serif",
  mono: "'JetBrains Mono', 'SF Mono', Menlo, monospace",
};

/** Expo-out: fast in, long settle. Used for most entrances. */
export const easeOut = Easing.bezier(0.16, 1, 0.3, 1);
/** Smooth in-out for camera moves. */
export const easeInOut = Easing.bezier(0.65, 0, 0.35, 1);
/** Quick exit. */
export const easeIn = Easing.bezier(0.7, 0, 0.84, 0);
