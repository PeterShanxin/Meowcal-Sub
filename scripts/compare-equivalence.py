#!/usr/bin/env python3
# =============================================================================
# COMPARE-EQUIVALENCE.PY - A vs B output-equivalence check on a shared dataset
# =============================================================================
# Compares the per-case outputs of two eval reports (same dataset, same seed,
# same decoding; only -ngl differs). Quantifies exact-match rate, edit
# distance, and flags malformed/truncated/repetitive/language-drift outputs.
#
# Usage: python compare-equivalence.py <reportA.json> <reportB.json> <targetLang>
#   targetLang: zh|en (used for script-presence checks)
# =============================================================================
import json, re, sys

CJK_RE = re.compile(r"[\u3400-\u4DBF\u4E00-\u9FFF\uF900-\uFAFF\u3040-\u30FF\uAC00-\uD7AF]")
LATIN_RE = re.compile(r"[A-Za-z]")


def levenshtein(a, b):
    if len(a) < len(b):
        a, b = b, a
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (ca != cb)))
        prev = cur
    return prev[-1]


def load(path):
    r = json.load(open(path, encoding="utf-8-sig"))
    return {x["caseId"]: x for x in r.get("results", [])}


def flag(text, target):
    flags = []
    if not text:
        flags.append("EMPTY")
    if target == "zh" and not CJK_RE.search(text):
        flags.append("NO_CJK")
    if target == "en" and not LATIN_RE.search(text):
        flags.append("NO_LATIN")
    # repetition heuristic: same 3+ char n-gram repeating >= 3 times
    grams = [text[i:i + 3] for i in range(len(text) - 2)]
    for g in set(grams):
        if len(g) == len(g.strip()) and grams.count(g) >= 4 and not g.strip().isascii():
            flags.append("REPETITION")
            break
    if len(text) > 200:
        flags.append("TRUNCATION_LIKE")
    return flags


def main():
    if len(sys.argv) < 4:
        print("usage: compare-equivalence.py <reportA.json> <reportB.json> <zh|en>")
        return
    ra = load(sys.argv[1])
    rb = load(sys.argv[2])
    target = sys.argv[3]
    ids = sorted(set(ra) & set(rb))
    exact = 0
    sims = []
    diffs = []
    flagged_a = []
    flagged_b = []
    for cid in ids:
        a = ra[cid]["output"]
        b = rb[cid]["output"]
        if a == b:
            exact += 1
            continue
        dist = levenshtein(a, b)
        sim = 1.0 - dist / max(len(a), len(b), 1)
        sims.append((cid, sim, a, b))
        fa = flag(a, target)
        fb = flag(b, target)
        if fa:
            flagged_a.append((cid, fa, a))
        if fb:
            flagged_b.append((cid, fb, b))
        # material divergence heuristic: very different length or low similarity
        if sim < 0.5:
            diffs.append((cid, sim, a, b))
    print(f"cases compared: {len(ids)}")
    print(f"exact string match: {exact}/{len(ids)} ({100.0*exact/max(len(ids),1):.1f}%)")
    if sims:
        sims_sorted = sorted(sims, key=lambda t: t[1])
        print(f"non-exact: {len(sims)}; similarity min/median/max = "
              f"{sims_sorted[0][1]:.3f}/{sims_sorted[len(sims_sorted)//2][1]:.3f}/{sims_sorted[-1][1]:.3f}")
    print(f"outputs flagged in A: {len(flagged_a)}")
    for cid, f, out in flagged_a[:10]:
        print(f"  A {cid}: {f} -> {out[:80]!r}")
    print(f"outputs flagged in B: {len(flagged_b)}")
    for cid, f, out in flagged_b[:10]:
        print(f"  B {cid}: {f} -> {out[:80]!r}")
    print(f"material divergences (sim<0.5): {len(diffs)}")
    for cid, sim, a, b in diffs[:15]:
        print(f"  {cid}: sim={sim:.2f}\n    A: {a[:100]!r}\n    B: {b[:100]!r}")


if __name__ == "__main__":
    main()
