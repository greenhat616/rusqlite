"""Exercise the patch CLI in isolated repositories, never the working source."""

import difflib
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


class PatchMaintenanceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="sqlite patch test ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.package = self.root / "libsqlite3-sys"
        (self.package / "patches").mkdir(parents=True)
        (self.package / "sqlite3").mkdir()
        self.script = self.package / "apply_sqlite_patches.py"
        shutil.copy2(Path(__file__).resolve().parents[1] / self.script.name, self.script)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        self.source = self.package / "sqlite3" / "sqlite3.c"
        self.original = "first\nsecond\nthird\n"
        self.changed = "first\nfixed\nthird\n"
        self.source.write_text(self.original, encoding="utf-8", newline="")
        self.patch("0001.patch", self.original, self.changed)

    def patch(self, name, before, after, path="libsqlite3-sys/sqlite3/sqlite3.c"):
        diff = "".join(difflib.unified_diff(before.splitlines(True), after.splitlines(True),
                                            fromfile="a/" + path, tofile="b/" + path))
        (self.package / "patches" / name).write_text(diff, encoding="utf-8", newline="")

    def cli(self, action, success=True):
        result = subprocess.run([sys.executable, str(self.script), action],
                                cwd=self.root.parent, capture_output=True)
        self.assertEqual(result.returncode == 0, success, result.stderr.decode(errors="replace"))
        return result

    def test_apply_check_revert_are_idempotent_and_do_not_stage(self):
        self.cli("check", False)
        self.assertEqual(self.source.read_text(), self.original)
        self.cli("apply")
        self.assertEqual(self.source.read_text(), self.changed)
        for action in ["check", "apply"]:
            before = self.source.stat().st_mtime_ns
            self.cli(action)
            self.assertEqual(self.source.stat().st_mtime_ns, before)
        self.cli("revert")
        self.assertEqual(self.source.read_text(), self.original)
        self.cli("revert")
        result = subprocess.check_output(["git", "ls-files", "--stage"], cwd=self.root)
        self.assertEqual(result, b"")

    def test_dependent_patches_apply_in_order_and_revert_in_reverse(self):
        final = "first\nfixed again\nthird\n"
        self.patch("0002.patch", self.changed, final)
        self.cli("apply")
        self.assertEqual(self.source.read_text(), final)
        self.cli("check")
        self.cli("apply")
        self.cli("revert")
        self.assertEqual(self.source.read_text(), self.original)

    def test_conflict_in_later_patch_leaves_all_sources_unchanged(self):
        other = self.package / "sqlite3" / "sqlite3.h"
        other.write_bytes(b"unexpected upstream change\n")
        self.patch("0002.patch", "old header\n", "new header\n", "libsqlite3-sys/sqlite3/sqlite3.h")
        for action in ["apply", "check", "revert"]:
            self.cli(action, False)
            self.assertEqual(self.source.read_text(), self.original)
            self.assertEqual(other.read_bytes(), b"unexpected upstream change\n")
        self.assertFalse(list(self.root.rglob("*.rej")))

    def test_partial_series_fails_without_touching_source(self):
        self.patch("0002.patch", self.changed, "first\nfixed again\nthird\n")
        self.source.write_text(self.changed, encoding="utf-8", newline="")
        for action in ["apply", "check", "revert"]:
            self.cli(action, False)
            self.assertEqual(self.source.read_text(), self.changed)

    def test_empty_patch_directory_fails(self):
        (self.package / "patches" / "0001.patch").unlink()
        self.cli("apply", False)
        self.cli("check", False)


if __name__ == "__main__":
    unittest.main()
