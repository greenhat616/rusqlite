use rusqlite::{Connection, OptionalExtension, Result, ffi};
use std::ffi::{c_int, c_void};

// All callbacks operate on the same FTS tokenizer statistics as the index.
unsafe extern "C" fn auxiliary(
    api: *const ffi::Fts5ExtensionApi,
    fts: *mut ffi::Fts5Context,
    ctx: *mut ffi::sqlite3_context,
    argc: c_int,
    argv: *mut *mut ffi::sqlite3_value,
) {
    unsafe {
        let api = &*api;
        let kind = api.xUserData.unwrap()(fts) as usize;
        let mut value = 0_i64;
        let rc = match kind {
            1 => api.xRowCount.unwrap()(fts, &mut value),
            2 => {
                let column = if argc == 1 {
                    ffi::sqlite3_value_int(*argv)
                } else {
                    -1
                };
                api.xColumnTotalSize.unwrap()(fts, column, &mut value)
            }
            3 => {
                let mut len = 0;
                let rc = api.xColumnSize.unwrap()(fts, -1, &mut len);
                value = len as i64;
                rc
            }
            4 => api.xQueryPhrase.unwrap()(fts, 0, (&mut value as *mut i64).cast(), Some(hit)),
            5 => {
                // Single-phrase, unit-column-weight BM25, matching Gety's K1/B.
                let avgdl = ffi::sqlite3_value_double(*argv);
                let idf = ffi::sqlite3_value_double(*argv.add(1));
                let mut frequency = 0;
                let mut len = 0;
                let mut rc = api.xInstCount.unwrap()(fts, &mut frequency);
                if rc == ffi::SQLITE_OK {
                    rc = api.xColumnSize.unwrap()(fts, -1, &mut len);
                }
                if rc == ffi::SQLITE_OK {
                    let frequency = frequency as f64;
                    let score = -idf * frequency * 2.2
                        / (frequency + 1.2 * (0.25 + 0.75 * len as f64 / avgdl));
                    ffi::sqlite3_result_double(ctx, score);
                } else {
                    ffi::sqlite3_result_error_code(ctx, rc);
                }
                return;
            }
            _ => ffi::SQLITE_ERROR,
        };
        if rc == ffi::SQLITE_OK {
            ffi::sqlite3_result_int64(ctx, value);
        } else {
            ffi::sqlite3_result_error_code(ctx, rc);
        }
    }
}

unsafe extern "C" fn hit(
    _: *const ffi::Fts5ExtensionApi,
    _: *mut ffi::Fts5Context,
    user: *mut c_void,
) -> c_int {
    unsafe { *user.cast::<i64>() += 1 };
    ffi::SQLITE_OK
}

pub fn register(conn: &Connection) -> Result<()> {
    unsafe {
        let mut api: *mut ffi::fts5_api = std::ptr::null_mut();
        // Obtain the extension API using SQLite's documented pointer binding.
        let mut raw = std::ptr::null_mut();
        assert_eq!(
            ffi::sqlite3_prepare_v2(
                conn.handle(),
                c"SELECT fts5(?1)".as_ptr(),
                -1,
                &mut raw,
                std::ptr::null_mut()
            ),
            ffi::SQLITE_OK
        );
        assert_eq!(
            ffi::sqlite3_bind_pointer(
                raw,
                1,
                (&mut api as *mut *mut ffi::fts5_api).cast(),
                c"fts5_api_ptr".as_ptr(),
                None
            ),
            ffi::SQLITE_OK
        );
        assert_eq!(ffi::sqlite3_step(raw), ffi::SQLITE_ROW);
        assert_eq!(ffi::sqlite3_finalize(raw), ffi::SQLITE_OK);
        assert!(!api.is_null());
        for (name, kind) in [
            (c"native_n", 1),
            (c"native_t", 2),
            (c"row_tokens", 3),
            (c"phrase_hits", 4),
            (c"bench_bm25", 5),
        ] {
            assert_eq!(
                (*api).xCreateFunction.unwrap()(
                    api,
                    name.as_ptr(),
                    kind as *mut c_void,
                    Some(auxiliary),
                    None
                ),
                ffi::SQLITE_OK
            );
        }
        #[cfg(test)]
        tests::register_tokenizer(api);
    }
    Ok(())
}

pub fn native(conn: &Connection, table: &str) -> Result<(i64, i64)> {
    Ok(conn
        .query_row(
            &format!("SELECT native_n({table}),native_t({table}) FROM {table} LIMIT 1"),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .unwrap_or_default())
}

pub fn scan(conn: &Connection, table: &str) -> Result<(i64, i64)> {
    let mut stmt = conn.prepare(&format!("SELECT row_tokens({table}) FROM {table}"))?;
    let mut rows = stmt.query([])?;
    let (mut n, mut t) = (0, 0);
    while let Some(row) = rows.next()? {
        n += 1;
        t += row.get::<_, i64>(0)?;
    }
    Ok((n, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::c_char;

    type Push =
        unsafe extern "C" fn(*mut c_void, c_int, *const c_char, c_int, c_int, c_int) -> c_int;

    unsafe extern "C" fn create(
        _: *mut c_void,
        _: *mut *const c_char,
        _: c_int,
        out: *mut *mut ffi::Fts5Tokenizer,
    ) -> c_int {
        unsafe {
            *out = Box::into_raw(Box::new(0_u8)).cast();
        }
        ffi::SQLITE_OK
    }

    unsafe extern "C" fn delete(tokenizer: *mut ffi::Fts5Tokenizer) {
        unsafe {
            drop(Box::from_raw(tokenizer.cast::<u8>()));
        }
    }

    unsafe extern "C" fn tokenize(
        _: *mut ffi::Fts5Tokenizer,
        ctx: *mut c_void,
        flags: c_int,
        text: *const c_char,
        len: c_int,
        push: Option<Push>,
    ) -> c_int {
        let bytes = unsafe { std::slice::from_raw_parts(text.cast::<u8>(), len as usize) };
        let mut offset = 0;
        for word in bytes.split(|b| *b == b' ') {
            if !word.is_empty() {
                if word == b"FAIL" {
                    return ffi::SQLITE_ERROR;
                }
                let rc = unsafe {
                    push.unwrap()(
                        ctx,
                        0,
                        word.as_ptr().cast(),
                        word.len() as c_int,
                        offset,
                        offset + word.len() as c_int,
                    )
                };
                if rc != ffi::SQLITE_OK {
                    return rc;
                }
                if flags & ffi::FTS5_TOKENIZE_DOCUMENT != 0 {
                    let rc = unsafe {
                        push.unwrap()(
                            ctx,
                            ffi::FTS5_TOKEN_COLOCATED,
                            c"alias".as_ptr(),
                            5,
                            offset,
                            offset + word.len() as c_int,
                        )
                    };
                    if rc != ffi::SQLITE_OK {
                        return rc;
                    }
                }
            }
            offset += word.len() as c_int + 1;
        }
        ffi::SQLITE_OK
    }

    pub(super) unsafe fn register_tokenizer(api: *mut ffi::fts5_api) {
        let mut tokenizer = ffi::fts5_tokenizer {
            xCreate: Some(create),
            xDelete: Some(delete),
            xTokenize: Some(tokenize),
        };
        unsafe {
            assert_eq!(
                (*api).xCreateTokenizer.unwrap()(
                    api,
                    c"synonyms".as_ptr(),
                    std::ptr::null_mut(),
                    &mut tokenizer,
                    None
                ),
                ffi::SQLITE_OK
            );
        }
    }

    #[test]
    fn colocated_tokens_and_failed_replacement_preserve_exact_statistics() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        register(&conn)?;
        conn.execute_batch("CREATE VIRTUAL TABLE ft USING fts5(t,content='',contentless_delete=1,tokenize='synonyms');
            INSERT INTO ft(rowid,t) VALUES(1,'a b c'),(2,'d e');")?;
        check(&conn, (2, 5));
        conn.execute_batch("INSERT OR REPLACE INTO ft(rowid,t) VALUES(1,'a b c')")?;
        check(&conn, (2, 5));
        assert!(
            conn.execute("INSERT OR REPLACE INTO ft(rowid,t) VALUES(1,'a FAIL')", [])
                .is_err()
        );
        check(&conn, (2, 5));
        conn.execute_batch("DELETE FROM ft WHERE rowid=1")?;
        check(&conn, (1, 2));
        Ok(())
    }

    #[test]
    fn update_history_matches_fresh_index_and_builtin_bm25() -> Result<()> {
        for options in [
            "",
            ",content='',contentless_delete=1",
            ",content='',contentless_delete=1,contentless_unindexed=1",
        ] {
            let conn = Connection::open_in_memory()?;
            register(&conn)?;
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE ft USING fts5(a,b,c UNINDEXED{options});"
            ))?;
            let mut oracle: Vec<Option<(String, String)>> = vec![None; 100];
            let mut state = 17_u64;
            for step in 0..2000 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                let id = ((state >> 32) % 100) as usize;
                if step % 7 == 0 {
                    conn.execute("DELETE FROM ft WHERE rowid=?1", [id as i64])?;
                    oracle[id] = None;
                } else {
                    let a = if id % 5 == 0 {
                        "needle ".to_owned()
                    } else {
                        String::new()
                    } + &"word ".repeat(step % 300);
                    let b = "other ".repeat(id % 11);
                    conn.execute(
                        "INSERT OR REPLACE INTO ft(rowid,a,b,c) VALUES(?1,?2,?3,'meta')",
                        rusqlite::params![id as i64, a, b],
                    )?;
                    oracle[id] = Some((a, b));
                }
                if step % 100 == 0 {
                    let expected = (
                        oracle.iter().flatten().count() as i64,
                        oracle
                            .iter()
                            .flatten()
                            .map(|(a, b)| {
                                (a.split_whitespace().count() + b.split_whitespace().count()) as i64
                            })
                            .sum(),
                    );
                    check(&conn, expected);
                }
            }
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE fresh USING fts5(a,b,c UNINDEXED{options});"
            ))?;
            for (id, item) in oracle.iter().enumerate() {
                if let Some((a, b)) = item {
                    conn.execute(
                        "INSERT INTO fresh(rowid,a,b,c) VALUES(?1,?2,?3,'meta')",
                        rusqlite::params![id as i64, a, b],
                    )?;
                }
            }
            assert_eq!(native(&conn, "ft")?, native(&conn, "fresh")?);
            for query in ["needle", "word", "needle OR other", "\"word word\""] {
                let read = |table: &str| -> Result<Vec<(i64, f64)>> {
                    let mut stmt=conn.prepare(&format!("SELECT rowid,bm25({table}) FROM {table} WHERE {table} MATCH ?1 ORDER BY rowid"))?;
                    stmt.query_map([query], |r| Ok((r.get(0)?, r.get(1)?)))?
                        .collect()
                };
                assert_eq!(read("ft")?, read("fresh")?, "{options}: {query}");
            }
        }
        Ok(())
    }

    fn check(conn: &Connection, expected: (i64, i64)) {
        assert_eq!(scan(conn, "ft").unwrap(), expected);
        assert_eq!(native(conn, "ft").unwrap(), expected);
        conn.execute_batch("INSERT INTO ft(ft) VALUES('integrity-check')")
            .unwrap();
    }

    #[test]
    fn contentless_statistics_follow_live_rows_and_transaction_rollback() -> Result<()> {
        let conn = Connection::open_in_memory()?;
        register(&conn)?;
        conn.execute_batch("CREATE VIRTUAL TABLE ft USING fts5(a,b,c UNINDEXED,content='',contentless_delete=1);
            INSERT INTO ft(rowid,a,b,c) VALUES(1,'a b c','d e','ignored'),(2,'a','b','ignored'),(3,'','','ignored');")?;
        check(&conn, (3, 7));
        for _ in 0..3 {
            conn.execute(
                "INSERT OR REPLACE INTO ft(rowid,a,b,c) VALUES(1,'a b c','d e','ignored')",
                [],
            )?;
            check(&conn, (3, 7));
        }
        conn.execute_batch(
            "UPDATE ft SET a='one',b='two',c='ignored' WHERE rowid=1;
            DELETE FROM ft WHERE rowid=98765;",
        )?;
        check(&conn, (3, 4));
        conn.execute_batch(
            "BEGIN; DELETE FROM ft WHERE rowid=1; SAVEPOINT s;
            INSERT OR REPLACE INTO ft(rowid,a,b,c) VALUES(2,'longer a b','four five','ignored');",
        )?;
        check(&conn, (2, 5));
        conn.execute_batch("ROLLBACK TO s")?;
        check(&conn, (2, 2));
        conn.execute_batch("ROLLBACK")?;
        check(&conn, (3, 4));
        conn.execute_batch("DELETE FROM ft WHERE rowid=2; INSERT INTO ft(ft) VALUES('optimize')")?;
        check(&conn, (2, 2));
        conn.execute_batch("DELETE FROM ft")?;
        check(&conn, (0, 0));
        conn.execute_batch("INSERT INTO ft(rowid,a,b,c) VALUES(1,'again','new','ignored')")?;
        check(&conn, (1, 2));
        // Multi-byte size varints and per-column totals.
        let long = "word ".repeat(300);
        conn.execute(
            "INSERT OR REPLACE INTO ft(rowid,a,b,c) VALUES(1,?1,'new','ignored')",
            [&long],
        )?;
        check(&conn, (1, 301));
        conn.execute_batch(
            "INSERT OR REPLACE INTO ft(rowid,a,b,c) VALUES(1,'short','','ignored')",
        )?;
        check(&conn, (1, 1));
        let cols: (i64, i64, i64) = conn.query_row(
            "SELECT native_t(ft,0),native_t(ft,1),native_t(ft,2) FROM ft",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        assert_eq!(cols, (1, 0, 0));
        Ok(())
    }
}
