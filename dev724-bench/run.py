"""Run isolated, interleaved benchmarks using prebuilt patched/unpatched binaries."""
import csv
import ctypes
import hashlib
import json
import platform
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent
OUT = ROOT / "results" / "measured"
OUT.mkdir(parents=True, exist_ok=True)
if (OUT / "raw.csv").exists():
    raise SystemExit("Refusing to overwrite an existing measured run")

# Pin this runner and inherited children to one logical processor. Do not
# change the user's global power plan or other applications' affinities.
kernel = ctypes.WinDLL("kernel32", use_last_error=True)
kernel.GetCurrentProcess.restype = ctypes.c_void_p
kernel.SetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
if not kernel.SetProcessAffinityMask(kernel.GetCurrentProcess(), 4):
    raise ctypes.WinError(ctypes.get_last_error())

metadata = {
    "platform": platform.platform(),
    "affinity_mask": 4,
    "started": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
    "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
    "rustc": subprocess.check_output(["rustc", "--version"], cwd=ROOT, text=True).strip(),
    "binaries": {
        name: hashlib.sha256((ROOT / "results" / "bin" / name).read_bytes()).hexdigest()
        for name in ["unpatched.exe", "patched.exe"]
    },
    "source_patch_sha256": hashlib.sha256((ROOT.parent / "libsqlite3-sys" / "sqlite3" / "sqlite3.c").read_bytes()).hexdigest(),
}
(OUT / "environment.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")

def execute(mode, case, repetition, warmup=False):
    exe = "patched.exe" if mode == "patched" else "unpatched.exe"
    stem = f"{case}-{repetition}-{mode}"
    start = time.monotonic()
    result = subprocess.run(
        [str(ROOT / "results" / "bin" / exe), mode, "1", str(OUT / "db"), case],
        capture_output=True, text=True, timeout=300,
    )
    (OUT / f"{stem}.log").write_text(result.stderr, encoding="utf-8")
    if result.returncode:
        print(result.stdout, result.stderr, file=sys.stderr)
        raise SystemExit(result.returncode)
    rows = list(csv.DictReader(result.stdout.splitlines()))
    for row in rows:
        row["rep"] = repetition
    print(f"{stem}: {time.monotonic()-start:.2f}s", flush=True)
    return [] if warmup else rows

for mode in ["baseline", "workaround", "patched"]:
    execute(mode, "smoke", "warmup", True)

all_rows = []
fields = ["mode", "case", "rep", "phase", "ops", "ms"]
with (OUT / "raw.csv").open("w", newline="", encoding="utf-8") as raw:
    writer = csv.DictWriter(raw, fieldnames=fields)
    writer.writeheader()
    for case, repetitions in [("short_wal",5),("short_memory",5),("short_single_wal",5),("medium_wal",3),("long_wal",3)]:
        for rep in range(repetitions):
            modes = ["baseline", "workaround", "patched"]
            modes = modes[rep%3:] + modes[:rep%3]
            for mode in modes:
                rows = execute(mode, case, rep)
                writer.writerows(rows)
                raw.flush()
                all_rows.extend(rows)

groups = {}
for row in all_rows:
    key = row["case"],row["phase"],row["mode"]
    groups.setdefault(key, []).append(float(row["ms"])/int(row["ops"]))
summary = []
for (case,phase,mode), values in sorted(groups.items()):
    summary.append({"case":case,"phase":phase,"mode":mode,"runs":len(values),"median_us_per_op":statistics.median(values)*1000,"min_us_per_op":min(values)*1000,"max_us_per_op":max(values)*1000})
with (OUT / "summary.csv").open("w",newline="",encoding="utf-8") as f:
    writer=csv.DictWriter(f,fieldnames=list(summary[0]))
    writer.writeheader()
    writer.writerows(summary)
print(f"Results saved to {OUT}",flush=True)
