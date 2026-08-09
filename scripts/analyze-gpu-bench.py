#!/usr/bin/env python3
# =============================================================================
# ANALYZE-GPU-BENCH.PY - merge per-arm artifacts into a comparison table
# =============================================================================
# Reads one or more run-summary.json (produced by run-gpu-bench.ps1) plus the
# referenced eval-report.json, server.err.log and gpu-counters.jsonl, and
# prints a per-arm summary plus a Markdown comparison table.
#
# Usage: python analyze-gpu-bench.py <dir1> <dir2> ...   (each dir has
#        run-summary.json)
# =============================================================================
import json, glob, os, re, sys, statistics

REQ_TIME_RE = re.compile(
    r"eval time =\s+([\d.]+) ms /\s*(\d+) tokens \(\s*([\d.]+) ms per token,\s*([\d.]+) tokens per second\)"
)
TOTAL_TIME_RE = re.compile(r"total time =\s+([\d.]+) ms /\s*(\d+) tokens")


def percentile(vals, p):
    if not vals:
        return None
    vals = sorted(vals)
    k = (len(vals) - 1) * p / 100.0
    f = int(k)
    c = f + 1
    if c >= len(vals):
        return vals[-1]
    return vals[f] + (vals[c] - vals[f]) * (k - f)


def parse_server_log(path):
    """Return dict of per-request generation stats parsed from llama-server stderr."""
    gen_tok_s = []
    gen_ms = []
    total_ms = []
    n_gen_tokens = 0
    n_requests = 0
    load_start = load_end = None
    load_start_re = re.compile(r"load_model: loading model")
    model_loaded_re = re.compile(r"llama_server: model loaded")
    if not os.path.exists(path):
        return None
    for line in open(path, encoding="utf-8-sig", errors="replace"):
        m = REQ_TIME_RE.search(line)
        if m:
            gen_ms.append(float(m.group(1)))
            gen_tok_s.append(float(m.group(4)))
            n_gen_tokens += int(m.group(2))
            n_requests += 1
            continue
        m = TOTAL_TIME_RE.search(line)
        if m:
            total_ms.append(float(m.group(1)))
            continue
        if load_start_re.search(line):
            load_start = line.split(" ")[0]
        if model_loaded_re.search(line):
            load_end = line.split(" ")[0]
    return {
        "n_timing_lines": n_requests,
        "gen_tok_s": gen_tok_s,
        "gen_ms": gen_ms,
        "total_ms": total_ms,
        "load_start_ts": load_start,
        "load_end_ts": load_end,
    }


def parse_counters(path):
    """Return per-arm aggregates from the sampler JSONL."""
    if not os.path.exists(path):
        return None
    cpu = []
    gpu_util = []
    gpu_llama = []
    gpu_llama_engtype = set()
    top_instances = []   # "pid:engtype" strings for the busiest instance per sample
    ws = []
    priv = []
    avail = []
    gpu_shared = []
    gpu_dedicated = []
    llama_shared = []
    n = 0
    for line in open(path, encoding="utf-8-sig", errors="replace"):
        line = line.strip()
        if not line:
            continue
        try:
            s = json.loads(line)
        except Exception:
            continue
        n += 1
        if "proc_cpu_pct" in s:
            cpu.append(s["proc_cpu_pct"])
        if "gpu_util_max_pct" in s:
            gpu_util.append(float(s["gpu_util_max_pct"]))
        if any(k.startswith("gpu_util_llama_pid") for k in s):
            for k, v in s.items():
                if k.startswith("gpu_util_llama_pid"):
                    gpu_llama.append(float(v))
        if any(k.startswith("gpu_engine_llama") for k in s):
            for k, v in s.items():
                if k.startswith("gpu_engine_llama"):
                    gpu_llama_engtype.add(v)
        if "gpu_top_instance_by_luid" in s and s["gpu_top_instance_by_luid"]:
            for part in s["gpu_top_instance_by_luid"].split(";"):
                if part.split("=")[0] == "luid_0x00000000_0x00010d66":
                    top_instances.append(part.split("=")[1])
        if "proc_ws_mb" in s:
            ws.append(s["proc_ws_mb"])
        if "proc_priv_mb" in s:
            priv.append(s["proc_priv_mb"])
        if "sys_avail_mb" in s:
            avail.append(s["sys_avail_mb"])
        if "gpu_proc_shared_llama" in s and s["gpu_proc_shared_llama"]:
            for part in s["gpu_proc_shared_llama"].split(";"):
                v = float(part.split("=")[1]) / (1024 * 1024)
                llama_shared.append(v)
        if "gpu_shared_by_luid" in s and s["gpu_shared_by_luid"]:
            vals = [float(p.split("=")[1]) / (1024 * 1024) for p in s["gpu_shared_by_luid"].split(";")]
            gpu_shared.append(max(vals))
        if "gpu_dedicated_by_luid" in s and s["gpu_dedicated_by_luid"]:
            vals = [float(p.split("=")[1]) / (1024 * 1024) for p in s["gpu_dedicated_by_luid"].split(";")]
            gpu_dedicated.append(max(vals))
    agg = {
        "samples": n,
        "cpu_pct": (statistics.mean(cpu), percentile(cpu, 95)) if cpu else (None, None),
        "gpu_util_max_pct": (statistics.mean(gpu_util), percentile(gpu_util, 95), max(gpu_util)) if gpu_util else (None, None, None),
        "gpu_util_llama_pct": (statistics.mean(gpu_llama), percentile(gpu_llama, 95)) if gpu_llama else (None, None),
        "gpu_llama_engtype": sorted(gpu_llama_engtype),
        "top_instance_mode": statistics.mode(top_instances) if top_instances else None,
        "top_instance_count": len(top_instances),
        "ws_mb": (statistics.mean(ws), percentile(ws, 95)) if ws else (None, None),
        "priv_mb": (statistics.mean(priv), percentile(priv, 95)) if priv else (None, None),
        "sys_avail_mb": (statistics.mean(avail), min(avail)) if avail else (None, None),
        "gpu_shared_mb_max": max(gpu_shared) if gpu_shared else None,
        "gpu_dedicated_mb_max": max(gpu_dedicated) if gpu_dedicated else None,
        "llama_gpu_proc_shared_mb_max": max(llama_shared) if llama_shared else None,
    }
    return agg


def summarize_dir(d):
    summary_path = os.path.join(d, "run-summary.json")
    if not os.path.exists(summary_path):
        return None
    s = json.load(open(summary_path, encoding="utf-8-sig"))
    out = {"dir": d, "summary": s}
    report_path = s.get("report_path") or os.path.join(d, "eval-report.json")
    if os.path.exists(report_path):
        r = json.load(open(report_path, encoding="utf-8-sig"))
        lat = [x["latencyMs"] for x in r.get("results", [])]
        failed = [x for x in r.get("results", []) if not x.get("passed")]
        reasons = {}
        for x in failed:
            reasons[x.get("reason")] = reasons.get(x.get("reason"), 0) + 1
        out["report"] = {
            "n": len(lat),
            "p50": percentile(lat, 50),
            "p95": percentile(lat, 95),
            "p99": percentile(lat, 99),
            "max": max(lat) if lat else None,
            "mean": statistics.mean(lat) if lat else None,
            "gt3s": sum(1 for v in lat if v > 3000),
            "gt10s": sum(1 for v in lat if v > 10000),
            "failed": len(failed),
            "fail_reasons": reasons,
            "warmup_ms": r.get("warmupLatencyMs"),
            "warmup_passed": r.get("warmupPassed"),
            "p50_within_budget": r.get("p50WithinBudget"),
            "p95_within_budget": r.get("p95WithinBudget"),
        }
    srv = parse_server_log(os.path.join(d, "server.err.log"))
    if srv and srv["n_timing_lines"] > 0:
        out["server"] = {
            "n": srv["n_timing_lines"],
            "tok_s_p50": percentile(srv["gen_tok_s"], 50),
            "tok_s_p10": percentile(srv["gen_tok_s"], 10),
            "tok_s_min": min(srv["gen_tok_s"]),
            "tok_s_mean": statistics.mean(srv["gen_tok_s"]),
            "gen_ms_p50": percentile(srv["gen_ms"], 50),
            "gen_ms_p95": percentile(srv["gen_ms"], 95),
            "gen_ms_max": max(srv["gen_ms"]),
        }
    cnt = parse_counters(s.get("counters_file") or os.path.join(d, "gpu-counters.jsonl"))
    if cnt:
        out["counters"] = cnt
    return out


def fmt(v, suffix=""):
    if v is None:
        return "-"
    if isinstance(v, float):
        return f"{v:.1f}{suffix}"
    return f"{v}{suffix}"


def main():
    dirs = sys.argv[1:] or ["."]
    arms = []
    for d in dirs:
        a = summarize_dir(d)
        if a:
            arms.append(a)
    if not arms:
        print("no run-summary.json found")
        return
    arms.sort(key=lambda a: a["summary"].get("ngl", 0))

    print("=== PER-ARM DETAILS ===")
    for a in arms:
        s = a["summary"]
        print(f"\n## Arm {s['arm']} -ngl {s['ngl']} ({a['dir']})")
        print(f"  seed={s.get('seed')} threads={s.get('threads')} runs={s.get('runs')} port={s.get('port')}")
        print(f"  eval_exit={s.get('eval_exit_code')} eval_elapsed={s.get('eval_elapsed_s')}s")
        print(f"  health_ready_after_spawn={s.get('health_ready_after_spawn_ms')}ms")
        print(f"  runtime_sha256={s.get('runtime_sha256','')[:12]}")
        r = a.get("report")
        if r:
            print(f"  n={r['n']} p50={fmt(r['p50'],'ms')} p95={fmt(r['p95'],'ms')} p99={fmt(r['p99'],'ms')} "
                  f"max={fmt(r['max'],'ms')} mean={fmt(r['mean'],'ms')}")
            print(f"  >3s={r['gt3s']} >10s={r['gt10s']} failed={r['failed']} warmup={fmt(r['warmup_ms'],'ms')} "
                  f"(budget p50={r['p50_within_budget']}, p95={r['p95_within_budget']})")
            if r["fail_reasons"]:
                print(f"  fail_reasons={r['fail_reasons']}")
        sv = a.get("server")
        if sv:
            print(f"  server: n={sv['n']} tok/s p50={fmt(sv['tok_s_p50'])} p10={fmt(sv['tok_s_p10'])} "
                  f"min={fmt(sv['tok_s_min'])} mean={fmt(sv['tok_s_mean'])}; gen ms p50={fmt(sv['gen_ms_p50'])} "
                  f"p95={fmt(sv['gen_ms_p95'])} max={fmt(sv['gen_ms_max'])}")
        c = a.get("counters")
        if c:
            print(f"  counters: n={c['samples']} cpu% mean/p95={fmt(c['cpu_pct'][0])}/{fmt(c['cpu_pct'][1])} "
                  f"gpu% max mean/p95/max={fmt(c['gpu_util_max_pct'][0])}/{fmt(c['gpu_util_max_pct'][1])}/{fmt(c['gpu_util_max_pct'][2])}")
            print(f"  gpu llama pid util% mean/p95={fmt(c['gpu_util_llama_pct'][0])}/{fmt(c['gpu_util_llama_pct'][1])} "
                  f"engines={','.join(c['gpu_llama_engtype'])}")
            print(f"  top busiest instance (Adreno luid): {c.get('top_instance_mode')} "
                  f"({c.get('top_instance_count')} samples)")
            print(f"  llama proc WS mb mean/p95={fmt(c['ws_mb'][0])}/{fmt(c['ws_mb'][1])} "
                  f"priv mb={fmt(c['priv_mb'][0])}/{fmt(c['priv_mb'][1])}")
            print(f"  sys avail mb mean/min={fmt(c['sys_avail_mb'][0])}/{fmt(c['sys_avail_mb'][1])} "
                  f"gpu shared mb max={fmt(c['gpu_shared_mb_max'])} dedicated max={fmt(c['gpu_dedicated_mb_max'])} "
                  f"llama gpu-proc-shared max={fmt(c['llama_gpu_proc_shared_mb_max'])}")

    print("\n=== COMPARISON ===")
    rows = []
    for a in arms:
        s = a["summary"]
        r = a.get("report") or {}
        sv = a.get("server") or {}
        c = a.get("counters") or {}
        rows.append({
            "arm": f"{s['arm']} (ngl {s['ngl']})",
            "p50": r.get("p50"), "p95": r.get("p95"), "p99": r.get("p99"), "max": r.get("max"),
            "tok_s_p50": sv.get("tok_s_p50"),
            "gt3s": r.get("gt3s"), "gt10s": r.get("gt10s"), "failed": r.get("failed"),
            "n": r.get("n"), "warmup": r.get("warmup_ms"),
            "cpu_mean": c.get("cpu_pct", (None,))[0], "cpu_p95": c.get("cpu_pct", (None, None))[1],
            "gpu_mean": c.get("gpu_util_max_pct", (None,))[0], "gpu_max": c.get("gpu_util_max_pct", (None, None, None))[2],
            "gpu_llama_mean": c.get("gpu_util_llama_pct", (None,))[0],
            "ws_mean": c.get("ws_mb", (None,))[0], "priv_mean": c.get("priv_mb", (None,))[0],
            "avail_min": c.get("sys_avail_mb", (None, None))[1],
            "shared_max": c.get("gpu_shared_mb_max"), "dedicated_max": c.get("gpu_dedicated_mb_max"),
            "startup": s.get("health_ready_after_spawn_ms"),
        })
    headers = [
        ("arm", "Config"), ("n", "N"), ("p50", "p50 ms"), ("p95", "p95 ms"), ("p99", "p99 ms"),
        ("max", "max ms"), ("tok_s_p50", "tok/s p50"), ("gt3s", ">3s"), ("gt10s", ">10s"),
        ("failed", "failed"), ("warmup", "warmup ms"), ("cpu_mean", "CPU% mean"), ("cpu_p95", "CPU% p95"),
        ("gpu_mean", "GPU% mean"), ("gpu_max", "GPU% max"), ("gpu_llama_mean", "GPU% llama mean"),
        ("ws_mean", "WS MB mean"), ("avail_min", "RAM free min MB"),
        ("shared_max", "GPU shared max MB"), ("startup", "startup ms"),
    ]
    print("| " + " | ".join(h for _, h in headers) + " |")
    print("|" + "|".join(["---"] * len(headers)) + "|")
    for row in rows:
        print("| " + " | ".join(fmt(row[k]) for k, _ in headers) + " |")


if __name__ == "__main__":
    main()
