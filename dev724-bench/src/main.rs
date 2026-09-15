use dev724_bench::{native, register, scan};
use rusqlite::{Connection, OptionalExtension, Result, params};
use std::{hint::black_box, path::Path, time::Instant};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Baseline,
    Workaround,
    Patched,
}

struct Doc {
    text: String,
    tokens: i64,
}

fn fixture(id: usize, target: usize, changed: bool) -> Doc {
    let len = target / 2 + (id * 37 % target) + if changed { target / 3 } else { 0 };
    let mut text = String::with_capacity(len * 6);
    for j in 0..len {
        if j == 0 && id % 100 == 0 {
            text.push_str("needle ");
        } else if j == 1 {
            text.push_str("common ");
        } else {
            text.push_str(&format!("w{} ", (id * 17 + j * 13) % 4096));
        }
    }
    Doc {
        text,
        tokens: len as i64,
    }
}

fn selected_stats(conn: &Connection, mode: Mode) -> Result<(i64, i64)> {
    match mode {
        Mode::Workaround => conn.query_row("SELECT n,t FROM corpus_stats WHERE id=1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        }),
        Mode::Patched => native(conn, "ft"),
        Mode::Baseline => {
            let (_, t) = native(conn, "ft")?;
            let n = conn.query_row("SELECT count(*) FROM ft", [], |r| r.get(0))?;
            Ok((n, t))
        }
    }
}

fn mutate(
    conn: &mut Connection,
    mode: Mode,
    docs: &[Doc],
    ids: &[usize],
    delete: bool,
    batch: usize,
) -> Result<()> {
    for chunk in ids.chunks(batch) {
        let tx = conn.transaction()?;
        {
            let mut write = tx.prepare(if delete {
                "DELETE FROM ft WHERE rowid=?1"
            } else {
                "INSERT OR REPLACE INTO ft(rowid,t) VALUES(?1,?2)"
            })?;
            let mut size = tx.prepare("SELECT row_tokens(ft) FROM ft WHERE rowid=?1")?;
            let (mut delta_n, mut delta_t) = (0_i64, 0_i64);
            for &id in chunk {
                let old: Option<i64> = if mode == Mode::Workaround {
                    size.query_row([id as i64], |r| r.get(0)).optional()?
                } else {
                    None
                };
                if delete {
                    write.execute([id as i64])?;
                } else {
                    write.execute(params![id as i64, docs[id].text])?;
                }
                if mode == Mode::Workaround {
                    let new: Option<i64> = if delete {
                        None
                    } else {
                        Some(size.query_row([id as i64], |r| r.get(0))?)
                    };
                    delta_n += i64::from(new.is_some()) - i64::from(old.is_some());
                    delta_t += new.unwrap_or(0) - old.unwrap_or(0);
                }
            }
            if mode == Mode::Workaround {
                tx.execute(
                    "UPDATE corpus_stats SET n=n+?1,t=t+?2 WHERE id=1",
                    params![delta_n, delta_t],
                )?;
            }
        }
        tx.commit()?;
    }
    Ok(())
}

fn scores(conn: &Connection, mode: Mode, query: &str) -> Result<Vec<(i64, f64)>> {
    // One read snapshot for N/T, document frequency, and ranking.
    let tx = conn.unchecked_transaction()?;
    let (n, t) = selected_stats(&tx, mode)?;
    let hits: i64 = tx
        .query_row(
            "SELECT phrase_hits(ft) FROM ft WHERE ft MATCH ?1 LIMIT 1",
            [query],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    let idf = (((n - hits) as f64 + 0.5) / (hits as f64 + 0.5))
        .ln()
        .max(1e-6);
    let result = {
        let mut stmt = tx.prepare("SELECT rowid,bench_bm25(ft,?1,?2) AS s FROM ft WHERE ft MATCH ?3 ORDER BY s,rowid LIMIT 20")?;
        stmt.query_map(params![t as f64 / n as f64, idf, query], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<Result<Vec<_>>>()?
    };
    tx.commit()?;
    Ok(result)
}

fn validate(conn: &Connection, mode: Mode, expected: (i64, i64)) -> Result<()> {
    assert_eq!(scan(conn, "ft")?, expected);
    if mode != Mode::Baseline {
        assert_eq!(selected_stats(conn, mode)?, expected);
    }
    conn.execute_batch("INSERT INTO ft(ft) VALUES('integrity-check')")?;
    Ok(())
}

fn report(mode: &str, case: &str, rep: usize, phase: &str, ops: usize, start: Instant) {
    println!(
        "{mode},{case},{rep},{phase},{ops},{:.6}",
        start.elapsed().as_secs_f64() * 1000.0
    );
}

fn run(
    mode: Mode,
    label: &str,
    case: &str,
    n: usize,
    length: usize,
    batch: usize,
    rep: usize,
    root: &Path,
    memory: bool,
) -> Result<()> {
    let original: Vec<_> = (0..n).map(|i| fixture(i, length, false)).collect();
    let changed: Vec<_> = (0..n).map(|i| fixture(i, length, true)).collect();
    // Coprime stride gives deterministic, non-sequential row access.
    let ids: Vec<_> = (0..n).map(|i| (i * 7919) % n).collect();
    let deleted: Vec<_> = ids.iter().copied().take(n / 2).collect();
    let path = root.join(format!("{label}-{case}-{rep}.db"));
    assert!(
        !path.exists(),
        "benchmark refuses to overwrite {}",
        path.display()
    );
    let mut conn = if memory {
        Connection::open_in_memory()?
    } else {
        Connection::open(&path)?
    };
    register(&conn)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;
        CREATE VIRTUAL TABLE ft USING fts5(t,content='',contentless_delete=1);
        CREATE TABLE corpus_stats(id INTEGER PRIMARY KEY,n INTEGER NOT NULL,t INTEGER NOT NULL);
        INSERT INTO corpus_stats VALUES(1,0,0);",
    )?;
    let expected_initial = (n as i64, original.iter().map(|d| d.tokens).sum());
    let expected_changed = (n as i64, changed.iter().map(|d| d.tokens).sum());
    let start = Instant::now();
    mutate(&mut conn, mode, &original, &ids, false, batch)?;
    report(label, case, rep, "insert", n, start);
    validate(&conn, mode, expected_initial)?;
    let start = Instant::now();
    mutate(&mut conn, mode, &original, &ids, false, batch)?;
    report(label, case, rep, "replace_same", n, start);
    validate(&conn, mode, expected_initial)?;
    let start = Instant::now();
    mutate(&mut conn, mode, &changed, &ids, false, batch)?;
    report(label, case, rep, "replace_changed", n, start);
    validate(&conn, mode, expected_changed)?;

    // This is only the common scan component of historical repair, not a migration.
    let start = Instant::now();
    assert_eq!(scan(&conn, "ft")?, expected_changed);
    report(label, case, rep, "recovery_scan", n, start);
    let start = Instant::now();
    for _ in 0..100 {
        black_box(selected_stats(&conn, mode)?);
    }
    report(label, case, rep, "stats_read", 100, start);
    let start = Instant::now();
    for _ in 0..20 {
        black_box(conn.query_row("SELECT count(*) FROM ft", [], |r| r.get::<_, i64>(0))?);
    }
    report(label, case, rep, "count_scan", 20, start);
    for (query, iterations) in [("needle", 100), ("common", 20)] {
        let reference = scores(&conn, mode, query)?;
        // Independent input oracle: count and token total from generated fixtures.
        if mode != Mode::Baseline {
            conn.execute(
                "UPDATE corpus_stats SET n=?1,t=?2",
                params![expected_changed.0, expected_changed.1],
            )?;
            assert_eq!(reference, scores(&conn, Mode::Workaround, query)?);
        }
        for _ in 0..3 {
            black_box(scores(&conn, mode, query)?);
        }
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(scores(&conn, mode, query)?);
        }
        report(
            label,
            case,
            rep,
            &format!("query_{query}"),
            iterations,
            start,
        );
        eprintln!("score,{label},{case},{rep},{query},{reference:?}");
    }

    let start = Instant::now();
    mutate(&mut conn, mode, &changed, &deleted, true, batch)?;
    report(label, case, rep, "delete_half", deleted.len(), start);
    let expected_deleted = (
        (n - deleted.len()) as i64,
        expected_changed.1 - deleted.iter().map(|&i| changed[i].tokens).sum::<i64>(),
    );
    validate(&conn, mode, expected_deleted)?;
    let start = Instant::now();
    mutate(&mut conn, mode, &changed, &deleted, true, batch)?;
    report(label, case, rep, "delete_missing", deleted.len(), start);
    validate(&conn, mode, expected_deleted)?;
    let start = Instant::now();
    conn.execute_batch("INSERT INTO ft(ft) VALUES('optimize')")?;
    report(label, case, rep, "optimize", 1, start);
    validate(&conn, mode, expected_deleted)?;
    if !memory {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
        drop(conn);
        let reopened = Connection::open(&path)?;
        register(&reopened)?;
        validate(&reopened, mode, expected_deleted)?;
        drop(reopened);
        eprintln!(
            "database_bytes,{label},{case},{rep},{}",
            std::fs::metadata(&path).unwrap().len()
        );
        std::fs::remove_file(&path).unwrap();
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let label = args.get(1).map(String::as_str).unwrap_or("baseline");
    let mode = match label {
        "baseline" => Mode::Baseline,
        "workaround" => Mode::Workaround,
        "patched" => Mode::Patched,
        _ => panic!("invalid mode"),
    };
    let reps: usize = args.get(2).map(|v| v.parse().unwrap()).unwrap_or(1);
    let root = Path::new(args.get(3).map(String::as_str).unwrap_or("results/db"));
    std::fs::create_dir_all(root).unwrap();
    let filter = args.get(4).map(String::as_str).unwrap_or("all");
    eprintln!(
        "sqlite={},source={},mode={label},profile=release",
        rusqlite::version(),
        Connection::open_in_memory()?
            .query_row("SELECT sqlite_source_id()", [], |r| r.get::<_, String>(0))?
    );
    println!("mode,case,rep,phase,ops,ms");
    for rep in 0..reps {
        for (name, n, length, batch, memory) in [
            ("smoke", 500, 32, 100, false),
            ("short_wal", 20000, 32, 500, false),
            ("medium_wal", 10000, 256, 500, false),
            ("long_wal", 3000, 2048, 500, false),
            ("short_single_wal", 2000, 32, 1, false),
            ("short_memory", 20000, 32, 500, true),
        ] {
            if (filter == "all" && name != "smoke") || filter == name {
                run(mode, label, name, n, length, batch, rep, root, memory)?;
            }
        }
    }
    Ok(())
}
