# Local SQLite patches

Keep the bundled `sqlite3/sqlite3.c` patched in Git so ordinary Cargo builds
need neither Python nor Git. The numbered `.patch` files in this directory
record the local changes independently of the upstream amalgamation.

## Commands

From the repository root (the script also works from other directories):

```sh
python libsqlite3-sys/apply_sqlite_patches.py apply
python libsqlite3-sys/apply_sqlite_patches.py check
python libsqlite3-sys/apply_sqlite_patches.py revert
```

Requires Python 3 and Git. `apply` and `revert` are idempotent. `check` is
read-only and succeeds only when the entire series is already applied.
Patches are applied in filename order and reverted in reverse order. The
script preflights the entire series on a temporary SQLite source copy before
changing the working tree; conflicts and partially
applied series fail instead of skipping a patch or producing `.rej` files.
Commands never stage files or change the Git index.

## Upgrading SQLite

`libsqlite3-sys/upgrade.sh` applies this series after extracting the upstream
amalgamation and before generating bindings or building. It uses `python3`
by default; set `PYTHON=python` if needed on Windows/Git Bash. CI checks the
resulting bundled source on Windows, Linux, and macOS.

If an upgrade conflicts, review whether upstream fixed the issue or changed
the affected code. Remove an obsolete patch or refresh it against the new
upstream source, then apply and rerun its regression tests. If the final patch
is retired, also retire the script's upgrade/CI hooks. A patch applying
cleanly is not proof that its semantics remain correct on a new SQLite version.
Do not use whitespace/context-ignoring options to force an old patch through.

To add or refresh a patch, generate a Git unified diff against the appropriate
unpatched source (or the previous patch in the series), using repository-root
paths. Keep the patched source and patch file together in the same commit.
This script only maintains the bundled SQLite sources; SQLCipher and system
SQLite are separate dependencies.

## Current series

| Patch | Upstream base | Reason | Regression tests |
|---|---|---|---|
| `0001-fts5-contentless-delete-statistics.patch` | SQLite 3.53.2 (rusqlite v0.40.2) | DEV-724: subtract deleted document sizes and row count using the existing docsize lookup | `cargo test --manifest-path dev724-bench/Cargo.toml --release --locked` |
| `0002-fts5-recount-totals.patch` | 0001 | DEV-724: add the `'recount-totals'` command, which rebuilds the averages record from `%_docsize` | same as 0001 |

0001 fixes future maintenance only; it does not erase overcounts that existing
databases already store. 0002 repairs them: run
`INSERT INTO t(t) VALUES('recount-totals')` once per existing table. It needs
`columnsize=1`, reads stored sizes without tokenizing content, and follows the
enclosing transaction (a rollback discards the recount). SQLite builds without
this series reject the command with an error instead of silently ignoring it.
