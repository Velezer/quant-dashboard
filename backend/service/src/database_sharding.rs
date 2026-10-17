use rusqlite::{params, Connection, OptionalExtension};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const MIGRATION_ID: &str = "market_database_to_ticker_files_v1";
const TABLES: [&str; 3] = ["bars", "series", "ingestion_runs"];
const BARS: &[&str] = &[
    "symbol",
    "interval",
    "source",
    "ts",
    "open",
    "high",
    "low",
    "close",
    "volume",
    "adj_close",
    "dividends",
    "splits",
    "ingested_at",
];
const SERIES: &[&str] = &[
    "symbol",
    "interval",
    "source",
    "exchange_id",
    "product_id",
    "instrument_id",
    "data_provider",
    "timezone",
    "price_basis",
    "instrument_kind",
    "last_success",
    "last_attempt",
    "last_error",
];
const RUNS: &[&str] = &[
    "id",
    "symbol",
    "interval",
    "source",
    "exchange_id",
    "product_id",
    "data_provider",
    "attempted_at",
    "status",
    "rows_received",
    "error",
];

pub(crate) fn shard_market_database(
    source_path: &str,
    market_dir: &str,
    app_path: &str,
    signals_path: &str,
) -> Result<String, String> {
    if same_path(source_path, app_path)? {
        return Err("QUANT_MARKET_DB and QUANT_APP_DB must point to different files".into());
    }
    if same_path(source_path, signals_path)? {
        return Err("QUANT_MARKET_DB and QUANT_SIGNAL_DB must point to different files".into());
    }
    if within(source_path, market_dir)? || within(app_path, market_dir)? {
        return Err("QUANT_MARKET_DB and QUANT_APP_DB must be outside QUANT_MARKET_DIR".into());
    }
    if within(signals_path, market_dir)? {
        return Err("QUANT_SIGNAL_DB must be outside QUANT_MARKET_DIR".into());
    }
    super::connections::validate_signal_database_path(Path::new(app_path), Path::new(signals_path))
        .map_err(|error| error.to_string())?;
    if let Some(hash) = completed_summary(app_path, source_path)? {
        if Path::new(source_path).exists() {
            verify_retired_source(source_path, &hash)?;
            remove_source_files(source_path)?;
        }
        return Ok(format!("already complete ({hash})"));
    }
    if !Path::new(source_path).is_file() {
        return Err(format!(
            "market migration source does not exist: {source_path}"
        ));
    }
    fs::create_dir_all(market_dir).map_err(|e| e.to_string())?;
    create_parent(app_path)?;
    let db = Connection::open(source_path).map_err(|e| e.to_string())?;
    db.execute("ATTACH DATABASE ?1 AS app", [app_path])
        .map_err(|e| e.to_string())?;
    let signals_existed = Path::new(signals_path).exists();
    create_parent(signals_path)?;
    db.execute("ATTACH DATABASE ?1 AS signals", [signals_path])
        .map_err(|e| e.to_string())?;
    prepare_offline(&db)?;
    super::connections::configure_signal_database(&db).map_err(|error| error.to_string())?;
    super::connections::validate_signal_layout(&db, signals_existed)
        .map_err(|error| error.to_string())?;
    super::initialize_app_storage_schema(&db).map_err(|e| e.to_string())?;
    validate_app_version(&db)?;
    validate_source(&db)?;
    let source_hash = hash_tables(&db, "main", &TABLES, None)?;
    let total = counts(&db, "main", None)?;
    let symbols = symbols(&db)?;
    let by_symbol = symbols
        .iter()
        .map(|s| Ok((s.clone(), counts(&db, "main", Some(s))?)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    for symbol in &symbols {
        shard_symbol(
            &db,
            Path::new(market_dir),
            symbol,
            &source_hash,
            &by_symbol[symbol],
        )?;
    }
    validate_destinations(&db, Path::new(market_dir), &source_hash, &by_symbol)?;
    if hash_tables(&db, "main", &TABLES, None)? != source_hash {
        return Err("shared market source changed during sharding; source was retained".into());
    }
    let total_json = serde_json::to_string(&total).map_err(|e| e.to_string())?;
    let symbols_json = serde_json::to_string(&by_symbol).map_err(|e| e.to_string())?;
    db.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        if hash_tables(&db, "main", &TABLES, None)? != source_hash {
            return Err(
                "shared market source changed before retirement; source was retained".into(),
            );
        }
        db.execute("INSERT INTO app.storage_shard_migrations(migration_id,source_path,source_hash,source_table_row_counts_json,source_symbol_row_counts_json,completed_at) VALUES(?1,?2,?3,?4,?5,datetime('now'))",params![MIGRATION_ID,source_path,source_hash,total_json,symbols_json]).map_err(|e|e.to_string())?;
        db.execute("INSERT INTO app.storage_layout_meta(key,value) VALUES('storage_layout_version',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[super::LAYOUT_VERSION]).map_err(|e|e.to_string())?;
        for table in TABLES {
            db.execute_batch(&format!("DROP TABLE main.{table}"))
                .map_err(|e| e.to_string())?;
        }
        db.execute_batch("DROP TABLE IF EXISTS main.storage_layout_meta; CREATE TABLE main.storage_shard_retired(migration_id TEXT PRIMARY KEY,source_hash TEXT NOT NULL,completed_at TEXT NOT NULL);")
            .map_err(|e|e.to_string())?;
        db.execute("INSERT INTO main.storage_shard_retired(migration_id,source_hash,completed_at) VALUES(?1,?2,datetime('now'))",params![MIGRATION_ID,source_hash]).map_err(|e|e.to_string())?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = db.execute_batch("ROLLBACK");
        return Err(e);
    }
    db.execute_batch("COMMIT").map_err(|e| e.to_string())?;
    drop(db);
    remove_source_files(source_path)?;
    Ok(format!(
        "complete; source {source_hash}; {} ticker databases; row counts {total_json}",
        symbols.len()
    ))
}

fn shard_symbol(
    db: &Connection,
    dir: &Path,
    symbol: &str,
    source_hash: &str,
    expected: &BTreeMap<String, i64>,
) -> Result<(), String> {
    let path = super::ticker_database_path(dir, symbol).map_err(|e| e.to_string())?;
    let checkpoint:Option<(String,String,String)>=db.query_row("SELECT source_hash,migrated_row_counts_json,destination_hash FROM app.storage_shard_checkpoints WHERE migration_id=?1 AND storage_symbol=?2",params![MIGRATION_ID,symbol],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;
    if checkpoint.is_some() && !path.is_file() {
        return Err(format!(
            "checkpoint exists but destination is missing for {symbol}; source retained"
        ));
    }
    db.execute(
        "ATTACH DATABASE ?1 AS ticker",
        [path.to_string_lossy().as_ref()],
    )
    .map_err(|e| e.to_string())?;
    db.execute_batch("PRAGMA ticker.journal_mode=DELETE; PRAGMA ticker.synchronous=FULL;")
        .map_err(|e| e.to_string())?;
    if let Some((hash, rows, dest_hash)) = checkpoint {
        let actual = counts(db, "ticker", None)?;
        let actual_hash = hash_tables(db, "ticker", &TABLES, None)?;
        let expected_json = serde_json::to_string(expected).map_err(|e| e.to_string())?;
        let valid = hash == source_hash
            && rows == expected_json
            && actual == *expected
            && dest_hash == actual_hash
            && actual_hash == hash_tables(db, "main", &TABLES, Some(symbol))?;
        db.execute_batch("DETACH DATABASE ticker")
            .map_err(|e| e.to_string())?;
        return if valid {
            Ok(())
        } else {
            Err(format!(
                "stored checkpoint failed validation for {symbol}; source retained"
            ))
        };
    }
    if has_user_objects(db, "ticker")? {
        db.execute_batch("DETACH DATABASE ticker")
            .map_err(|e| e.to_string())?;
        return Err(format!(
            "destination for {symbol} has data without a checkpoint; inspect it before retrying"
        ));
    }
    let rows_json = serde_json::to_string(expected).map_err(|e| e.to_string())?;
    db.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        super::connections::create_ticker_schema(db, "ticker")
            .map_err(|error| error.to_string())?;
        for table in TABLES {
            let fields = columns(table).join(",");
            db.execute(&format!("INSERT INTO ticker.{table}({fields}) SELECT {fields} FROM main.{table} WHERE symbol=?1"),[symbol]).map_err(|e|format!("copy {table} for {symbol}: {e}"))?;
            let got = count_symbol(db, "ticker", table, symbol)?;
            if expected.get(table) != Some(&got) {
                return Err(format!("row count mismatch for {symbol}/{table}"));
            }
            if hash_table(db, "main", table, Some(symbol))?
                != hash_table(db, "ticker", table, Some(symbol))?
            {
                return Err(format!(
                    "stored-value validation failed for {symbol}/{table}"
                ));
            }
        }
        db.execute("INSERT INTO ticker.storage_layout_meta(key,value) VALUES('storage_layout_version',?1),('storage_symbol',?2)",params![super::LAYOUT_VERSION,symbol]).map_err(|e|e.to_string())?;
        let dest_hash = hash_tables(db, "ticker", &TABLES, None)?;
        let filename = path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or("invalid ticker filename")?;
        db.execute("INSERT INTO app.market_store_inventory(storage_symbol,database_filename,updated_at) VALUES(?1,?2,unixepoch()) ON CONFLICT(storage_symbol) DO UPDATE SET database_filename=excluded.database_filename,updated_at=excluded.updated_at",params![symbol,filename]).map_err(|e|e.to_string())?;
        db.execute("INSERT INTO app.storage_shard_checkpoints(migration_id,storage_symbol,source_hash,migrated_row_counts_json,destination_hash,completed_at) VALUES(?1,?2,?3,?4,?5,datetime('now'))",params![MIGRATION_ID,symbol,source_hash,rows_json,dest_hash]).map_err(|e|e.to_string())?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = db.execute_batch("ROLLBACK");
        let _ = db.execute_batch("DETACH DATABASE ticker");
        return Err(e);
    }
    db.execute_batch("COMMIT").map_err(|e| e.to_string())?;
    db.execute_batch("DETACH DATABASE ticker")
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn validate_destinations(
    db: &Connection,
    dir: &Path,
    source_hash: &str,
    by_symbol: &BTreeMap<String, BTreeMap<String, i64>>,
) -> Result<(), String> {
    for (symbol, expected) in by_symbol {
        let path = super::ticker_database_path(dir, symbol).map_err(|e| e.to_string())?;
        if !path.is_file() {
            return Err(format!("destination missing for {symbol}; source retained"));
        }
        db.execute(
            "ATTACH DATABASE ?1 AS ticker",
            [path.to_string_lossy().as_ref()],
        )
        .map_err(|e| e.to_string())?;
        let result = (|| -> Result<(), String> {
            let version:Option<String>=db.query_row("SELECT value FROM ticker.storage_layout_meta WHERE key='storage_layout_version'",[],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
            let saved: Option<String> = db
                .query_row(
                    "SELECT value FROM ticker.storage_layout_meta WHERE key='storage_symbol'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if version.as_deref() != Some(super::LAYOUT_VERSION)
                || saved.as_deref() != Some(symbol.as_str())
            {
                return Err(format!("layout marker mismatch for {symbol}"));
            }
            if counts(db, "ticker", None)? != *expected
                || hash_tables(db, "main", &TABLES, Some(symbol))?
                    != hash_tables(db, "ticker", &TABLES, None)?
            {
                return Err(format!(
                    "final row or stored-value validation failed for {symbol}"
                ));
            }
            let record:Option<(String,String)>=db.query_row("SELECT source_hash,destination_hash FROM app.storage_shard_checkpoints WHERE migration_id=?1 AND storage_symbol=?2",params![MIGRATION_ID,symbol],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
            let hash = hash_tables(db, "ticker", &TABLES, None)?;
            if record.as_ref().map(|v| v.0.as_str()) != Some(source_hash)
                || record.as_ref().map(|v| v.1.as_str()) != Some(hash.as_str())
            {
                return Err(format!("checkpoint validation failed for {symbol}"));
            }
            Ok(())
        })();
        let detach = db.execute_batch("DETACH DATABASE ticker");
        result?;
        detach.map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn validate_source(db: &Connection) -> Result<(), String> {
    let version: Option<String> = db
        .query_row(
            "SELECT value FROM main.storage_layout_meta WHERE key='storage_layout_version'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("source needs the explicit legacy split first: {e}"))?;
    if version.as_deref() != Some("2") {
        return Err("shared source is not at completed split layout version 2; run split-market-database first".into());
    }
    if super::split::legacy_app_tables_in_market(db).map_err(|e| e.to_string())? {
        return Err("app-owned tables remain in source; run split-market-database first".into());
    }
    let names = table_names(db, "main")?;
    for name in &names {
        if !TABLES.contains(&name.as_str()) && name != "storage_layout_meta" {
            return Err(format!(
                "unrecognized source table {name}; no market data was moved"
            ));
        }
    }
    for table in TABLES {
        if !names.iter().any(|n| n == table) {
            return Err(format!("source table {table} is missing"));
        }
        let actual = table_columns(db, "main", table)?;
        let mut expected = columns(table).to_vec();
        let mut actual_names = actual.iter().map(String::as_str).collect::<Vec<_>>();
        expected.sort_unstable();
        actual_names.sort_unstable();
        if expected != actual_names {
            return Err(format!(
                "source table {table} has an unsupported column set: expected {expected:?}, found {actual_names:?}"
            ));
        }
        let empty: i64 = db
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM main.{table} WHERE symbol IS NULL OR trim(symbol)=''"
                ),
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if empty > 0 {
            return Err(format!(
                "{table} contains {empty} rows without a storage symbol"
            ));
        }
    }
    Ok(())
}
fn validate_app_version(db: &Connection) -> Result<(), String> {
    let version: Option<String> = db
        .query_row(
            "SELECT value FROM app.storage_layout_meta WHERE key='storage_layout_version'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if version.as_deref() == Some("2") {
        Ok(())
    } else {
        Err("app database must be at completed shared layout version 2 before sharding".into())
    }
}
fn symbols(db: &Connection) -> Result<Vec<String>, String> {
    let mut set = BTreeSet::new();
    for table in TABLES {
        let mut stmt = db
            .prepare(&format!(
                "SELECT DISTINCT symbol FROM main.{table} ORDER BY symbol COLLATE BINARY"
            ))
            .map_err(|e| e.to_string())?;
        for row in stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
        {
            set.insert(row.map_err(|e| e.to_string())?);
        }
    }
    Ok(set.into_iter().collect())
}
fn counts(
    db: &Connection,
    schema: &str,
    symbol: Option<&str>,
) -> Result<BTreeMap<String, i64>, String> {
    let mut result = BTreeMap::new();
    for table in TABLES {
        let count = match symbol {
            Some(s) => count_symbol(db, schema, table, s)?,
            None => db
                .query_row(&format!("SELECT COUNT(*) FROM {schema}.{table}"), [], |r| {
                    r.get(0)
                })
                .map_err(|e| e.to_string())?,
        };
        result.insert(table.to_owned(), count);
    }
    Ok(result)
}
fn count_symbol(db: &Connection, schema: &str, table: &str, symbol: &str) -> Result<i64, String> {
    db.query_row(
        &format!("SELECT COUNT(*) FROM {schema}.{table} WHERE symbol=?1"),
        [symbol],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}
fn hash_tables(
    db: &Connection,
    schema: &str,
    tables: &[&str],
    symbol: Option<&str>,
) -> Result<String, String> {
    let mut hash = Fnv64::new();
    for table in tables {
        hash.add(table.as_bytes());
        hash.add(&hash_table(db, schema, table, symbol)?.to_le_bytes());
    }
    Ok(format!("fnv1a64:{:016x}", hash.0))
}
fn hash_table(
    db: &Connection,
    schema: &str,
    table: &str,
    symbol: Option<&str>,
) -> Result<u64, String> {
    let cols = columns(table)
        .iter()
        .map(|column| (*column).to_owned())
        .collect::<Vec<_>>();
    let selected = cols
        .iter()
        .map(|c| format!("quote(\"{c}\")"))
        .collect::<Vec<_>>()
        .join(",");
    let mut keys = primary_keys(db, schema, table)?;
    if keys.is_empty() {
        keys = cols.clone();
    }
    let order = keys
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(",");
    let query = if symbol.is_some() {
        format!("SELECT {selected} FROM {schema}.{table} WHERE symbol=?1 ORDER BY {order}")
    } else {
        format!("SELECT {selected} FROM {schema}.{table} ORDER BY {order}")
    };
    let mut stmt = db.prepare(&query).map_err(|e| e.to_string())?;
    let mut rows = match symbol {
        Some(s) => stmt.query([s]).map_err(|e| e.to_string())?,
        None => stmt.query([]).map_err(|e| e.to_string())?,
    };
    let mut hash = Fnv64::new();
    for col in &cols {
        hash.add(col.as_bytes());
    }
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        hash.add(b"row");
        for i in 0..cols.len() {
            let value: String = row.get(i).map_err(|e| e.to_string())?;
            hash.add(value.as_bytes());
            hash.add(&[0]);
        }
    }
    Ok(hash.0)
}
fn columns(table: &str) -> &'static [&'static str] {
    match table {
        "bars" => BARS,
        "series" => SERIES,
        "ingestion_runs" => RUNS,
        _ => &[],
    }
}
fn table_columns(db: &Connection, schema: &str, table: &str) -> Result<Vec<String>, String> {
    let mut statement = db
        .prepare(&format!("PRAGMA {schema}.table_info(\"{table}\")"))
        .map_err(|error| error.to_string())?;
    let result = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string());
    result
}
fn primary_keys(db: &Connection, schema: &str, table: &str) -> Result<Vec<String>, String> {
    let mut s = db
        .prepare(&format!("PRAGMA {schema}.table_info('{table}')"))
        .map_err(|e| e.to_string())?;
    let mut v = s
        .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    v.retain(|(_, n)| *n > 0);
    v.sort_by_key(|(_, n)| *n);
    Ok(v.into_iter().map(|(s, _)| s).collect())
}
fn table_names(db: &Connection, schema: &str) -> Result<Vec<String>, String> {
    let mut statement = db
        .prepare(&format!(
            "SELECT name FROM {schema}.sqlite_master WHERE type=?1 AND name NOT LIKE ?2"
        ))
        .map_err(|error| error.to_string())?;
    let result = statement
        .query_map(["table", "sqlite_%"], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string());
    result
}
fn has_user_objects(db: &Connection, schema: &str) -> Result<bool, String> {
    db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {schema}.sqlite_master WHERE name NOT LIKE 'sqlite_%' AND type IN ('table','view','trigger'))"),[],|r|r.get(0)).map_err(|e|e.to_string())
}
fn completed_summary(app: &str, source: &str) -> Result<Option<String>, String> {
    if !Path::new(app).is_file() {
        return Ok(None);
    }
    let db = Connection::open(app).map_err(|e| e.to_string())?;
    let exists:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='storage_shard_migrations')",[],|r|r.get(0)).map_err(|e|e.to_string())?;
    if !exists {
        return Ok(None);
    }
    db.query_row(
        "SELECT source_hash FROM storage_shard_migrations WHERE migration_id=?1 AND source_path=?2",
        params![MIGRATION_ID, source],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}
fn verify_retired_source(source: &str, expected: &str) -> Result<(), String> {
    let db = Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let names = table_names(&db, "main")?;
    if names.as_slice() != ["storage_shard_retired"] {
        return Err("completed migration record exists but the source path contains data outside its retirement marker; left untouched".into());
    }
    let (found, count): (String, i64) = db
        .query_row(
            "SELECT source_hash,(SELECT COUNT(*) FROM storage_shard_retired)
         FROM storage_shard_retired WHERE migration_id=?1",
            [MIGRATION_ID],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "retirement marker is missing; source left untouched".to_string())?;
    if found == expected && count == 1 {
        Ok(())
    } else {
        Err(
            "completed migration record does not match the retired source marker; left untouched"
                .into(),
        )
    }
}
fn prepare_offline(db: &Connection) -> Result<(), String> {
    db.execute_batch("PRAGMA busy_timeout=5000;PRAGMA foreign_keys=ON;PRAGMA main.cache_size=-512;PRAGMA app.cache_size=-512;PRAGMA temp_store=FILE;PRAGMA main.mmap_size=0;PRAGMA app.mmap_size=0;PRAGMA main.journal_mode=DELETE;PRAGMA app.journal_mode=DELETE;PRAGMA main.synchronous=FULL;PRAGMA app.synchronous=FULL;").map_err(|e|format!("could not prepare offline migration; stop service and retry: {e}"))
}
fn create_parent(path: &str) -> Result<(), String> {
    if let Some(p) = Path::new(path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn remove_source_files(path: &str) -> Result<(), String> {
    for p in [
        path.to_owned(),
        format!("{path}-wal"),
        format!("{path}-shm"),
        format!("{path}-journal"),
    ] {
        match fs::remove_file(&p) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(format!("could not retire {p}: {e}")),
        }
    }
    Ok(())
}
fn normalize(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                normalized.push(component.as_os_str());
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(_) => {
                normalized.push(component.as_os_str());
                if let Ok(canonical) = fs::canonicalize(&normalized) {
                    normalized = canonical;
                }
            }
        }
    }
    Ok(normalized)
}
pub(super) fn same_path(a: &str, b: &str) -> Result<bool, String> {
    Ok(normalize(a)? == normalize(b)?)
}
fn within(path: &str, dir: &str) -> Result<bool, String> {
    Ok(normalize(path)?.starts_with(normalize(dir)?))
}
struct Fnv64(u64);
impl Fnv64 {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }
    fn add(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}
