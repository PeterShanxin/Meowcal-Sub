#!/usr/bin/env python3
"""Analyze a meowcal-sub app log for gate evidence: per-frame model_ms
distribution, outage/defer behavior, translation counts."""
import re, sys, statistics
from datetime import datetime

TS_RE = re.compile(r"^\s*(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+)Z")
FRAME_RE = re.compile(
    r"pipeline_frame_complete, session_id: (\d+), capture_id: (\d+), "
    r"capture_ms: (\d+), ocr_ms: (\d+), model_ms: (\d+), overlay_ms: (\d+), total_ms: (\d+)"
)
TRANS_RE = re.compile(r"Translated, source: (.*?), translated: (.*)$")
REQUEST_RE = re.compile(r"Translation request, source_chars: (\d+), source_lang: (\w+), target_lang: (\w+)")
DEFER_RE = re.compile(r"DEFER|defer")

def parse_ts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00"))

def main(path):
    model_ms = []
    ocr_ms = []
    total_ms = []
    translates = []
    requests = []
    defers = 0
    t_last_translate = None
    outages = []  # (seconds, started_at) gaps between successful translations
    for line in open(path, encoding="utf-8", errors="replace"):
        m = FRAME_RE.search(line)
        if m:
            model_ms.append(int(m.group(5)))
            ocr_ms.append(int(m.group(4)))
            total_ms.append(int(m.group(7)))
            continue
        m = TRANS_RE.search(line)
        if m:
            ts = TS_RE.match(line)
            if ts:
                t = parse_ts(ts.group(1))
                if t_last_translate is not None:
                    gap = (t - t_last_translate).total_seconds()
                    if gap > 2.0:
                        outages.append((gap, str(t_last_translate.time())))
                t_last_translate = t
            translates.append(m.group(2))
            continue
        if REQUEST_RE.search(line):
            requests.append(1)
            continue
        if DEFER_RE.search(line):
            defers += 1

    def pct(vals, p):
        if not vals:
            return None
        sv = sorted(vals)
        return sv[min(int(len(sv) * p), len(sv) - 1)]

    print(f"frames: {len(model_ms)}  translates: {len(translates)}  requests: {len(requests)}  defers: {defers}")
    if model_ms:
        print(f"model_ms: p50={pct(model_ms,.5)} p95={pct(model_ms,.95)} p99={pct(model_ms,.99)} "
              f"max={max(model_ms)} mean={statistics.mean(model_ms):.0f} "
              f">3s={sum(1 for v in model_ms if v>3000)} >10s={sum(1 for v in model_ms if v>10000)}")
    if ocr_ms:
        print(f"ocr_ms: p50={pct(ocr_ms,.5)} p95={pct(ocr_ms,.95)} max={max(ocr_ms)}")
    if total_ms:
        print(f"total_ms: p50={pct(total_ms,.5)} p95={pct(total_ms,.95)} max={max(total_ms)}")
    if outages:
        big = [o for o in outages if o[0] > 5]
        print(f"translation gaps >2s: {len(outages)}; >5s: {len(big)}; "
              f"longest: {max(outages)[0]:.1f}s at {max(outages)[1]}")
        print(f"  gaps >5s: {[(round(g,1), str(t)) for g,t in big[:12]]}")
    else:
        print("translation gaps: none (>2s)")
    # slot-blocked time proxy (model_ms > 3000)
    if model_ms:
        blocked = sum(v for v in model_ms if v > 3000) / 1000.0
        print(f"slot blocked by >3s calls: {blocked:.1f}s total")

if __name__ == "__main__":
    main(sys.argv[1])
