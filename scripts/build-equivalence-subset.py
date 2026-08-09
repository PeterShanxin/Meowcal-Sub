#!/usr/bin/env python3
"""Build a deterministic ~100-line en-US->zh-CN equivalence subset from the
real-session s3 dataset, with a fixed stride so selection is reproducible."""
import json, sys

SRC = "eval-results/bench/datasets/s3_2026-08-05_22-00-04.json"
OUT = "eval-results/gpu-bench/datasets/equivalence-100.json"
N = 100

d = json.load(open(SRC, encoding="utf-8"))
cases = [c for c in d["cases"] if c.get("expectedAction") == "translate"]
cases.sort(key=lambda c: c["id"])
step = max(1, len(cases) // N)
selected = cases[::step][:N]
# force stable field order / minimal shape
sel = [
    {
        "id": c["id"],
        "sourceLanguage": c["sourceLanguage"],
        "targetLanguage": c["targetLanguage"],
        "sourceText": c["sourceText"],
        "tags": ["equivalence"] + list(c.get("tags", [])),
        "expectedAction": "translate",
        "acceptableOutputs": [],
        "maxOutputLines": c.get("maxOutputLines", 1),
    }
    for c in selected
]
out = {
    "schemaVersion": d.get("schemaVersion", 1),
    "dataset_id": "equivalence-100-from-s3",
    "datasetId": "equivalence-100-from-s3",
    "provenance": "deterministic stride subset of s3_2026-08-05_22-00-04 for GPU/CPU output-equivalence check",
    "allowUnreferencedCases": True,
    "rejectionFixtures": [],
    "cases": sel,
}
import os
os.makedirs(os.path.dirname(OUT), exist_ok=True)
json.dump(out, open(OUT, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
lens = sorted(len(c["sourceText"]) for c in sel)
print(f"wrote {OUT}: {len(sel)} cases, src len min/med/max = {lens[0]}/{lens[len(lens)//2]}/{lens[-1]}")
