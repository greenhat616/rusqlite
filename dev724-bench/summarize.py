"""Freeze measurements, verify cross-binary scores, and print comparison tables."""
import ast
import csv
import shutil
import statistics
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent
sys.stdout.reconfigure(encoding="utf-8")
MEASURED = ROOT / "results" / "measured"
SAVED = ROOT / "measurements" / "2026-09-15"
SAVED.mkdir(parents=True, exist_ok=True)
assert (MEASURED / "summary.csv").exists(), "Wait for the runner to finish"
for name in ["raw.csv", "summary.csv", "environment.json"]:
    shutil.copy2(MEASURED / name, SAVED / name)

scores = defaultdict(dict)
for log in MEASURED.glob("*.log"):
    if "warmup" in log.name:
        continue
    for line in log.read_text().splitlines():
        if line.startswith("score,"):
            _, mode, case, _, query, values = line.split(",", 5)
            score = ast.literal_eval(values)
            key = case, query
            if mode in scores[key]:
                assert scores[key][mode] == score, (log, "repetition mismatch")
            scores[key][mode] = score
for key, variants in scores.items():
    assert variants["patched"] == variants["workaround"], (key, "cross-binary mismatch")
print(f"Verified identical top-20 rowids/scores for {len(scores)} case/query combinations in every repetition.")

rows = list(csv.DictReader((MEASURED / "raw.csv").open()))
groups = defaultdict(list)
totals = defaultdict(float)
for r in rows:
    groups[r["case"],r["phase"],r["mode"]].append(float(r["ms"])/int(r["ops"]))
    if r["phase"] in ["insert","replace_same","replace_changed","delete_half"]:
        totals[r["case"],r["mode"],r["rep"]] += float(r["ms"])
aggregate = defaultdict(list)
for (case,mode,rep), value in totals.items():
    aggregate[case,mode].append(value)

def med(case, phase, mode):
    return statistics.median(groups[case,phase,mode])

lines = ["# Benchmark comparison", "", "All mutation values are milliseconds per entire workload; query/statistics values are microseconds per call.", "",
    "| Case | Baseline writes | Workaround writes | Patched writes | Patched vs baseline | Workaround vs patched |", "|---|---:|---:|---:|---:|---:|"]
for case in ["short_wal","short_memory","short_single_wal","medium_wal","long_wal"]:
    b,w,p = [statistics.median(aggregate[case,m]) for m in ["baseline","workaround","patched"]]
    lines.append(f"| {case} | {b:.2f} | {w:.2f} | {p:.2f} | {(p/b-1)*100:+.2f}% | {(w/p-1)*100:+.2f}% |")
lines += ["", "## Per-operation medians and ranges", "", "| Case | Phase | Baseline µs | Workaround µs | Patched µs | Patched min–max µs |", "|---|---|---:|---:|---:|---:|"]
for case in ["short_wal","short_memory","short_single_wal","medium_wal","long_wal"]:
    for phase in ["insert","replace_same","replace_changed","delete_half","stats_read","query_needle","query_common","recovery_scan"]:
        b,w,p=[med(case,phase,m)*1000 for m in ["baseline","workaround","patched"]]
        pv=groups[case,phase,"patched"]
        lines.append(f"| {case} | {phase} | {b:.3f} | {w:.3f} | {p:.3f} | {min(pv)*1000:.3f}–{max(pv)*1000:.3f} |")
text="\n".join(lines)+"\n"
(SAVED / "comparison.md").write_text(text,encoding="utf-8")
print(text)
