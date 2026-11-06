"""Score every engine's JSONL in a corpus with ocr_experiment_score plus per-cue jitter metrics.

Normalization is the application's cue-identity rule, so a reported change is one the
application would also see. Transition frames are scored as `ignore`.
Real corpora use cues.csv from ocr_experiment_segment_cues.py plus a hand-written
labels.csv (cue_id,truth; IGNORE for unreadable cues, empty for no subtitle).
--row keeps lines centred in a vertical window; --size-filter keeps only lines at least
60% as tall as the tallest remaining line.
"""
import argparse
import csv
import json
import statistics
from collections import Counter
import sys
import unicodedata
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from ocr_experiment_score import score  # noqa: E402


def subtitle_lines(lines, size_filter, row):
    """Optionally keep lines centred in `row` (lo, hi pixels), then lines at least 60% as
    tall as the tallest (drops small scene text)."""
    if row:
        lines = [line for line in lines if row[0] <= line["cy"] <= row[1]]
    if not size_filter or not lines:
        return [line["text"] for line in lines]
    tallest = max(line["h"] for line in lines)
    return [line["text"] for line in lines if line["h"] >= 0.6 * tallest]


def norm(lines):
    """NFKC, then the application's cue-identity rule (band_cue.rs `normalize`): lowercase,
    keep letters and digits plus '.' or ',' between digits; whitespace and punctuation go."""
    chars = unicodedata.normalize("NFKC", " ".join(lines)).lower()
    return "".join(c for i, c in enumerate(chars)
                   if c.isalnum() or (c in ".," and 0 < i < len(chars) - 1
                                      and chars[i - 1].isdigit() and chars[i + 1].isdigit()))


def load_truth(corpus):
    if (corpus / "truth.csv").exists():
        rows = list(csv.DictReader((corpus / "truth.csv").open(encoding="utf-8")))
        return [{"frame": int(r["frame"]), "seconds": r["seconds"], "cue_id": r["cue_id"],
                 "cue_start_seconds": r["cue_start_seconds"] or "0",
                 "truth": norm([r["truth"]]), "phase": r["phase"]} for r in rows]
    labels = {r["cue_id"]: r["truth"] for r in csv.DictReader((corpus / "labels.csv").open(encoding="utf-8"))}
    rows = list(csv.DictReader((corpus / "cues.csv").open(encoding="utf-8")))
    starts = {}
    for r in rows:
        starts.setdefault(r["cue_id"], r["seconds"])
    out = []
    for r in rows:
        truth = labels.get(r["cue_id"], "") if r["cue_id"] else ""
        phase = "ignore" if truth == "IGNORE" else r["phase"]
        out.append({"frame": int(r["frame"]), "seconds": r["seconds"], "cue_id": r["cue_id"],
                    "cue_start_seconds": starts.get(r["cue_id"], "0"),
                    "truth": "" if truth == "IGNORE" else norm([truth]), "phase": phase})
    return out


def jitter(rows):
    """Per-cue consistency on stable frames of cues with known text."""
    outputs, truth = {}, {}
    for r in rows:
        if r["phase"] == "stable" and r["truth"]:
            outputs.setdefault(r["cue_id"], []).append(r["output"])
            truth[r["cue_id"]] = r["truth"]
    frames = [(o, truth[c]) for c, outs in outputs.items() for o in outs]
    distinct = [len({o for o in outs if o}) for outs in outputs.values()]
    always = sum(all(o == truth[c] for o in outs) for c, outs in outputs.items())
    # What one reading per cue, chosen by majority over its stable frames, would show.
    voted = sum(Counter(outs).most_common(1)[0][0] == truth[c] for c, outs in outputs.items())
    return {"stable_frame_exact": sum(o == t for o, t in frames) / len(frames) if frames else None,
            "cue_vote_exact": f"{voted}/{len(outputs)}",
            "distinct_outputs_per_cue": statistics.mean(distinct) if distinct else None,
            "cues_always_exact": f"{always}/{len(outputs)}"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("corpus", type=Path, help="directory with truth.csv, or cues.csv plus labels.csv")
    parser.add_argument("--size-filter", action="store_true")
    parser.add_argument("--row", help="LO:HI vertical-centre window in pixels, e.g. the known subtitle row")
    args = parser.parse_args()
    corpus, size_filter = args.corpus, args.size_filter
    row = tuple(float(v) for v in args.row.split(":")) if args.row else None
    truth = load_truth(corpus)
    report = {}
    for path in sorted((corpus / "ocr").glob("*.jsonl")):
        recs = [json.loads(line) for line in path.open(encoding="utf-8")]
        if len(recs) < len(truth):
            print(f"skip {path.stem}: {len(recs)}/{len(truth)} frames")
            continue
        if len(recs) > len(truth) or any(t["frame"] != rec["frame"] for t, rec in zip(truth, recs)):
            raise SystemExit(f"{path.name}: frames do not align with the corpus truth")
        rows = []
        for t, rec in zip(truth, recs):
            phase = t["phase"] if t["phase"] in ("stable", "blank") else "ignore"
            rows.append({"seconds": t["seconds"], "cue_id": t["cue_id"], "cue_start_seconds": t["cue_start_seconds"],
                         "truth": t["truth"], "output": norm(subtitle_lines(rec["lines"], size_filter, row)), "phase": phase})
        base = score(rows)
        ms = sorted(r["ms"] for r in recs)
        summary = {
            "CER": base["character_error_rate"], "blank_FP": base["blank_false_positive_rate"],
            "change_rate": base["exact_change_rate"], **jitter(rows),
            "cues_reaching_3_correct": f'{base["cues_stable"]}/{base["cues"]}',
            "ms_p50": ms[len(ms) // 2], "ms_p95": ms[int(len(ms) * 0.95)],
            "errors": sum(bool(r["error"]) for r in recs),
        }
        report[path.stem] = summary
    if not report:
        raise SystemExit("no complete engine output to score")
    keys = list(next(iter(report.values())).keys())
    print("engine".ljust(18) + "".join(k[:14].rjust(15) for k in keys))
    for name, s in report.items():
        cells = [f"{v:.3f}" if isinstance(v, float) else str(v) for v in s.values()]
        print(name.ljust(18) + "".join(c.rjust(15) for c in cells))
    (corpus / f"report{'-row' if row else ''}{'-size' if size_filter else ''}.json").write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")


if __name__ == "__main__":
    main()
