import React from "react";
import { AbsoluteFill, Html5Audio, Sequence, staticFile } from "remotion";
import { S1Moments } from "./scenes/S1Moments";
import { S2Record } from "./scenes/S2Record";
import { S3Transcribe } from "./scenes/S3Transcribe";
import { S4Mark } from "./scenes/S4Mark";
import { S5Rewind } from "./scenes/S5Rewind";
import { S6Notes } from "./scenes/S6Notes";
import { S7Offline } from "./scenes/S7Offline";
import { S8End } from "./scenes/S8End";
import { BAR, FILM_SCENES } from "./timing";

const SCENES: { key: keyof typeof FILM_SCENES; C: React.FC }[] = [
  { key: "moments", C: S1Moments },
  { key: "record", C: S2Record },
  { key: "transcribe", C: S3Transcribe },
  { key: "mark", C: S4Mark },
  { key: "rewind", C: S5Rewind },
  { key: "notes", C: S6Notes },
  { key: "offline", C: S7Offline },
  { key: "end", C: S8End },
];

/** The hero film: 1920x1080, 62.4 s. Tells the story in type; sound optional. */
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
