// noFriction launch film: the score, synthesized from scratch.
//
// No samples, no downloaded or copyrighted audio: every sound below is made
// here from oscillators, noise and envelopes, so the music is ours and
// royalty-free. Writes 48 kHz / 16-bit stereo WAV files.
//
//   node scripts/make-score.mjs            # all arrangements
//   node scripts/make-score.mjs film       # one of: film, preview
//
// Tempo is 100 BPM so one beat is exactly 18 frames at 30 fps and one bar
// (4 beats) is 72 frames = 2.4 s. The film's scenes are cut on these bars
// (src/timing.ts), so hits land on cuts.

import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const OUT = join(HERE, "..", "..", "out", "audio");
const SR = 48000;
const BPM = 100;
const BEAT = 60 / BPM; // 0.6 s
const BAR = 4 * BEAT; // 2.4 s

// ─── Tiny deterministic noise (same score every render) ────────────────────
let seed = 0x9e3779b9;
function rand() {
  seed ^= seed << 13; seed >>>= 0;
  seed ^= seed >>> 17;
  seed ^= seed << 5; seed >>>= 0;
  return seed / 4294967296;
}
const noise = () => rand() * 2 - 1;
const mtof = (m) => 440 * Math.pow(2, (m - 69) / 12);
const clamp = (x, a, b) => Math.max(a, Math.min(b, x));

// ─── DSP building blocks ───────────────────────────────────────────────────
function polyblep(t, dt) {
  if (t < dt) { t /= dt; return t + t - t * t - 1; }
  if (t > 1 - dt) { t = (t - 1) / dt; return t * t + t + t + 1; }
  return 0;
}

/** Zavalishin TPT state-variable filter (stable under modulation). */
class SVF {
  constructor() { this.ic1 = 0; this.ic2 = 0; }
  process(x, cutoff, q, mode = "lp") {
    const g = Math.tan(Math.PI * clamp(cutoff, 20, SR * 0.45) / SR);
    const k = 1 / q;
    const a1 = 1 / (1 + g * (g + k));
    const a2 = g * a1;
    const a3 = g * a2;
    const v3 = x - this.ic2;
    const v1 = a1 * this.ic1 + a2 * v3;
    const v2 = this.ic2 + a2 * this.ic1 + a3 * v3;
    this.ic1 = 2 * v1 - this.ic1;
    this.ic2 = 2 * v2 - this.ic2;
    if (mode === "lp") return v2;
    if (mode === "bp") return v1;
    return x - k * v1 - v2; // hp
  }
}

/** Freeverb-style reverb (Schroeder combs + allpasses), stereo. */
class Reverb {
  constructor(room = 0.86, damp = 0.35) {
    const combs = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
    const aps = [556, 441, 341, 225];
    const scale = SR / 44100;
    const mk = (n, spread) => ({ buf: new Float32Array(Math.round((n + spread) * scale)), i: 0, store: 0 });
    this.cl = combs.map((n) => mk(n, 0));
    this.cr = combs.map((n) => mk(n, 23));
    this.al = aps.map((n) => mk(n, 0));
    this.ar = aps.map((n) => mk(n, 23));
    this.room = room; this.damp = damp;
  }
  comb(c, x) {
    const y = c.buf[c.i];
    c.store = y * (1 - this.damp) + c.store * this.damp;
    c.buf[c.i] = x + c.store * this.room;
    c.i = (c.i + 1) % c.buf.length;
    return y;
  }
  allpass(a, x) {
    const b = a.buf[a.i];
    const y = -x + b;
    a.buf[a.i] = x + b * 0.5;
    a.i = (a.i + 1) % a.buf.length;
    return y;
  }
  process(xl, xr) {
    const x = (xl + xr) * 0.015;
    let l = 0, r = 0;
    for (const c of this.cl) l += this.comb(c, x);
    for (const c of this.cr) r += this.comb(c, x);
    for (const a of this.al) l = this.allpass(a, l);
    for (const a of this.ar) r = this.allpass(a, r);
    return [l, r];
  }
}

// ─── Harmony ───────────────────────────────────────────────────────────────
// D major: I (Dmaj9) · vi (Bm9) · IV (Gmaj9) · V (A add9). Pads leave the root
// to the bass, so the voicings stay open and soft.
const CHORDS = {
  D: { root: 38, pad: [57, 61, 64, 66], arp: [69, 73, 76, 78, 81] },
  Bm: { root: 35, pad: [54, 57, 61, 62], arp: [66, 69, 73, 74, 78] },
  G: { root: 31, pad: [54, 57, 59, 62], arp: [66, 69, 71, 74, 78] },
  A: { root: 33, pad: [57, 59, 61, 64], arp: [69, 71, 73, 76, 81] },
};

// ─── Arrangements ──────────────────────────────────────────────────────────
// One entry per bar: chord + which parts play. "pad" is a brightness 0..1.
function filmArrangement() {
  const cycle = ["D", "Bm", "G", "A"];
  const bars = [];
  for (let b = 0; b < 26; b++) {
    const chord = b <= 19 ? cycle[b % 4] : ["Bm", "G", "A", "D", "D", "D"][b - 20];
    const s = { chord, pad: 0.5, bass: 0, kick: 0, hat: 0, clap: 0, arp: 0 };
    if (b <= 3) { s.pad = 0.32 + b * 0.08; s.arp = b >= 2 ? 1 : 0; }          // 1. moments
    else if (b <= 6) { s.pad = 0.55; s.bass = 1; s.kick = 1; s.arp = 2; }   // 2. record
    else if (b <= 9) { s.pad = 0.62; s.bass = 1; s.kick = 1; s.hat = 1; s.arp = 3; } // 3. transcribe
    else if (b <= 12) { s.pad = 0.68; s.bass = 2; s.kick = 1; s.hat = 1; s.clap = 1; s.arp = 3; } // 4. mark
    else if (b <= 19) { s.pad = 0.8; s.bass = 2; s.kick = 2; s.hat = 2; s.clap = 1; s.arp = 3; } // 5-6. rewind, notes
    else if (b <= 22) { s.pad = 0.45; s.arp = 2; }                          // 7. offline (breakdown)
    else { s.pad = 0.7; s.arp = b === 23 ? 1 : 0; }                         // 8. end card
    bars.push(s);
  }
  return {
    name: "score-film", bars,
    risers: [{ endBar: 13, bars: 1 }, { endBar: 23, bars: 1.5 }],
    rewinds: [13],
    impacts: [13, 23],
    bells: [{ bar: 23, beat: 0 }, { bar: 0, beat: 0, soft: true }],
    tail: 1.6,
  };
}

function previewArrangement() {
  const cycle = ["D", "Bm", "G", "A"];
  const bars = [];
  for (let b = 0; b < 12; b++) {
    const chord = b <= 9 ? cycle[b % 4] : "D";
    const s = { chord, pad: 0.6, bass: 0, kick: 0, hat: 0, clap: 0, arp: 0 };
    if (b <= 1) { s.pad = 0.35 + b * 0.15; s.arp = 2; }
    else if (b <= 9) { s.pad = 0.7; s.bass = b >= 3 ? 2 : 1; s.kick = 1; s.hat = b >= 3 ? 2 : 1; s.clap = b >= 4 ? 1 : 0; s.arp = 3; }
    else { s.pad = 0.65; s.arp = 1; }
    bars.push(s);
  }
  return { name: "score-preview", bars, risers: [{ endBar: 2, bars: 1 }, { endBar: 10, bars: 1 }], rewinds: [], impacts: [2, 10], bells: [{ bar: 10, beat: 0 }], tail: 1.2 };
}

// ─── Render ────────────────────────────────────────────────────────────────
function render(arr) {
  const totalSec = arr.bars.length * BAR;
  const N = Math.round(totalSec * SR);
  const L = new Float32Array(N), R = new Float32Array(N);
  const sendL = new Float32Array(N), sendR = new Float32Array(N); // reverb send
  const dlyL = new Float32Array(N), dlyR = new Float32Array(N); // delay send
  const sidechain = new Float32Array(N).fill(1); // ducking from the kick

  const add = (buf, i, v) => { if (i >= 0 && i < N) buf[i] += v; };
  const t2i = (t) => Math.round(t * SR);
  const barAt = (t) => arr.bars[clamp(Math.floor(t / BAR), 0, arr.bars.length - 1)];

  // 1. Pad: 3 detuned saws per note through a slow, breathing low-pass.
  {
    const notes = new Map(); // midi → state
    for (let b = 0; b < arr.bars.length; b++) {
      for (const m of CHORDS[arr.bars[b].chord].pad) {
        if (!notes.has(m)) notes.set(m, { m, ph: [rand(), rand(), rand()], f: new SVF(), f2: new SVF() });
      }
    }
    const detune = [-0.11, 0, 0.12];
    for (const st of notes.values()) {
      const freq = mtof(st.m);
      let amp = 0;
      const pan = ((st.m * 7) % 11) / 11 - 0.5;
      for (let i = 0; i < N; i++) {
        const t = i / SR;
        const bi = Math.min(Math.floor(t / BAR), arr.bars.length - 1);
        const bar = arr.bars[bi];
        const on = CHORDS[bar.chord].pad.includes(st.m);
        const target = on ? 1 : 0;
        // slow attack, slower release: chords melt into each other
        amp += (target - amp) * (on ? 1 / (0.45 * SR) : 1 / (0.8 * SR));
        if (amp < 1e-5 && !on) continue;
        let s = 0;
        for (let v = 0; v < 3; v++) {
          const f = freq * Math.pow(2, detune[v] / 12);
          const dt = f / SR;
          st.ph[v] += dt; if (st.ph[v] >= 1) st.ph[v] -= 1;
          s += 2 * st.ph[v] - 1 - polyblep(st.ph[v], dt);
        }
        // brightness follows the section, with a slow LFO
        const prev = arr.bars[Math.max(0, bi - 1)].pad;
        const frac = (t - bi * BAR) / BAR;
        const bright = prev + (bar.pad - prev) * Math.min(1, frac * 2);
        const lfo = 0.5 + 0.5 * Math.sin(2 * Math.PI * t / (BAR * 2) + st.m);
        const cutoff = 260 + bright * 2400 + lfo * 380 * bright;
        let y = st.f.process(s, cutoff, 0.75);
        y = st.f2.process(y, cutoff * 1.4, 0.6);
        y *= amp * 0.08;
        const l = y * (0.5 - pan * 0.6), r = y * (0.5 + pan * 0.6);
        L[i] += l; R[i] += r;
        sendL[i] += l * 0.9; sendR[i] += r * 0.9;
      }
    }
  }

  // 2. Kick + sidechain envelope
  const kick = (t, vel) => {
    const i0 = t2i(t), len = Math.round(0.42 * SR);
    let ph = 0;
    for (let k = 0; k < len; k++) {
      const tt = k / SR;
      const f = 44 + 92 * Math.exp(-tt * 26);
      ph += f / SR;
      const env = Math.exp(-tt * 7.5) * (1 - Math.exp(-tt * 900));
      const click = k < 90 ? noise() * 0.12 * (1 - k / 90) : 0;
      const v = (Math.sin(2 * Math.PI * ph) * env + click) * 0.55 * vel;
      add(L, i0 + k, v); add(R, i0 + k, v);
    }
    const dlen = Math.round(BEAT * 0.9 * SR);
    for (let k = 0; k < dlen; k++) {
      const d = 1 - 0.55 * vel * Math.pow(1 - k / dlen, 2);
      if (i0 + k < N) sidechain[i0 + k] = Math.min(sidechain[i0 + k], d);
    }
  };
  const hat = (t, vel, open = false) => {
    const i0 = t2i(t), len = Math.round((open ? 0.22 : 0.05) * SR);
    const f = new SVF();
    for (let k = 0; k < len; k++) {
      const env = Math.exp(-k / SR * (open ? 16 : 70));
      const v = f.process(noise(), 8200, 0.9, "hp") * env * 0.09 * vel;
      add(L, i0 + k, v * 0.8); add(R, i0 + k, v * 1.1);
    }
  };
  const clap = (t, vel) => {
    const i0 = t2i(t), len = Math.round(0.25 * SR);
    const f = new SVF();
    for (let k = 0; k < len; k++) {
      const tt = k / SR;
      // three quick bursts, then a tail
      const burst = (tt < 0.01 ? 1 : tt < 0.02 ? 0.6 : tt < 0.03 ? 0.9 : 0) + Math.exp(-tt * 22) * 0.7;
      const v = f.process(noise(), 1500, 1.4, "bp") * burst * 0.16 * vel;
      add(L, i0 + k, v); add(R, i0 + k, v);
      add(sendL, i0 + k, v * 0.8); add(sendR, i0 + k, v * 0.8);
    }
  };

  // 3. Bass: a round sine with a little octave saw for presence
  const bassNote = (t, m, dur, vel) => {
    const i0 = t2i(t), len = Math.round((dur + 0.08) * SR);
    const f = mtof(m), lp = new SVF();
    let ph = 0, ph2 = 0;
    for (let k = 0; k < len; k++) {
      const tt = k / SR;
      const env = Math.min(1, tt / 0.012) * (tt > dur ? Math.exp(-(tt - dur) * 60) : 1) * (0.75 + 0.25 * Math.exp(-tt * 3));
      ph += f / SR; if (ph >= 1) ph -= 1;
      ph2 += (2 * f) / SR; if (ph2 >= 1) ph2 -= 1;
      const saw = lp.process(2 * ph2 - 1 - polyblep(ph2, 2 * f / SR), 520, 0.7);
      const v = Math.tanh((Math.sin(2 * Math.PI * ph) * 0.9 + saw * 0.25) * 1.4) * 0.17 * env * vel;
      add(L, i0 + k, v); add(R, i0 + k, v);
    }
  };

  // 4. Pluck arpeggio, into a ping-pong delay
  const pluck = (t, m, vel, pan = 0) => {
    const i0 = t2i(t), len = Math.round(0.9 * SR);
    const f = mtof(m), lp = new SVF();
    let ph = 0;
    for (let k = 0; k < len; k++) {
      const tt = k / SR;
      ph += f / SR; if (ph >= 1) ph -= 1;
      const tri = 1 - 4 * Math.abs(ph - 0.5);
      const sq = ph < 0.5 ? 1 : -1;
      const env = Math.exp(-tt * 9) * Math.min(1, tt / 0.003);
      const y = lp.process(tri * 0.8 + sq * 0.12, 900 + 3800 * Math.exp(-tt * 16), 0.8) * env * 0.09 * vel;
      add(L, i0 + k, y * (1 - pan)); add(R, i0 + k, y * (1 + pan));
      add(dlyL, i0 + k, y * 0.55); add(dlyR, i0 + k, y * 0.55);
      add(sendL, i0 + k, y * 0.5); add(sendR, i0 + k, y * 0.5);
    }
  };

  // 5. FM bell for the logo / end card
  const bell = (t, m, vel) => {
    const i0 = t2i(t), len = Math.round(4.5 * SR);
    const f = mtof(m);
    for (let k = 0; k < len; k++) {
      const tt = k / SR;
      const idx = 2.2 * Math.exp(-tt * 3);
      const mod = Math.sin(2 * Math.PI * f * 3.5 * tt) * idx;
      const env = Math.exp(-tt * 1.1) * Math.min(1, tt / 0.002);
      const v = (Math.sin(2 * Math.PI * f * tt + mod) * 0.7 + Math.sin(2 * Math.PI * f * 2 * tt) * 0.12 * Math.exp(-tt * 3)) * env * 0.12 * vel;
      add(L, i0 + k, v * 0.9); add(R, i0 + k, v);
      add(sendL, i0 + k, v * 1.4); add(sendR, i0 + k, v * 1.4);
    }
  };

  // 6. Riser: band-passed noise sweeping up, and an impact (sub drop + air)
  const riser = (t0, t1) => {
    const i0 = t2i(t0), i1 = t2i(t1);
    const f = new SVF();
    for (let i = i0; i < i1; i++) {
      const p = (i - i0) / (i1 - i0);
      const v = f.process(noise(), 300 + 7000 * p * p, 2.2, "bp") * p * p * 0.22;
      add(L, i, v * (1 - 0.3 * Math.sin(p * 9))); add(R, i, v * (1 + 0.3 * Math.sin(p * 9)));
      add(sendL, i, v * 0.6); add(sendR, i, v * 0.6);
    }
  };
  const impact = (t) => {
    const i0 = t2i(t), len = Math.round(2.2 * SR);
    let ph = 0;
    const f = new SVF();
    for (let k = 0; k < len; k++) {
      const tt = k / SR;
      ph += (32 + 40 * Math.exp(-tt * 5)) / SR;
      const sub = Math.sin(2 * Math.PI * ph) * Math.exp(-tt * 2.2) * 0.42;
      const air = f.process(noise(), 5200, 0.7, "hp") * Math.exp(-tt * 4) * 0.05;
      add(L, i0 + k, sub + air); add(R, i0 + k, sub - air * 0.5);
      add(sendL, i0 + k, air * 2); add(sendR, i0 + k, air * 2);
    }
  };
  // Tape-rewind gesture: a descending, wobbling saw and a down-sweep of noise
  const rewind = (t) => {
    const len = Math.round(0.75 * SR), i0 = t2i(t - 0.75);
    let ph = 0;
    const lp = new SVF(), bp = new SVF();
    for (let k = 0; k < len; k++) {
      const p = k / len;
      const f = 1400 * Math.pow(0.08, p) * (1 + 0.06 * Math.sin(2 * Math.PI * 18 * p));
      ph += f / SR; if (ph >= 1) ph -= 1;
      const saw = lp.process(2 * ph - 1, 2500 * (1 - p) + 300, 0.9);
      const n = bp.process(noise(), 6000 * (1 - p) + 400, 1.5, "bp");
      const env = Math.sin(Math.PI * Math.min(1, p * 1.15)) * 0.12;
      add(L, i0 + k, (saw * 0.6 + n) * env); add(R, i0 + k, (saw * 0.6 - n) * env);
      add(sendL, i0 + k, n * env); add(sendR, i0 + k, n * env);
    }
  };

  // ── Sequence the parts ──
  for (let b = 0; b < arr.bars.length; b++) {
    const s = arr.bars[b], c = CHORDS[s.chord], t0 = b * BAR;
    if (s.kick) for (let q = 0; q < 4; q++) {
      if (s.kick === 1 && (q === 1 || q === 3)) continue;
      kick(t0 + q * BEAT, q === 0 ? 1 : 0.85);
    }
    if (s.kick === 1 && b % 2 === 1) kick(t0 + 3.5 * BEAT, 0.6);
    if (s.hat) for (let e = 0; e < 8; e++) {
      if (e % 2 === 1) hat(t0 + e * BEAT / 2, s.hat === 2 ? 1 : 0.7, s.hat === 2 && e === 7);
      else if (s.hat === 2) hat(t0 + e * BEAT / 2, 0.35);
    }
    if (s.clap) { clap(t0 + BEAT, 0.8); clap(t0 + 3 * BEAT, 0.9); }
    if (s.bass === 1) { bassNote(t0, c.root, BEAT * 1.8, 0.9); bassNote(t0 + BEAT * 2.5, c.root, BEAT * 1.3, 0.7); }
    if (s.bass === 2) {
      const pat = [[0, 0.8, 0], [1, 0.4, 12], [1.5, 0.4, 0], [2.5, 0.8, 0], [3.5, 0.4, 7]];
      for (const [beat, len, off] of pat) bassNote(t0 + beat * BEAT, c.root + off, len * BEAT, beat === 0 ? 1 : 0.75);
    }
    if (s.arp) {
      const step = s.arp === 1 ? 1 : s.arp === 2 ? 0.5 : 0.25; // beats per note
      const order = [0, 2, 1, 3, 2, 4, 1, 3];
      let n = 0;
      for (let beat = 0; beat < 4 - 1e-6; beat += step, n++) {
        const m = c.arp[order[n % order.length] % c.arp.length];
        const accent = Math.abs(beat % 1) < 1e-6 ? 1 : 0.7;
        pluck(t0 + beat * BEAT, m, accent * (s.arp === 1 ? 0.9 : 0.8), n % 2 ? 0.35 : -0.35);
      }
    }
  }
  for (const r of arr.risers) riser((r.endBar - r.bars) * BAR, r.endBar * BAR);
  for (const b of arr.impacts) impact(b * BAR);
  for (const b of arr.rewinds) rewind(b * BAR);
  for (const bl of arr.bells) {
    const t = bl.bar * BAR + bl.beat * BEAT;
    for (const [m, v] of [[74, 1], [81, 0.55], [86, 0.35]]) bell(t, m, (bl.soft ? 0.45 : 1) * v);
  }

  // ── Delay (dotted eighth, ping-pong) and reverb returns ──
  {
    const d = Math.round(BEAT * 0.75 * SR);
    const bl = new Float32Array(N), br = new Float32Array(N);
    for (let i = 0; i < N; i++) {
      // mono in on the left line; each line feeds the other = ping-pong
      const mono = (dlyL[i] + dlyR[i]) * 0.5;
      bl[i] = mono + (i >= d ? br[i - d] * 0.42 : 0);
      br[i] = i >= d ? bl[i - d] * 0.42 : 0;
      const outL = i >= d ? bl[i - d] : 0, outR = i >= d ? br[i - d] : 0;
      L[i] += outL * 0.5; R[i] += outR * 0.5;
      sendL[i] += outL * 0.2; sendR[i] += outR * 0.2;
    }
  }
  {
    const rv = new Reverb(0.88, 0.3);
    for (let i = 0; i < N; i++) {
      const [l, r] = rv.process(sendL[i], sendR[i]);
      L[i] += l * 0.55; R[i] += r * 0.55;
    }
  }

  // ── Master: pump, glue, fades, normalize ──
  const fadeIn = 0.35, fadeOut = arr.tail;
  let peak = 0;
  for (let i = 0; i < N; i++) {
    const t = i / SR;
    const sc = 0.55 + 0.45 * sidechain[i]; // gentle pump on the pads
    let g = 1;
    if (t < fadeIn) g *= t / fadeIn;
    if (t > totalSec - fadeOut) g *= Math.pow(Math.max(0, (totalSec - t) / fadeOut), 1.6);
    L[i] = Math.tanh(L[i] * sc * 1.25) * g;
    R[i] = Math.tanh(R[i] * sc * 1.25) * g;
    peak = Math.max(peak, Math.abs(L[i]), Math.abs(R[i]));
  }
  const norm = peak > 0 ? 0.89 / peak : 1; // about -1 dBFS
  const pcm = Buffer.alloc(44 + N * 4);
  pcm.write("RIFF", 0); pcm.writeUInt32LE(36 + N * 4, 4); pcm.write("WAVE", 8);
  pcm.write("fmt ", 12); pcm.writeUInt32LE(16, 16); pcm.writeUInt16LE(1, 20); pcm.writeUInt16LE(2, 22);
  pcm.writeUInt32LE(SR, 24); pcm.writeUInt32LE(SR * 4, 28); pcm.writeUInt16LE(4, 32); pcm.writeUInt16LE(16, 34);
  pcm.write("data", 36); pcm.writeUInt32LE(N * 4, 40);
  for (let i = 0; i < N; i++) {
    pcm.writeInt16LE(Math.round(clamp(L[i] * norm, -1, 1) * 32767), 44 + i * 4);
    pcm.writeInt16LE(Math.round(clamp(R[i] * norm, -1, 1) * 32767), 46 + i * 4);
  }
  mkdirSync(OUT, { recursive: true });
  const file = join(OUT, `${arr.name}.wav`);
  writeFileSync(file, pcm);
  console.log(`${file}  ${totalSec.toFixed(2)} s  (${arr.bars.length} bars @ ${BPM} BPM)`);
}

const which = process.argv[2];
if (!which || which === "film") render(filmArrangement());
if (!which || which === "preview") render(previewArrangement());
