"""Score manually labeled per-frame OCR CSV files without external packages."""
import argparse
import csv
import json
import unicodedata
from collections import defaultdict
from pathlib import Path


def distance(a, b):
    previous = list(range(len(b) + 1))
    for i, char in enumerate(a, 1):
        current = [i]
        for j, other in enumerate(b, 1):
            current.append(min(current[-1] + 1, previous[j] + 1,
                               previous[j - 1] + (char != other)))
        previous = current
    return previous[-1]


def normalize(text):
    return " ".join(unicodedata.normalize("NFKC", text).casefold().split())


def score(rows):
    errors = chars = blank = false_positive = comparisons = changes = normalized_changes = 0
    groups = defaultdict(list)
    scorable_cues = set()
    previous = None
    for row in rows:
        if row["phase"] == "ignore":
            if row["cue_id"]:
                groups[row["cue_id"]].append(row)
            previous = None
            continue
        truth, output = row["truth"], row["output"]
        if truth:
            errors += distance(truth, output)
            chars += len(truth)
        else:
            blank += 1
            false_positive += bool(output)
        if previous and row["cue_id"] == previous["cue_id"] and truth == previous["truth"]:
            comparisons += 1
            changes += output != previous["output"]
            normalized_changes += normalize(output) != normalize(previous["output"])
        if row["cue_id"]:
            groups[row["cue_id"]].append(row)
            if truth:
                scorable_cues.add(row["cue_id"])
        previous = row
    delays = []
    for cue_id in sorted(scorable_cues):
        cue_rows = groups[cue_id]
        run = 0
        for row in cue_rows:
            if row["phase"] == "ignore":
                run = 0
                continue
            run = run + 1 if row["truth"] and row["truth"] == row["output"] else 0
            if run == 3:
                delays.append(float(row["seconds"]) - float(row["cue_start_seconds"]))
                break
    ratio = lambda a, b: a / b if b else None
    return {"frames_scored": sum(r["phase"] != "ignore" for r in rows),
            "character_error_rate": ratio(errors, chars), "reference_characters": chars,
            "blank_frames": blank, "blank_false_positive_rate": ratio(false_positive, blank),
            "unchanged_frame_pairs": comparisons, "exact_change_rate": ratio(changes, comparisons),
            "normalized_change_rate": ratio(normalized_changes, comparisons),
            "cues": len(scorable_cues), "cues_stable": len(delays),
            "unstable_cues": len(scorable_cues) - len(delays), "stable_delays_seconds": delays}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("csv", type=Path, help="columns: seconds,cue_id,cue_start_seconds,truth,output,phase")
    args = parser.parse_args()
    with args.csv.open(encoding="utf-8-sig", newline="") as source:
        rows = list(csv.DictReader(source))
    required = {"seconds", "cue_id", "cue_start_seconds", "truth", "output", "phase"}
    if not rows or not required.issubset(rows[0]):
        parser.error("CSV is empty or missing required columns")
    print(json.dumps(score(rows), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

