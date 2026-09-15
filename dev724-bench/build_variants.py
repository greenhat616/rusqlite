"""Build benchmark variants without modifying the working SQLite patch.

The baseline is an isolated archive of the measured upstream commit, with this benchmark harness
copied into it. Both builds use the same locked dependencies and toolchain.
"""
import io
import shutil
import subprocess
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent
OUT = ROOT / "results"
BASE = OUT / "baseline-source"
if any((OUT / "bin" / name).exists() for name in ["unpatched.exe", "patched.exe"]):
    raise SystemExit("Refusing to overwrite benchmark binaries; preserve the existing run first")
if BASE.exists():
    raise SystemExit(f"Refusing to overwrite {BASE}")
BASE.mkdir(parents=True)
baseline_revision = "e88f112bef7899234a497baed5cc3c3d553deeb8"
archive = subprocess.check_output(["git", "archive", baseline_revision], cwd=REPO)
with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
    tar.extractall(BASE, filter="data")
harness = BASE / "dev724-bench"
harness.mkdir()
for name in ["Cargo.toml", "Cargo.lock"]:
    shutil.copy2(ROOT / name, harness / name)
shutil.copytree(ROOT / "src", harness / "src")
(OUT / "bin").mkdir(exist_ok=True)
for source, name in [(harness, "unpatched.exe"), (ROOT, "patched.exe")]:
    subprocess.run(["cargo", "build", "--release", "--locked", "--manifest-path", str(source / "Cargo.toml")], cwd=REPO, check=True, timeout=300)
    shutil.copy2(source / "target" / "release" / "dev724-bench.exe", OUT / "bin" / name)
