//! Explicit, offline conversion of duplicate app proxy histories to shared series.
use crate::research_proxy::shared::{legacy_shared_source, SOURCE_EXCHANGE_ID};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::path::Path;

const MIGRATION_ID: &str = "shared_crypto_reference_series_v1";
const TABLES: [&str; 3] = ["bars", "series", "ingestion_runs"];

pub(crate) fn share_crypto_candles(market_dir: &str, app_path: &str) -> Result<String, String> {
    if !Path::new(app_path).is_file() || !Path::new(market_dir).is_dir() {
        return Err("migration requires an existing QUANT_APP_DB and QUANT_MARKET_DIR; stop the service first".into());
    }
    if super::sharding::within(app_path, market_dir)? {
        return Err("QUANT_APP_DB must be outside QUANT_MARKET_DIR".into());
    }
    let app = Connection::open_with_flags(app_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| error.to_string())?;
    let version: Option<String> = app
        .query_row(
            "SELECT value FROM storage_layout_meta WHERE key='storage_layout_version'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if version.as_deref() != Some(super::LAYOUT_VERSION) {
        return Err("app database must have a completed ticker storage layout before sharing crypto candles".into());
    }
    let mut cursor: Option<String> = None;
    let mut tickers = 0usize;
    let mut merged_series = 0usize;
    loop {
        // No inventory cursor or app read transaction stays open while the
        // ticker transaction writes its checkpoint to the attached app store.
        let next: Option<(String, String)> = app
            .query_row(
                "SELECT storage_symbol,database_filename FROM market_store_inventory
             WHERE ?1 IS NULL OR storage_symbol>?1 ORDER BY storage_symbol LIMIT 1",
                params![cursor],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some((symbol, filename)) = next else {
            break;
        };
        let path = super::connections::ticker_database_path(Path::new(market_dir), &symbol)
            .map_err(|error| error.to_string())?;
        if path.file_name().and_then(|name| name.to_str()) != Some(filename.as_str()) {
            return Err(format!(
                "market inventory filename does not match {symbol}; left untouched"
            ));
        }
        let merged = migrate_ticker(&path, app_path, &symbol)?;
        merged_series += merged;
        tickers += usize::from(merged > 0);
        cursor = Some(symbol);
    }
    Ok(format!(
        "complete: {merged_series} legacy series merged in {tickers} ticker databases"
    ))
}

fn migrate_ticker(path: &Path, app_path: &str, symbol: &str) -> Result<usize, String> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|error| format!("cannot open {symbol}: {error}"))?;
    db.execute("ATTACH DATABASE ?1 AS app", [app_path])
        .map_err(|error| error.to_string())?;
    db.execute_batch(
        "PRAGMA busy_timeout=5000; PRAGMA main.journal_mode=DELETE; PRAGMA app.journal_mode=DELETE;
         PRAGMA main.synchronous=FULL; PRAGMA app.synchronous=FULL;
         PRAGMA main.cache_size=-256; PRAGMA app.cache_size=-256; PRAGMA temp_store=FILE;
         PRAGMA main.mmap_size=0; PRAGMA app.mmap_size=0;",
    )
    .map_err(|error| format!("stop the service before migrating {symbol}: {error}"))?;
    let marker: Option<(String, String)> = db
        .query_row(
            "SELECT version.value,symbol.value FROM storage_layout_meta AS version
         JOIN storage_layout_meta AS symbol ON symbol.key='storage_symbol'
         WHERE version.key='storage_layout_version'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if marker != Some((super::LAYOUT_VERSION.to_string(), symbol.to_string())) {
        return Err(format!(
            "ticker storage layout does not match {symbol}; left untouched"
        ));
    }
    db.execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let result = merge_ticker(&db, symbol);
    match result {
        Ok(merged) => match db.execute_batch("COMMIT") {
            Ok(()) => Ok(merged),
            Err(error) => {
                let _ = db.execute_batch("ROLLBACK");
                Err(error.to_string())
            }
        },
        Err(error) => {
            let _ = db.execute_batch("ROLLBACK");
            Err(format!("{symbol}: {error}"))
        }
    }
}

struct LegacySeries {
    interval: String,
    source: String,
    exchange: String,
    product: String,
    provider: String,
    timezone: String,
    price_basis: String,
    category: String,
}

fn merge_ticker(db: &Connection, symbol: &str) -> Result<usize, String> {
    let mut source_snapshot = None;
    let mut cursor: Option<(String, String)> = None;
    let mut merged_series = 0usize;
    let mut legacy_bars = 0i64;
    loop {
        let legacy = db.query_row(
            "SELECT interval,source,exchange_id,product_id,data_provider,timezone,price_basis,instrument_kind
             FROM main.series WHERE symbol=?1 AND instrument_kind IN ('crypto_spot','crypto_perpetual','crypto_futures')
               AND (?2 IS NULL OR (source,interval)>(?2,?3)) ORDER BY source,interval LIMIT 1",
            params![symbol, cursor.as_ref().map(|key| &key.0), cursor.as_ref().map(|key| &key.1)],
            |row| Ok(LegacySeries { interval:row.get(0)?, source:row.get(1)?, exchange:row.get(2)?,
                product:row.get(3)?, provider:row.get(4)?, timezone:row.get(5)?, price_basis:row.get(6)?, category:row.get(7)? }),
        ).optional().map_err(|error| error.to_string())?;
        let Some(legacy) = legacy else {
            break;
        };
        cursor = Some((legacy.source.clone(), legacy.interval.clone()));
        if legacy.timezone != "UTC" {
            continue;
        }
        let Some(canonical) = legacy_shared_source(
            &legacy.source,
            &legacy.exchange,
            &legacy.product,
            &legacy.category,
            &legacy.provider,
            &legacy.price_basis,
        ) else {
            continue;
        };
        let checkpoint: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM app.storage_shard_checkpoints WHERE migration_id=?1 AND storage_symbol=?2)",
            params![MIGRATION_ID,symbol], |row| row.get(0),
        ).map_err(|error| error.to_string())?;
        if checkpoint {
            return Err("legacy crypto series reappeared after a completed sharing checkpoint; left untouched".into());
        }
        if source_snapshot.is_none() {
            source_snapshot = Some((
                super::sharding::hash_tables(db, "main", &TABLES, Some(symbol))?,
                super::sharding::counts(db, "main", Some(symbol))?,
            ));
        }
        legacy_bars += merge_series(db, symbol, &legacy, &canonical)?;
        merged_series += 1;
    }
    if let Some((source_hash, source_counts)) = source_snapshot {
        let destination_hash = super::sharding::hash_tables(db, "main", &TABLES, Some(symbol))?;
        let destination_counts = super::sharding::counts(db, "main", Some(symbol))?;
        let counts = serde_json::json!({"source":source_counts,"destination":destination_counts,
            "merged_series":merged_series,"legacy_bars":legacy_bars})
        .to_string();
        db.execute(
            "INSERT INTO app.storage_shard_checkpoints(migration_id,storage_symbol,source_hash,migrated_row_counts_json,destination_hash,completed_at)
             VALUES(?1,?2,?3,?4,?5,datetime('now'))",
            params![MIGRATION_ID,symbol,source_hash,counts,destination_hash],
        ).map_err(|error| error.to_string())?;
    }
    Ok(merged_series)
}

fn merge_series(
    db: &Connection,
    symbol: &str,
    legacy: &LegacySeries,
    canonical: &str,
) -> Result<i64, String> {
    let mismatch: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM series WHERE symbol=?1 AND interval=?2 AND source=?3
         AND (exchange_id<>?4 OR product_id<>'' OR instrument_id IS NOT NULL OR data_provider<>?5
              OR timezone<>?6 OR instrument_kind<>?8
              OR (price_basis<>?7 AND NOT (price_basis='research_proxy' AND NOT EXISTS(
                  SELECT 1 FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3)))))",
            params![
                symbol,
                legacy.interval,
                canonical,
                SOURCE_EXCHANGE_ID,
                legacy.provider,
                legacy.timezone,
                legacy.price_basis,
                legacy.category
            ],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if mismatch {
        return Err(format!(
            "shared series {canonical} has incompatible provenance; left untouched"
        ));
    }
    let before: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3",
            params![symbol, legacy.interval, canonical],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let (old_rows, added): (i64, i64) = db
        .query_row(
            "SELECT COUNT(*),COALESCE(SUM(NOT EXISTS(SELECT 1 FROM bars AS destination
          WHERE destination.symbol=old.symbol AND destination.interval=old.interval
            AND destination.source=?4 AND destination.ts=old.ts)),0)
         FROM bars AS old WHERE old.symbol=?1 AND old.interval=?2 AND old.source=?3",
            params![symbol, legacy.interval, legacy.source, canonical],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    db.execute(
        "INSERT INTO series(symbol,interval,source,exchange_id,product_id,instrument_id,data_provider,
             timezone,price_basis,instrument_kind,last_success,last_attempt,last_error)
         SELECT symbol,interval,?4,?5,'',NULL,data_provider,timezone,price_basis,instrument_kind,
             last_success,last_attempt,last_error FROM series WHERE symbol=?1 AND interval=?2 AND source=?3
         ON CONFLICT(symbol,interval,source) DO UPDATE SET price_basis=excluded.price_basis,
             last_success=CASE WHEN COALESCE(julianday(replace(excluded.last_success,' UTC','')),0)
                >= COALESCE(julianday(replace(series.last_success,' UTC','')),0) THEN excluded.last_success ELSE series.last_success END,
             last_error=CASE WHEN COALESCE(julianday(replace(excluded.last_attempt,' UTC','')),0)
                >= COALESCE(julianday(replace(series.last_attempt,' UTC','')),0) THEN excluded.last_error ELSE series.last_error END,
             last_attempt=CASE WHEN COALESCE(julianday(replace(excluded.last_attempt,' UTC','')),0)
                >= COALESCE(julianday(replace(series.last_attempt,' UTC','')),0) THEN excluded.last_attempt ELSE series.last_attempt END",
        params![symbol,legacy.interval,legacy.source,canonical,SOURCE_EXCHANGE_ID],
    ).map_err(|error| error.to_string())?;
    // A newer ingestion wins. Equal timestamps are resolved by the fixed
    // legacy source ordering; the later source replaces the earlier one.
    db.execute(
        "INSERT INTO bars(symbol,interval,source,ts,open,high,low,close,volume,adj_close,dividends,splits,ingested_at)
         SELECT symbol,interval,?4,ts,open,high,low,close,volume,adj_close,dividends,splits,ingested_at
         FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3
         ON CONFLICT(symbol,interval,source,ts) DO UPDATE SET open=excluded.open,high=excluded.high,
             low=excluded.low,close=excluded.close,volume=excluded.volume,adj_close=excluded.adj_close,
             dividends=excluded.dividends,splits=excluded.splits,ingested_at=excluded.ingested_at
         WHERE COALESCE(julianday(replace(excluded.ingested_at,' UTC','')),0)
             >= COALESCE(julianday(replace(bars.ingested_at,' UTC','')),0)",
        params![symbol,legacy.interval,legacy.source,canonical],
    ).map_err(|error| error.to_string())?;
    let invalid: i64 = db.query_row(
        "SELECT COUNT(*) FROM bars AS old LEFT JOIN bars AS destination
         ON destination.symbol=old.symbol AND destination.interval=old.interval AND destination.source=?4 AND destination.ts=old.ts
         WHERE old.symbol=?1 AND old.interval=?2 AND old.source=?3 AND (destination.ts IS NULL
           OR COALESCE(julianday(replace(old.ingested_at,' UTC','')),0)>COALESCE(julianday(replace(destination.ingested_at,' UTC','')),0)
           OR (COALESCE(julianday(replace(old.ingested_at,' UTC','')),0)=COALESCE(julianday(replace(destination.ingested_at,' UTC','')),0)
             AND NOT (old.open IS destination.open AND old.high IS destination.high AND old.low IS destination.low
               AND old.close IS destination.close AND old.volume IS destination.volume AND old.adj_close IS destination.adj_close
               AND old.dividends IS destination.dividends AND old.splits IS destination.splits AND old.ingested_at IS destination.ingested_at)))",
        params![symbol,legacy.interval,legacy.source,canonical], |row| row.get(0),
    ).map_err(|error| error.to_string())?;
    let after: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3",
            params![symbol, legacy.interval, canonical],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if invalid != 0 || after != before + added {
        return Err(format!(
            "shared history validation failed for {} ({invalid} differing or missing rows)",
            legacy.source
        ));
    }
    db.execute(
        "UPDATE ingestion_runs SET source=?4 WHERE symbol=?1 AND interval=?2 AND source=?3",
        params![symbol, legacy.interval, legacy.source, canonical],
    )
    .map_err(|error| error.to_string())?;
    db.execute(
        "DELETE FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3",
        params![symbol, legacy.interval, legacy.source],
    )
    .map_err(|error| error.to_string())?;
    db.execute(
        "DELETE FROM series WHERE symbol=?1 AND interval=?2 AND source=?3",
        params![symbol, legacy.interval, legacy.source],
    )
    .map_err(|error| error.to_string())?;
    Ok(old_rows)
}

#[cfg(test)]
#[path = "shared_crypto_tests.rs"]
mod tests;
