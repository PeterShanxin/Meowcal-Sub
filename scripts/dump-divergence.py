import json, sys

BASE = "eval-results/gpu-bench"
ra = json.load(open(f"{BASE}/a-ngl0-20260809-184158/eval-report.json", encoding="utf-8-sig"))
rb = json.load(open(f"{BASE}/b-ngl99-20260809-184254/eval-report.json", encoding="utf-8-sig"))
ds = json.load(open(f"{BASE}/datasets/equivalence-100.json", encoding="utf-8-sig"))
src = {c["id"]: c["sourceText"] for c in ds["cases"]}
ra = {x["caseId"]: x for x in ra["results"]}
rb = {x["caseId"]: x for x in rb["results"]}

def lev(a, b):
    if len(a) < len(b):
        a, b = b, a
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (ca != cb)))
        prev = cur
    return prev[-1]

pairs = []
for cid in sorted(ra):
    a = ra[cid]["output"]
    b = rb[cid]["output"]
    if a != b:
        sim = 1.0 - lev(a, b) / max(len(a), len(b), 1)
        pairs.append({
            "id": cid, "sim": round(sim, 3), "src": src.get(cid, ""),
            "cpu": a, "gpu": b,
            "cpu_passed": ra[cid].get("passed"), "gpu_passed": rb[cid].get("passed"),
            "cpu_reason": ra[cid].get("reason"), "gpu_reason": rb[cid].get("reason"),
        })
pairs.sort(key=lambda p: (p["sim"], p["id"]))
low = [p for p in pairs if p["sim"] < 0.5]
rest = [p for p in pairs if p["sim"] >= 0.5]
# deterministic sample of 20 from the rest: every 2nd by id order (first 20)
sample = rest[::2][:20]
print(f"non-exact total: {len(pairs)}; sim<0.5: {len(low)}; sample from rest: {len(sample)}")
print(f"ids reviewed total: {len(low) + len(sample)}")

with open(f"{BASE}/review/divergence-dump.json", "w", encoding="utf-8") as f:
    json.dump({"low": low, "sample": sample}, f, ensure_ascii=False, indent=2)

for tag, group in [("=== LOW (sim<0.5) ===", low), ("=== SAMPLE (rest) ===", sample)]:
    print(f"\n{tag}")
    for p in group:
        print(f"\n[{p['id']}] sim={p['sim']} cpu_pass={p['cpu_passed']} gpu_pass={p['gpu_passed']}")
        print(f"  SRC: {p['src']!r}")
        print(f"  CPU: {p['cpu']!r}")
        print(f"  GPU: {p['gpu']!r}")
