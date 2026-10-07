import React from "react";
import { AbsoluteFill, interpolate, useCurrentFrame } from "remotion";
import { CLIPS } from "../clips";
import { PencilIcon, QuestionIcon, RewindIcon, StarIcon } from "../components/Icons";
import { Backdrop, Chip, Enter, Timeline, clock } from "../components/Motifs";
import { Label, Reveal } from "../components/Reveal";
import { Screen } from "../components/Screen";
import { C, F, easeIn } from "../theme";

// 4. Mark what matters. 216 frames (3 bars). The last 24 frames start the
// rewind: the timeline scrubs backward into scene 5.

const THIRD = ["Follow up", "On the test", "Remember"];
const TYPES = ["Meeting", "Class", "Personal"];

export const S4Mark: React.FC = () => {
  const frame = useCurrentFrame();
  // playhead time: plays forward, then rewinds hard at the end
  const fwd = 742 + frame * 0.32;
  const rw = interpolate(frame, [190, 216], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing: easeIn });
  const t = fwd - rw * 260;
  // which type's third mark is highlighted (the words stay on screen)
  const third = frame < 100 ? -1 : Math.min(2, Math.floor((frame - 100) / 30));
  return (
    <AbsoluteFill>
      <Backdrop glowY={55} glow={0.9} />
      <div style={{ position: "absolute", top: 84, left: 0, right: 0, opacity: 1 - rw }}>
        <Reveal text="Mark what *matters.*" at={0} size={100} align="center" />
      </div>
      <div style={{ position: "absolute", left: 104, top: 236, opacity: 1 - rw * 0.6 }}>
        <Enter at={-8} dur={28} from={{ x: -80 }}>
          <Screen clip={CLIPS.iosMark.clip} from={CLIPS.iosMark.from} kind="phone" width={360} glow={0.5} />
        </Enter>
      </div>
      <div style={{ position: "absolute", left: 1500, top: 350, opacity: 1 - rw * 0.6 }}>
        <Enter at={-4} dur={28} from={{ x: 80 }}>
          <Screen clip={CLIPS.watchClass.clip} from={CLIPS.watchClass.from} kind="watch" width={270} />
          <Label at={26} color={C.text3} style={{ marginTop: 22, textAlign: "center" }}>Watch, too</Label>
        </Enter>
      </div>
      {/* the three marks */}
      <div style={{ position: "absolute", left: 500, width: 960, top: 320, display: "flex", justifyContent: "center", gap: 20, opacity: 1 - rw }}>
        <Chip at={34} size={38} tone="yellow" icon={<StarIcon size={38} filled color="#0a0a0a" />}>Important</Chip>
        <Chip at={58} size={38} icon={<QuestionIcon size={38} />}>Question</Chip>
        <Chip at={82} size={38} icon={<PencilIcon size={38} />}>
          On the test
        </Chip>
      </div>
      <div style={{ position: "absolute", left: 500, width: 960, top: 448, textAlign: "center", fontFamily: F.sans, fontSize: 32, fontWeight: 500, color: C.text2, opacity: interpolate(frame, [96, 112], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }) * (1 - rw) }}>
        <div>The third mark follows the type:</div>
        <div style={{ marginTop: 10, display: "flex", justifyContent: "center", gap: 34, fontSize: 28 }}>
          {TYPES.map((ty, i) => (
            <span key={ty} style={{ color: i === third ? C.yellow : C.text2 }}>
              <span style={{ color: i === third ? C.yellow : C.text3 }}>{ty}</span> · {THIRD[i]}
            </span>
          ))}
        </div>
      </div>
      <div style={{ position: "absolute", left: 500, top: 596, width: 768, transform: "scale(1.25)", transformOrigin: "0 0" }}>
        <Timeline
          width={768}
          t={t}
          pxPerSec={7}
          draw={interpolate(frame, [8, 40], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })}
          marks={[
            { at: 756, appear: 38, icon: <StarIcon size={26} filled color="#0a0a0a" /> },
            { at: 768, appear: 62, color: "#e5e5e5", icon: <QuestionIcon size={26} color="#0a0a0a" stroke={2.2} /> },
            { at: 780, appear: 86, color: "#e5e5e5", icon: <PencilIcon size={26} color="#0a0a0a" stroke={2.2} /> },
          ]}
        />
        <div style={{ position: "absolute", left: 0, width: 768, top: 170, textAlign: "center", fontFamily: F.mono, fontSize: 26, color: rw > 0 ? C.yellow : C.text2, letterSpacing: "0.08em" }}>
          {rw > 0 && <RewindIcon size={22} style={{ verticalAlign: "-3px", marginRight: 12 }} />}
          {clock(Math.max(0, t))}
        </div>
      </div>
    </AbsoluteFill>
  );
};
