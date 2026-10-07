import { loadFont as loadInter } from "@remotion/google-fonts/Inter";
import { loadFont as loadMono } from "@remotion/google-fonts/JetBrainsMono";
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

const inter = loadInter("normal", { weights: ["400", "500", "600", "700", "800"], subsets: ["latin"] });
const mono = loadMono("normal", { weights: ["400", "500", "700"], subsets: ["latin"] });

export const F = {
  sans: inter.fontFamily,
  mono: mono.fontFamily,
};

/** Expo-out: fast in, long settle. Used for most entrances. */
export const easeOut = Easing.bezier(0.16, 1, 0.3, 1);
/** Smooth in-out for camera moves. */
export const easeInOut = Easing.bezier(0.65, 0, 0.35, 1);
/** Quick exit. */
export const easeIn = Easing.bezier(0.7, 0, 0.84, 0);
