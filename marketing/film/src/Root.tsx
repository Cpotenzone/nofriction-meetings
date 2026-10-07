import React from "react";
import { Composition } from "remotion";
import { Film } from "./Film";
import { PreviewIPhone } from "./previews/PreviewIPhone";
import { PreviewMac } from "./previews/PreviewMac";
import { FILM_FRAMES, FPS, PREVIEW_FRAMES } from "./timing";

export const Root: React.FC = () => (
  <>
    {/* Hero film for the website: 1920x1080, 62.4 s */}
    <Composition id="Film" component={Film} durationInFrames={FILM_FRAMES} fps={FPS} width={1920} height={1080} defaultProps={{ music: true }} />
    {/* App Store app previews (Apple: 15–30 s, ≤30 fps). iPhone 6.9": 886x1920 portrait. Mac: 1920x1080. */}
    <Composition id="PreviewIPhone" component={PreviewIPhone} durationInFrames={PREVIEW_FRAMES} fps={FPS} width={886} height={1920} defaultProps={{ music: true }} />
    <Composition id="PreviewMac" component={PreviewMac} durationInFrames={PREVIEW_FRAMES} fps={FPS} width={1920} height={1080} defaultProps={{ music: true }} />
  </>
);
