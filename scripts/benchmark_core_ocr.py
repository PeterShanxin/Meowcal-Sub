"""Compare prebuilt Sub 1 OCR examples using identical synthetic BGRA fixtures."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import statistics
import subprocess
import time
from pathlib import Path


def summarize(samples: list[float]) -> dict:
    if not samples or any(not math.isfinite(x) or x < 0 for x in samples):
        raise ValueError("Expected finite nonnegative timing samples")
    ordered = sorted(samples)
    return {
        "count": len(samples),
        "p50Ms": statistics.median(samples),
        "p95Ms": ordered[math.ceil(len(samples) * 0.95) - 1],
        "maxMs": max(samples),
        "over250Rate": sum(x > 250 for x in samples) / len(samples),
    }


def compare(baseline: dict, candidate: dict) -> dict:
    limits = {
        "p50Ms": baseline["p50Ms"] + max(10, baseline["p50Ms"] * 0.15),
        "p95Ms": baseline["p95Ms"] + max(15, baseline["p95Ms"] * 0.20),
        "over250Rate": baseline["over250Rate"] + 0.01,
    }
    return {
        "limits": limits,
        "passed": all(candidate[key] <= limit for key, limit in limits.items()),
    }


def digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--fixtures", type=Path, required=True, help="fixtures.json")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=20, help="Measured calls per ABBA block")
    parser.add_argument("--warmup", type=int, default=3)
    parser.add_argument("--blocks", type=int, default=1, help="Number of ABBA cycles")
    parser.add_argument("--timeout", type=int, default=900, help="Per-process seconds")
    args = parser.parse_args()
    if min(args.runs, args.warmup, args.blocks, args.timeout) < 1:
        parser.error("runs, warmup, blocks and timeout must be positive")
    executables = {"baseline": args.baseline.resolve(strict=True),
                   "candidate": args.candidate.resolve(strict=True)}
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    fixtures = json.loads(args.fixtures.read_text(encoding="utf-8"))
    if {item["name"] for item in fixtures} != {"latin", "large", "color", "chinese", "blank"}:
        raise ValueError("Expected latin, large, color, chinese and blank fixtures")
    report = {
        "machine": {"platform": platform.platform(), "architecture": platform.machine()},
        "executables": {name: {"path": str(path), "sha256": digest(path)}
                        for name, path in executables.items()},
        "coreExecutable": os.environ.get("MEOWCAL_CORE_EXECUTABLE"),
        "runsPerBlock": args.runs, "warmupPerBlock": args.warmup,
        "abbaCycles": args.blocks, "cases": [],
    }
    core = os.environ.get("MEOWCAL_CORE_EXECUTABLE")
    if core:
        report["coreSha256"] = digest(Path(core))
    for fixture in fixtures:
        pixels = args.fixtures.resolve().parent / f"{fixture['name']}.bgra"
        if digest(pixels) != fixture["bgraSha256"]:
            raise ValueError(f"Fixture digest mismatch: {pixels}")
        if pixels.stat().st_size != fixture["width"] * fixture["height"] * 4:
            raise ValueError(f"Fixture byte length mismatch: {pixels}")
        for policy in ("single", "raw", "multi"):
            samples = {name: [] for name in executables}
            outputs = {name: [] for name in executables}
            cold = {name: [] for name in executables}
            raw_files = []
            for block, lane in enumerate(["baseline", "candidate", "candidate", "baseline"] * args.blocks):
                stem = f"{fixture['name']}-{policy}-{block:02d}-{lane}"
                output = args.output / f"{stem}.json"
                command = [str(executables[lane]), str(pixels), str(fixture["width"]),
                           str(fixture["height"]), fixture["language"], policy,
                           str(args.runs), str(args.warmup), str(output)]
                started = time.perf_counter()
                print(stem, flush=True)
                with (args.output / f"{stem}.log").open("w", encoding="utf-8") as log:
                    subprocess.run(command, stdout=log, stderr=subprocess.STDOUT,
                                   check=True, timeout=args.timeout)
                result = json.loads(output.read_text(encoding="utf-8"))
                if len(result["samplesMs"]) != args.runs or len(result["outputs"]) != args.runs:
                    raise ValueError(f"Incomplete measured samples: {output}")
                if len(result["warmupOutputs"]) != args.warmup:
                    raise ValueError(f"Incomplete warmup outputs: {output}")
                samples[lane].extend(result["samplesMs"])
                outputs[lane].extend(result["warmupOutputs"] + result["outputs"])
                cold[lane].append({"initializationMs": result["initializationMs"],
                                   "firstCallMs": result["firstCallMs"],
                                   "processWallMs": (time.perf_counter() - started) * 1000})
                raw_files.append(output.name)
            summaries = {name: summarize(values) for name, values in samples.items()}
            expected = outputs["baseline"][0]
            equivalent = all(value == expected for values in outputs.values() for value in values)
            content_valid = all(bool(value["text"].strip()) == (fixture["name"] != "blank")
                                for values in outputs.values() for value in values)
            budget = compare(summaries["baseline"], summaries["candidate"])
            case = {"fixture": fixture, "policy": "multi3" if policy == "multi" else policy,
                    "baseline": summaries["baseline"], "candidate": summaries["candidate"],
                    "cold": cold, "fullResultEquivalent": equivalent,
                    "contentValid": content_valid, "latencyBudget": budget, "rawFiles": raw_files,
                    "passed": equivalent and content_valid and budget["passed"]}
            report["cases"].append(case)
            (args.output / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False),
                                                     encoding="utf-8")
    report["passed"] = all(case["passed"] for case in report["cases"])
    (args.output / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False),
                                             encoding="utf-8")
    print(json.dumps({"report": str(args.output / "report.json"), "passed": report["passed"]}))
    if not report["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
