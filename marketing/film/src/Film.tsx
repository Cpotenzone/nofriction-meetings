import React from "react";
import { AbsoluteFill, Html5Audio, Sequence, staticFile } from "remotion";
import { S8End } from "./scenes/S8End";
import { SChat, SMark, SNotes, SPrivacy, SRecord, SRewind, SSync } from "./scenes/Scenes";
import { BAR, FILM_SCENES } from "./timing";

// Storyboard (3.7 Mac UI · 1.0.1 iPhone and Watch):
// record → mark (then Stop) → rewind → notes and review guide → chat → sync → privacy → end card
const SCENES: { key: keyof typeof FILM_SCENES; C: React.FC }[] = [
  { key: "record", C: SRecord },
  { key: "mark", C: SMark },
  { key: "rewind", C: SRewind },
  { key: "notes", C: SNotes },
  { key: "chat", C: SChat },
  { key: "sync", C: SSync },
  { key: "privacy", C: SPrivacy },
  { key: "end", C: S8End },
];

/** The hero film: 1920x1080, 33.6 s. Tells the story in type; sound optional. */
export const Film: React.FC<{ music: boolean }> = ({ music }) => (
  <AbsoluteFill style={{ background: "#050505" }}>
    {SCENES.map(({ key, C }) => (
      <Sequence key={key} name={key} from={FILM_SCENES[key].from * BAR} durationInFrames={FILM_SCENES[key].bars * BAR}>
        <C />
      </Sequence>
    ))}
    {music && <Html5Audio src={staticFile("audio/score-film.wav")} />}
  </AbsoluteFill>
);
