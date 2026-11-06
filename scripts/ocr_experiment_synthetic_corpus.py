"""Burn known subtitles onto a real video, re-encode lossy, capture-scale, crop the band.

Produces frames/*.png plus truth.csv (per-frame ground truth) so OCR stability can be
scored without manual labels. Synthetic: real glyph rendering (libass) and real codec
noise, but not a real streaming source.
"""
import argparse
import csv
import json
import subprocess
from pathlib import Path

ZH = [
    "你今天怎么回来得这么早", "我们得在天黑之前赶到山脚下", "这件事我已经考虑了很久",
    "别担心，一切都会好起来的", "他说明天会把钥匙送过来", "你确定这就是我们要找的地方吗",
    "我从来没有见过这么大的雪", "先把东西放下，坐下来慢慢说", "如果你不介意的话，我想留下来",
    "电话一直打不通，我有点着急", "这条路比我想象的还要远", "那天晚上到底发生了什么",
    "我只是想知道真相而已", "我们三个人一起去吧", "请你再给我一次机会",
    "外面下雨了，记得带伞", "这家店的面条特别好吃", "时间不早了，我们该走了",
    "你听我解释，事情不是你想的那样", "谢谢你一直陪在我身边",
]
EN = [
    "Why are you back so early today?", "We have to reach the valley before dark.",
    "I've been thinking about this for a long time.", "Don't worry, everything will be fine.",
    "He said he'd bring the keys over tomorrow.", "Are you sure this is the right place?",
    "I've never seen this much snow in my life.", "Put that down and tell me slowly.",
    "If you don't mind, I'd like to stay.", "Nobody's answering the phone, I'm worried.",
    "This road is longer than I thought.", "What really happened that night?",
    "I just want to know the truth.", "Let's all three of us go together.",
    "Please give me one more chance.", "It's raining outside, take an umbrella.",
    "The noodles here are really good.", "It's getting late, we should go.",
    "Let me explain, it's not what you think.", "Thank you for always being here for me.",
]
FONT = {"zh": "DengXian", "en": "Arial"}
SHOW, GAP, FADE = 2.6, 0.4, 0.15


def ass_time(t):
    h, rem = divmod(t, 3600)
    m, s = divmod(rem, 60)
    return f"{int(h)}:{int(m):02d}:{s:05.2f}"


def build_cues(lines):
    cues, t = [], 0.5
    for i, text in enumerate(lines):
        fade = i % 3 == 0          # every third cue fades
        gap = 0.0 if i % 4 == 1 else GAP   # some cues replace the previous one directly
        cues.append({"id": f"c{i:02d}", "start": t, "end": t + SHOW, "text": text, "fade": fade})
        t += SHOW + gap
    return cues


def write_ass(path, cues, lang):
    head = (
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\n\n"
        "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, "
        "OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, "
        "Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n"
        f"Style: Default,{FONT[lang]},58,&H00FFFFFF,&H00FFFFFF,&H00000000,&H80000000,-1,0,0,0,"
        "100,100,0,0,1,2.5,1.5,2,60,60,70,1\n\n"
        "[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
    )
    body = []
    for c in cues:
        tag = r"{\fad(150,150)}" if c["fade"] else ""
        body.append(f"Dialogue: 0,{ass_time(c['start'])},{ass_time(c['end'])},Default,,0,0,0,,{tag}{c['text']}")
    path.write_text(head + "\n".join(body) + "\n", encoding="utf-8-sig")


def truth_rows(cues, duration, fps, edge):
    rows = []
    for n in range(int(duration * fps)):
        t = round(n / fps, 4)
        active = [c for c in cues if c["start"] <= t < c["end"]]
        if not active:
            near = any(abs(t - c["start"]) < edge or abs(t - c["end"]) < edge for c in cues)
            rows.append([n, t, "", "", "", "ignore" if near else "blank"])
            continue
        c = active[0]
        fade = FADE if c["fade"] else 0.0
        transition = t < c["start"] + fade + edge or t >= c["end"] - fade - edge
        rows.append([n, t, c["id"], c["start"], c["text"], "transition" if transition else "stable"])
    return rows


def main():
    p = argparse.ArgumentParser()
    p.add_argument("background", type=Path)
    p.add_argument("out", type=Path)
    p.add_argument("--lang", choices=["zh", "en"], required=True)
    p.add_argument("--bg-start", type=float, default=0.0)
    p.add_argument("--fps", type=float, default=10.0)
    p.add_argument("--crf", type=int, default=30)
    p.add_argument("--capture-width", type=int, default=1440)
    p.add_argument("--loop", action="store_true", help="loop a background shorter than the corpus")
    args = p.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    cues = build_cues(ZH if args.lang == "zh" else EN)
    duration = cues[-1]["end"] + 1.0
    ass = args.out / "subs.ass"
    write_ass(ass, cues, args.lang)
    burned = args.out / "burned.mp4"
    # Burn at 1080p, then encode lossy like a stream.
    loop = ["-stream_loop", "-1"] if args.loop else []
    run = ["ffmpeg", "-nostdin", "-v", "error", *loop, "-ss", str(args.bg_start), "-t", f"{duration}",
           "-i", str(args.background.resolve()), "-vf", f"scale=1920:1080,subtitles={ass.name}",
           "-an", "-c:v", "libx264", "-preset", "medium", "-crf", str(args.crf), "-pix_fmt", "yuv420p",
           str(burned.name)]
    subprocess.run(run, check=True, cwd=args.out)
    w = args.capture_width
    h = round(w * 1080 / 1920)
    band_y, band_h = round(h * 0.80), round(h * 0.16)
    frames = args.out / "frames"
    frames.mkdir()
    # Downscale like a windowed player, sample at the capture cadence, crop the subtitle band.
    grab = ["ffmpeg", "-nostdin", "-v", "error", "-i", burned.name,
            "-vf", f"fps={args.fps},scale={w}:{h}:flags=bilinear,crop={w}:{band_h}:0:{band_y}",
            str(Path("frames") / "%06d.png")]
    subprocess.run(grab, check=True, cwd=args.out)
    rows = truth_rows(cues, duration, args.fps, edge=1.0 / args.fps)
    count = len(list(frames.glob("*.png")))
    rows = rows[:count]
    with (args.out / "truth.csv").open("w", newline="", encoding="utf-8") as f:
        wr = csv.writer(f)
        wr.writerow(["frame", "seconds", "cue_id", "cue_start_seconds", "truth", "phase"])
        wr.writerows(rows)
    meta = {"lang": args.lang, "background": str(args.background), "bg_start": args.bg_start, "loop": args.loop,
            "fps": args.fps, "crf": args.crf, "capture": [w, h], "band": [band_y, band_h],
            "frames": count, "cues": len(cues), "duration": duration}
    (args.out / "manifest.json").write_text(json.dumps(meta, indent=2), encoding="utf-8")
    print(json.dumps(meta))


if __name__ == "__main__":
    main()
