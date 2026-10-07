#!/usr/bin/env python3
"""Cut a clip out of a raw Simulator recording.

    cut.py RAW OUT --start S --duration D [--max-hold H] [--end-hold E] [--crf 15]

The Simulator (simctl io recordVideo) writes a frame only when the screen
changes, so a stretch with no frames is a still screen. While a UI test runs,
those stretches include XCUITest's own waits (it lets the app settle before
and after every action, longer when the machine is busy). Any still stretch
inside [S, S+D] longer than H seconds is shortened to H; nothing changes on
screen during it, so the cut can't be seen. The last still stretch is kept up
to E seconds. The clip is constant 30 fps, H.264 High, yuv420p, CRF, no
audio, +faststart, at the recording's size.

Timing comes from the packets' presentation times, not from ffmpeg's
filters: the Simulator's files give many frames the same timestamp, and
ffmpeg's decoder then falls back to the decode times, which run seconds
early. So the frames are decoded in order (one per packet, presentation
order) and placed on the 30 fps grid here.

Prints the kept ranges (raw seconds) and the clip's length.
"""
import argparse
import json
import subprocess
import sys


def probe(raw):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
         "stream=width,height:packet=pts_time", "-of", "json", raw],
        check=True, capture_output=True, text=True).stdout
    j = json.loads(out)
    s = j["streams"][0]
    times = sorted(float(p["pts_time"]) for p in j.get("packets", []) if "pts_time" in p)
    return int(s["width"]), int(s["height"]), times


def keep_ranges(times, start, end, max_hold, end_hold):
    """[start, end] minus the excess of every still stretch."""
    changes = [t for t in times if start < t < end]
    cuts, prev = [], start
    for t in changes:
        if t - prev > max_hold:
            cuts.append((prev + max_hold, t))
        prev = t
    if end - prev > end_hold:
        cuts.append((prev + end_hold, end))
    ranges, cursor = [], start
    for a, b in cuts:
        if a > cursor:
            ranges.append((cursor, a))
        cursor = b
    if cursor < end:
        ranges.append((cursor, end))
    return ranges


def source_times(ranges, fps=30):
    """The raw time shown at each output frame."""
    out = []
    for s, e in ranges:
        n = int(round((e - s) * fps))
        out += [s + k / fps for k in range(n)]
    return out


def first_clock_frame(raw, after, box=(290, 6, 120, 44), threshold=10.0):
    """Apple Watch: when the app's own UI is first on screen. After `after`
    (raw seconds, the launch), the top-right corner goes dark (the app's
    launch screen has no clock) and then lights up again (the system clock
    over the app's UI)."""
    x, y, bw, bh = box
    _, _, times = probe(raw)
    dec = subprocess.Popen(
        ["ffmpeg", "-nostdin", "-loglevel", "fatal", "-i", raw, "-map", "0:v:0", "-fps_mode", "passthrough",
         "-vf", f"crop={bw}:{bh}:{x}:{y}", "-f", "rawvideo", "-pix_fmt", "gray", "-"],
        stdout=subprocess.PIPE)
    found, dark = None, False
    for t in times:
        data = dec.stdout.read(bw * bh)
        if len(data) < bw * bh:
            break
        if t < after:
            continue
        level = sum(data) / len(data)
        if not dark:
            dark = level < threshold / 3
        elif level > threshold:
            found = t
            break
    dec.kill()
    dec.wait()
    return found


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--watch-ui":
        # cut.py --watch-ui RAW AFTER → raw seconds of the first frame with the app's UI
        t = first_clock_frame(sys.argv[2], float(sys.argv[3]))
        print("" if t is None else f"{t:.3f}")
        return
    ap = argparse.ArgumentParser()
    ap.add_argument("raw")
    ap.add_argument("out")
    ap.add_argument("--start", type=float, required=True)
    ap.add_argument("--duration", type=float, required=True)
    ap.add_argument("--max-hold", type=float, default=1.1)
    ap.add_argument("--end-hold", type=float, default=1.4)
    ap.add_argument("--crf", type=int, default=15)
    a = ap.parse_args()

    w, h, times = probe(a.raw)
    start, end = max(0.0, a.start), a.start + a.duration
    ranges = keep_ranges(times, start, end, a.max_hold, a.end_hold)
    wanted = source_times(ranges)
    size = w * h * 3 // 2   # yuv420p

    dec = subprocess.Popen(
        # (fatal: the muxer warns about every repeated timestamp, harmlessly)
        ["ffmpeg", "-nostdin", "-loglevel", "fatal", "-i", a.raw, "-map", "0:v:0",
         "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-"],
        stdout=subprocess.PIPE)
    enc = subprocess.Popen(
        ["ffmpeg", "-nostdin", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "yuv420p",
         "-s", f"{w}x{h}", "-r", "30", "-i", "-", "-c:v", "libx264", "-preset", "slow",
         "-crf", str(a.crf), "-profile:v", "high", "-pix_fmt", "yuv420p", "-an",
         "-movflags", "+faststart", a.out],
        stdin=subprocess.PIPE)

    # Frame i (decoder output order) is shown from times[i] until times[i+1]
    i, frame, written = -1, None, 0
    for t in wanted:
        while i + 1 < len(times) and times[i + 1] <= t + 1e-6:
            data = dec.stdout.read(size)
            if len(data) < size:
                break
            frame = data
            i += 1
        if frame is None:
            continue
        enc.stdin.write(frame)
        written += 1
    dec.kill()
    dec.wait()
    enc.stdin.close()
    if enc.wait() != 0:
        sys.exit("encode failed")
    total = written / 30
    print("kept " + " ".join(f"{s:.2f}-{e:.2f}" for s, e in ranges) + f"  → {total:.2f} s")


if __name__ == "__main__":
    main()
