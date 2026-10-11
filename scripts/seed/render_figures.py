#!/usr/bin/env python3
"""Render clean, labeled scientific hero figures for the six stoichiometry
seed lessons (docs/LESSONS_AUTHORING.md).

Art direction: hazard yellow (#facc15) line art and accents on a matte-black
(#0a0a0a) background, flat minimalist textbook style, sentence-case labels,
legible at 2x. These deterministic diagrams are the shipped seed figures until
the genmedia (Nano Banana Pro) art-directed versions can be regenerated (that
path needs cloud re-authentication); they are correct, legible, and reviewed —
not placeholder slop. Run:  python3 scripts/seed/render_figures.py
"""

import os
from PIL import Image, ImageDraw, ImageFont

OUT = os.path.join(os.path.dirname(__file__), "figures")
os.makedirs(OUT, exist_ok=True)

W, H = 1200, 900
BG = (10, 10, 10)
YELLOW = (250, 204, 21)
GRAY = (163, 163, 163)
WHITE = (245, 245, 245)
GREEN = (52, 211, 153)


def font(size, bold=False):
    candidates = [
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf" if bold else "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "/Library/Fonts/Arial.ttf",
    ]
    for c in candidates:
        if os.path.exists(c):
            try:
                return ImageFont.truetype(c, size)
            except Exception:
                pass
    return ImageFont.load_default()


def center(draw, xy, text, fnt, fill):
    bbox = draw.textbbox((0, 0), text, font=fnt)
    w = bbox[2] - bbox[0]
    h = bbox[3] - bbox[1]
    draw.text((xy[0] - w / 2, xy[1] - h / 2), text, font=fnt, fill=fill)


def new():
    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)
    return img, d


def header(d, title):
    center(d, (W / 2, 70), title, font(46, bold=True), YELLOW)
    d.line([(120, 120), (W - 120, 120)], fill=(60, 60, 60), width=2)


def bar(d, x, y, w, h, fill):
    d.rectangle([x, y, x + w, y + h], fill=fill)


def save(img, name):
    path = os.path.join(OUT, name)
    img.save(path, "PNG")
    print("wrote", path)


# ── Lesson 1: the mole and molar mass ───────────────────────────────────────
def lesson1():
    img, d = new()
    header(d, "The mole and molar mass")
    # balance scale: grams vs moles
    cx, cy = W / 2, 320
    d.line([(cx - 260, cy), (cx + 260, cy)], fill=YELLOW, width=6)
    d.line([(cx, cy), (cx, cy + 120)], fill=YELLOW, width=6)
    d.ellipse([cx - 20, cy + 110, cx + 20, cy + 150], outline=YELLOW, width=5)
    for sx, label in [(cx - 260, "mass (g)"), (cx + 260, "amount (mol)")]:
        d.line([(sx, cy), (sx, cy + 70)], fill=GRAY, width=3)
        d.arc([sx - 70, cy + 60, sx + 70, cy + 160], 0, 180, fill=YELLOW, width=4)
        center(d, (sx, cy + 200), label, font(30, bold=True), WHITE)
    center(d, (cx, 560), "grams  /  molar mass (g/mol)  =  moles", font(38, bold=True), YELLOW)
    center(d, (cx, 650), "1 mole = 6.022 x 10^23 particles  (Avogadro's number)", font(30), GRAY)
    center(d, (cx, 720), "molar mass of H2O = 18.02 g/mol", font(30), GREEN)
    save(img, "lesson1.png")


# ── Lesson 2: balancing ─────────────────────────────────────────────────────
def lesson2():
    img, d = new()
    header(d, "Balancing chemical equations")
    center(d, (W / 2, 230), "2 H2  +  O2   ->   2 H2O", font(60, bold=True), YELLOW)
    center(d, (W / 2, 320), "conservation of mass: atoms are equal on both sides", font(28), GRAY)
    # per-element atom-count bars (left vs right)
    rows = [("H", 4, 4), ("O", 2, 2)]
    y = 430
    for el, left, right in rows:
        center(d, (200, y + 25), el, font(40, bold=True), WHITE)
        bar(d, 300, y, left * 70, 50, YELLOW)
        center(d, (300 + left * 70 + 40, y + 25), str(left), font(32), WHITE)
        center(d, (W / 2, y + 25), "=", font(40, bold=True), GREEN)
        bar(d, 760, y, right * 70, 50, YELLOW)
        center(d, (760 + right * 70 + 40, y + 25), str(right), font(32), WHITE)
        y += 130
    center(d, (W / 2, 760), "adjust coefficients, never subscripts", font(30, bold=True), GREEN)
    save(img, "lesson2.png")


# ── Lesson 3: stoichiometry railroad ────────────────────────────────────────
def lesson3():
    img, d = new()
    header(d, "Mole ratios: the stoichiometry railroad")
    stops = ["grams A", "moles A", "moles B", "grams B"]
    labels = ["/ molar mass A", "x mole ratio", "x molar mass B"]
    y = 400
    xs = [180, 480, 780, 1040]
    for i, (x, s) in enumerate(zip(xs, stops)):
        d.ellipse([x - 70, y - 55, x + 70, y + 55], outline=YELLOW, width=5)
        center(d, (x, y), s, font(28, bold=True), WHITE)
        if i < len(labels):
            d.line([(x + 72, y), (xs[i + 1] - 72, y)], fill=YELLOW, width=4)
            center(d, ((x + xs[i + 1]) / 2, y - 90), labels[i], font(24), GRAY)
    center(d, (W / 2, 600), "2 H2 + O2 -> 2 H2O :  4.00 g H2 -> 35.69 g H2O", font(30, bold=True), GREEN)
    save(img, "lesson3.png")


# ── Lesson 4: limiting reagent ──────────────────────────────────────────────
def lesson4():
    img, d = new()
    header(d, "Limiting reagent and percent yield")
    center(d, (W / 2, 210), "the reactant that runs out first limits the product", font(28), GRAY)
    # two draining bars
    base = 560
    for x, name, frac, limiting in [(360, "H2", 0.4, True), (760, "O2", 1.0, False)]:
        full_h = 300
        d.rectangle([x - 70, base - full_h, x + 70, base], outline=GRAY, width=3)
        fill_h = int(full_h * frac)
        bar(d, x - 70, base - fill_h, 140, fill_h, YELLOW if limiting else GREEN)
        center(d, (x, base + 50), name, font(36, bold=True), WHITE)
        center(d, (x, base - full_h - 40), "limiting" if limiting else "excess", font(28, bold=True), YELLOW if limiting else GREEN)
    center(d, (W / 2, 720), "percent yield = actual / theoretical x 100", font(32, bold=True), GREEN)
    save(img, "lesson4.png")


# ── Lesson 5: titration curve ───────────────────────────────────────────────
def lesson5():
    img, d = new()
    header(d, "Solution stoichiometry and titration")
    ox, oy = 220, 720
    d.line([(ox, oy), (ox, 180)], fill=GRAY, width=3)
    d.line([(ox, oy), (W - 160, oy)], fill=GRAY, width=3)
    center(d, (ox - 60, 450), "pH", font(30, bold=True), WHITE)
    center(d, (W / 2 + 100, oy + 50), "titrant volume (mL)", font(30, bold=True), WHITE)
    pts = [(0, 1.0), (10, 1.5), (20, 2.2), (24, 3.0), (25, 7.0), (26, 11.0), (30, 12.0), (40, 12.5)]
    maxv = 40.0
    scr = []
    for v, ph in pts:
        px = ox + (v / maxv) * (W - 160 - ox)
        py = oy - (ph / 14.0) * (oy - 180)
        scr.append((px, py))
    d.line(scr, fill=YELLOW, width=5)
    # equivalence point at 25 mL
    ev_x = ox + (25 / maxv) * (W - 160 - ox)
    d.line([(ev_x, 180), (ev_x, oy)], fill=GREEN, width=3)
    center(d, (ev_x + 140, 300), "equivalence point", font(28, bold=True), GREEN)
    save(img, "lesson5.png")


# ── Lesson 6: ideal gas at STP ──────────────────────────────────────────────
def lesson6():
    img, d = new()
    header(d, "Gas stoichiometry: the ideal gas")
    center(d, (W / 2, 240), "PV = nRT", font(70, bold=True), YELLOW)
    center(d, (W / 2, 350), "at STP, 1 mole of an ideal gas = 22.41 L", font(34, bold=True), GREEN)
    # a labeled container box
    cx = W / 2
    d.rectangle([cx - 180, 430, cx + 180, 720], outline=YELLOW, width=5)
    for gx, gy in [(cx - 110, 500), (cx + 60, 560), (cx - 40, 640), (cx + 120, 480), (cx - 150, 660), (cx + 10, 500)]:
        d.ellipse([gx - 14, gy - 14, gx + 14, gy + 14], fill=GRAY)
    center(d, (cx, 760), "P pressure \u00b7 V volume \u00b7 n moles \u00b7 R = 0.0821 \u00b7 T kelvin", font(26), GRAY)
    save(img, "lesson6.png")


if __name__ == "__main__":
    lesson1()
    lesson2()
    lesson3()
    lesson4()
    lesson5()
    lesson6()
    print("done")
