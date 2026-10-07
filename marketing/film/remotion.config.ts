import { Config } from "@remotion/cli/config";

// public/ holds the committed brand assets (mark.svg, app-icon.png).
// public/footage and public/audio are symlinks to ../out/... (gitignored),
// created by `npm run render` / scripts/link-media.mjs.
Config.setVideoImageFormat("jpeg");
Config.setJpegQuality(96);
Config.setConcurrency(6);
Config.setChromiumOpenGlRenderer("angle");
