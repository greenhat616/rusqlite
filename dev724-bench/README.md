# DEV-724 FTS5 statistics experiment

This isolated crate compares SQLite's contentless-delete statistics fix with
transactional application-maintained corpus statistics. It does not change
Gety's dependencies or implement its historical database migration.

## Variants

- `baseline`: unpatched SQLite; query statistics combine live `COUNT(*)` with
  the native token total, as Gety's current main ranking path does.
- `workaround`: the same unpatched SQLite; each affected row is read through
  `xColumnSize` before and, for an upsert, after the write. A single statistics
  record is updated once per transaction, with the accumulated count/length
  delta. No re-tokenization or direct shadow-table writes are used.
- `patched`: SQLite's contentless-delete path subtracts the old sizes using
  its existing docsize lookup. Queries use native row and token totals.

The baseline's scores are incorrect after replacements. Use it to estimate
existing costs, not as a score oracle. The two correct variants are checked
against independently generated corpus lengths and identical score inputs.

## Reproduce on Windows

From the rusqlite checkout, with the SQLite patch applied:

```powershell
cargo test --manifest-path dev724-bench/Cargo.toml --release --locked
cargo test --lib --features bundled
python dev724-bench/build_variants.py
python dev724-bench/run.py
python dev724-bench/summarize.py
```

The builder archives the upstream base commit
`e88f112bef7899234a497baed5cc3c3d553deeb8` (rusqlite v0.40.2) into an isolated baseline source tree,
so committing the patch does not change the baseline. Existing
baseline sources and measured results are not overwritten. Each build or
benchmark child has a 300-second timeout. The runner pins itself and its
children to logical processor 2 and alternates variant order. It does not
change the machine's power plan. Benchmarks run sequentially; do not run
compilation or other benchmarks concurrently.

Raw timings, score samples, binary hashes, and environment metadata are in
`results/measured/`. The executable prints SQLite's version and source ID;
the patch intentionally does not change the upstream source ID, so the
binary hashes and source patch must also be retained.
The frozen measurements from this experiment are in `measurements/2026-09-15/`.
They were taken on an earlier base, upstream master
`726c921a350591abffdc99ac8d8a5bea21fbb208` with SQLite 3.53.4 (see its
`environment.json`); rebuilding the variants now measures rusqlite v0.40.2.
Per-operation summaries are the median of each repetition's mean, not
percentiles of individual-operation latency.

## Workloads and limitations

- Short: 20,000 documents, lengths 16–47 tokens, batches of 500, WAL and memory.
- Medium: 10,000 documents, lengths 128–383, batches of 500, WAL.
- Long: 3,000 documents, lengths 1,024–3,071, batches of 500, WAL.
- Single: 2,000 short documents, one document per transaction, WAL.
- Each workload inserts, replaces identically, replaces with longer content,
  ranks queries, deletes half the documents, repeats missing-row deletes,
  optimizes, and (for disk databases) reopens and verifies statistics.
- Fixture generation and correctness scans are outside mutation timings.
  Full scans between phases warm the cache equally for each variant.
- WAL uses `synchronous=NORMAL` and default cache/autocheckpoint settings.
- The small statistics table is present in all variants to keep schema layout
  comparable; reported database sizes are not a measurement of its overhead.
- Mutation timings include statement preparation, SQL work, and commits.
- Search timings include one read transaction, corpus statistics, exact
  phrase document frequency, and top-20 scoring with deterministic ties.
  The scorer uses Gety's BM25 K1/B and unit column weights for a single phrase.
  `needle` matches 1% of documents; `common` matches all documents.
- The tokenizer is SQLite's default unicode61. This is a SQLite/rusqlite
  mechanism benchmark, not Gety's Jieba/pinyin tokenizer, multi-index search,
  interval scorer, RRF, connector extraction, or UI latency benchmark.
- `recovery_scan` measures only reading/summing live row sizes. It does not
  measure an implemented repair of native historical averages, migration
  locking, or cold-cache startup on a production corpus.
- The workaround represents the public-API approach proposed for DEV-724;
  it is not a lower bound for all possible application-level implementations.

## Correctness

The patched tests cover unchanged and changed replacements, per-column
totals, unindexed and empty fields, multi-byte sizes, missing-row deletion,
transaction/savepoint rollback, tokenizer failure during replacement,
co-located tokens, and comparison with a fresh index after 2,000 deterministic
mutations. Built-in BM25 scores are compared for terms, OR queries, and
phrases. Benchmarks additionally check optimize and close/reopen behavior.

The `'recount-totals'` tests forge inflated totals (as an unpatched SQLite
leaves them) and check that the recount restores native statistics, passes
`integrity-check` on normal content tables (which verifies totals against
content), and matches a fresh index's built-in BM25 scores. They also cover
rollback and savepoint rollback of a recount, writes before and after a recount
in one transaction, `columnsize=0` rejection, and truncated size records.

The unpatched build fails the first repeated-replacement assertion: native
statistics become `(4,12)` while live rows remain `(3,7)`. `integrity-check`
alone is insufficient to catch this particular contentless statistics defect.

## Patch scope

The maintained patch now lives in
[`libsqlite3-sys/patches/`](../libsqlite3-sys/patches/README.md).
Use `python libsqlite3-sys/apply_sqlite_patches.py check` to verify it.

The change is in `libsqlite3-sys/sqlite3/sqlite3.c`, within
`fts5StorageContentlessDelete`. It adds no SQL statements, allocation,
tokenization, public Rust API, or persistent format. The bounded size decoder
also rejects truncated/corrupt size records before reading past the blob.

The patch fixes future maintenance. It does not erase overcounts already
stored in existing databases. Deployment needs a separate, transactional
historical repair, ideally implemented inside FTS5 using its own averages
cache and persistence routines, plus migration tests. Direct application
writes to `_data` are not part of this experiment.

The patched amalgamation should be regenerated from an equivalent change to
SQLite's `ext/fts5/fts5_storage.c` for an upstream submission or maintained
SQLite fork. SQLCipher and non-bundled/system SQLite builds are not patched.
