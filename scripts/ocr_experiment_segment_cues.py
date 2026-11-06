"""Segment a real-subtitle corpus into cues from white-glyph pixels; no text truth needed.

A frame's signature is the white-text mask (min channel >= 200, spread <= 40, the same
rule the app's glyph mask uses) next to a dark outline. Consecutive frames whose masks overlap (IoU >= 0.6)
belong to one cue; near-empty masks are blank. Frames at a cue's first/last sample
are marked transition. Frame times come from frames.csv when present, otherwise from
an FPS argument (default 10). Writes cues.csv and a contact sheet of one frame per cue.
"""
import csv
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

MIN_PIXELS = 150
SAME_CUE_IOU = 0.6


def dilate(m, r):
    padded = np.pad(m, r)
    out = m.copy()
    h, w = m.shape
    for dy in range(2 * r + 1):
        for dx in range(2 * r + 1):
            out |= padded[dy:dy + h, dx:dx + w]
    return out


def mask(path):
    # White glyph pixels that sit next to a dark outline/shadow; plain white scenery is dropped.
    a = np.asarray(Image.open(path).convert("RGB")).astype(np.int16)
    white = (a.min(axis=2) >= 200) & ((a.max(axis=2) - a.min(axis=2)) <= 40)
    dark = a.max(axis=2) <= 80
    return white & dilate(dark, 3)


def main():
    corpus = Path(sys.argv[1])
    frames = sorted((corpus / "frames").glob("*.png"))
    timing = corpus / "frames.csv"   # written by ocr_experiment_extract_frames.py
    if timing.exists():
        seconds = [float(r["source_seconds"]) for r in csv.DictReader(timing.open(encoding="utf-8"))]
    else:
        fps = float(sys.argv[2]) if len(sys.argv) > 2 else 10.0
        seconds = [i / fps for i in range(len(frames))]
    masks = [mask(p) for p in frames]
    cue_of, cue, prev = [], -1, None
    for m in masks:
        if m.sum() < MIN_PIXELS:
            cue_of.append(None)
            prev = None
            continue
        if prev is None or (m & prev).sum() / max((m | prev).sum(), 1) < SAME_CUE_IOU:
            cue += 1
        cue_of.append(cue)
        prev = m
    rows = []
    for i, c in enumerate(cue_of):
        if c is None:
            phase = "blank"
        else:
            edge = (i == 0 or cue_of[i - 1] != c) or (i + 1 == len(cue_of) or cue_of[i + 1] != c)
            phase = "transition" if edge else "stable"
        rows.append([i, round(seconds[i], 3), "" if c is None else f"r{c:03d}", phase])
    with (corpus / "cues.csv").open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["frame", "seconds", "cue_id", "phase"])
        w.writerows(rows)
    # Contact sheet: middle frame of each cue, labelled with its cue id.
    picks = {}
    for i, c in enumerate(cue_of):
        if c is not None:
            picks.setdefault(c, []).append(i)
    tiles = [(c, Image.open(frames[ids[len(ids) // 2]]).convert("RGB")) for c, ids in sorted(picks.items())]
    if tiles:
        w, h = tiles[0][1].size
        sheet = Image.new("RGB", (w + 70, h * len(tiles)), "black")
        draw = ImageDraw.Draw(sheet)
        for k, (c, im) in enumerate(tiles):
            sheet.paste(im, (70, k * h))
            draw.text((4, k * h + h // 3), f"r{c:03d}", fill="yellow")
        sheet.save(corpus / "cues_sheet.png")
    print(f"frames={len(frames)} cues={cue + 1} blank={sum(c is None for c in cue_of)}")


if __name__ == "__main__":
    main()
