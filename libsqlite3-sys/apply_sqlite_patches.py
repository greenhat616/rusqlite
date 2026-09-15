#!/usr/bin/env python3
"""Maintain the local SQLite patch series without changing the Git index."""

import argparse
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("apply", "check", "revert"))
    args = parser.parse_args()
    package = Path(__file__).resolve().parent
    root = package.parent
    patches = sorted((package / "patches").glob("*.patch"))
    if not patches:
        parser.error("No SQLite patches found in libsqlite3-sys/patches")

    def run(reverse=False, check=False):
        command = ["git", "apply", "--whitespace=nowarn"]
        if reverse:
            command.append("--reverse")

        def series(directory):
            for patch in (reversed(patches) if reverse else patches):
                result = subprocess.run(command + [str(patch)], cwd=directory,
                                        capture_output=True)
                if result.returncode:
                    return result
            return result

        if not check:
            return series(root)
        # Git's multi-file --check does not model dependent patches to the
        # same file. Replay the complete series on a disposable source copy.
        with tempfile.TemporaryDirectory(prefix="sqlite-patch-check-") as temp:
            shutil.copytree(package / "sqlite3", Path(temp) / "libsqlite3-sys" / "sqlite3")
            return series(temp)

    try:
        applied = run(reverse=True, check=True)
        if applied.returncode == 0:
            if args.action != "revert":
                print("SQLite patches are already applied.")
                return 0
            result = run(reverse=True)
        else:
            pending = run(check=True)
            if pending.returncode != 0:
                print("SQLite patch conflict or partially applied series; no files changed.\n"
                      "Review the patches against the current SQLite source before retrying.", file=sys.stderr)
                sys.stderr.buffer.write(pending.stderr)
                return 1
            if args.action == "check":
                print("SQLite patches are missing. Run apply_sqlite_patches.py apply.", file=sys.stderr)
                return 1
            if args.action == "revert":
                print("SQLite patches are already reverted.")
                return 0
            result = run()
    except FileNotFoundError:
        print("Git is required to maintain SQLite patches.", file=sys.stderr)
        return 1

    if result.returncode:
        sys.stderr.buffer.write(result.stderr)
        return result.returncode
    print(f"SQLite patches {'reverted' if args.action == 'revert' else 'applied'} ({len(patches)}).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
