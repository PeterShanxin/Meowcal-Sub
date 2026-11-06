"""Extract a selected local clip into lossless consecutive frames and timing data."""
import argparse
import hashlib
import json
import csv
import re
import subprocess
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("video", type=Path)
    parser.add_argument("output", type=Path, help="new empty output directory")
    parser.add_argument("--start", type=float, required=True, help="seconds from start")
    parser.add_argument("--duration", type=float, required=True, help="seconds")
    parser.add_argument("--crop", help="optional language ROI as width:height:x:y")
    args = parser.parse_args()
    if args.start < 0 or args.duration <= 0:
        parser.error("start must be nonnegative and duration positive")
    if not args.video.is_file():
        parser.error("video does not exist")
    args.output.mkdir(parents=True, exist_ok=False)
    frames = args.output / "frames"
    frames.mkdir()
    digest = hashlib.sha256()
    with args.video.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    # Decode from the beginning so selection does not depend on keyframe seeking.
    if args.crop and not re.fullmatch(r"\d+:\d+:\d+:\d+", args.crop):
        parser.error("crop must be width:height:x:y, using nonnegative integers")
    filters = f"trim=start={args.start}:duration={args.duration}"
    if args.crop:
        filters += f",crop={args.crop}"
    command = ["ffmpeg", "-nostdin", "-v", "info", "-i", str(args.video),
               "-vf", filters + ",showinfo",
               "-fps_mode", "passthrough", str(frames / "%08d.png")]
    run = subprocess.run(command, capture_output=True, text=True, encoding="utf-8", errors="replace")
    (args.output / "ffmpeg.log").write_text(run.stderr, encoding="utf-8")
    matches = re.findall(r"\bn:\s*(\d+)\s+pts:\s*(-?\d+)\s+pts_time:\s*([\d.eE+-]+)", run.stderr)
    with (args.output / "frames.csv").open("w", newline="", encoding="utf-8") as output:
        writer = csv.writer(output)
        writer.writerow(["file", "source_pts", "source_seconds"])
        for number, pts, seconds in matches:
            writer.writerow([f"{int(number) + 1:08d}.png", pts, seconds])
    metadata = {"source": str(args.video.resolve()), "sha256": digest.hexdigest(),
                "start_seconds": args.start, "duration_seconds": args.duration,
                "crop": args.crop,
                "command": command, "returncode": run.returncode,
                "frame_count": len(list(frames.glob("*.png")))}
    (args.output / "manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
    if run.returncode:
        raise SystemExit("ffmpeg failed; inspect ffmpeg.log")
    if len(matches) != metadata["frame_count"] or not matches:
        raise SystemExit("frame/timestamp mismatch or empty selection; inspect ffmpeg.log")
    print(json.dumps(metadata, indent=2))


if __name__ == "__main__":
    main()

