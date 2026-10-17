#[cfg(test)]
use super::*;

/// Add normalized app/product/provider identities to the established SQLite
/// tables. `bars` remains the only persisted candle history table.
#[cfg(test)]
pub(super) fn migrate_normalized_market_model(
    connection: &Connection,
) -> Result<(), rusqlite::Error> {
    // The migration writes this marker in the same transaction as the
    // normalization. Skip the series rewrite after it has completed.
    let already_migrated: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM instrument_registry_meta
                       WHERE key='normalized_market_model_v1')",
        [],
        |row| row.get(0),
    )?;
    if already_migrated {
        return Ok(());
    }

    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        for (table, column, definition) in [
            (
                "series",
                "product_id",
                "ALTER TABLE main.series ADD COLUMN product_id TEXT NOT NULL DEFAULT ''",
            ),
            (
                "series",
                "instrument_id",
                "ALTER TABLE main.series ADD COLUMN instrument_id TEXT",
            ),
            (
                "ingestion_runs",
                "product_id",
                "ALTER TABLE main.ingestion_runs ADD COLUMN product_id TEXT NOT NULL DEFAULT ''",
            ),
        ] {
            if !sqlite_table_column_exists(connection, table, column)? {
                connection.execute_batch(definition)?;
            }
        }
        connection.execute(
            "UPDATE series SET product_id=CASE
                WHEN source LIKE 'pintu_retail_spot%' THEN 'pintu_retail_spot'
                WHEN source LIKE 'pintu_pro_spot%' THEN 'pintu_pro_spot'
                WHEN source LIKE 'pintu_futures%' OR source LIKE 'pintu_pro_futures%' THEN 'pintu_pro_futures'
                WHEN source LIKE 'pluang_spot%' THEN 'pluang_spot'
                WHEN source LIKE 'pluang_perpetual%' THEN 'pluang_perpetual'
                ELSE product_id END WHERE product_id=''",
            [],
        )?;
        connection.execute(
            "UPDATE ingestion_runs SET product_id=CASE
                WHEN source LIKE 'daily_scheduler_pintu_retail_spot%' THEN 'pintu_retail_spot'
                WHEN source LIKE 'daily_scheduler_pintu_pro_spot%' THEN 'pintu_pro_spot'
                WHEN source LIKE 'daily_scheduler_pintu_pro_futures%' OR source LIKE 'daily_scheduler_pintu_futures%' THEN 'pintu_pro_futures'
                WHEN source LIKE 'daily_scheduler_pluang_spot%' THEN 'pluang_spot'
                WHEN source LIKE 'daily_scheduler_pluang_perpetual%' THEN 'pluang_perpetual'
                ELSE COALESCE((SELECT s.product_id FROM series s
                    WHERE s.symbol=ingestion_runs.symbol AND s.source=ingestion_runs.source
                    ORDER BY s.last_success DESC LIMIT 1),product_id) END
              WHERE product_id=''",
            [],
        )?;

        let series_rows = {
            let mut statement = connection
                .prepare("SELECT symbol,interval,source,exchange_id,product_id FROM series")?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for (symbol, interval, source, exchange, product) in series_rows {
            let app_id = if exchange == "pintu_futures" {
                "pintu"
            } else {
                &exchange
            };
            let instrument: Option<String> = if product.is_empty() {
                None
            } else {
                connection.query_row(
                    "SELECT instrument_id FROM instruments WHERE exchange_id=?1 AND product_id=?2
                       AND (storage_symbol=?3 OR symbol=?3) LIMIT 1",
                    params![app_id,product,symbol], |row| row.get(0),
                ).optional()?
            };
            connection.execute(
                "UPDATE series SET instrument_id=COALESCE(?1,instrument_id)
                  WHERE symbol=?2 AND interval=?3 AND source=?4",
                params![instrument, symbol, interval, source],
            )?;
        }

        connection.execute_batch(
            "INSERT INTO instrument_registry_meta(key,value)
                 VALUES('normalized_market_model_v1',datetime('now'))
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value;",
        )?;
        Ok::<(), rusqlite::Error>(())
    })();
    match result {
        Ok(()) => connection.execute_batch("COMMIT"),
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

#[cfg(test)]
pub(super) fn sqlite_table_column_exists(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, rusqlite::Error> {
    let Some(schema) = sqlite_table_schema(connection, table)? else {
        return Ok(false);
    };
    let escaped_table = table.replace('\'', "''");
    let mut statement =
        connection.prepare(&format!("PRAGMA {schema}.table_info('{escaped_table}')"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
pub(super) fn sqlite_column_is_primary_key(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, rusqlite::Error> {
    let Some(schema) = sqlite_table_schema(connection, table)? else {
        return Ok(false);
    };
    let escaped_table = table.replace('\'', "''");
    let mut statement =
        connection.prepare(&format!("PRAGMA {schema}.table_info('{escaped_table}')"))?;
    let columns = statement.query_map([], |row| {
        Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?))
    })?;
    for existing in columns {
        let (name, position) = existing?;
        if name == column && position > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
fn sqlite_table_schema(
    connection: &Connection,
    table: &str,
) -> Result<Option<&'static str>, rusqlite::Error> {
    let in_main: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM main.sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )?;
    if in_main {
        return Ok(Some("main"));
    }
    let in_app = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM app.sqlite_master WHERE type='table' AND name=?1)",
            [table],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false);
    Ok(in_app.then_some("app"))
}

/// Add source identity to canonical candle rows and series keys. Existing
/// candles inherit their source from the old one-source-per-series metadata.
/// The conversion is transactional and keeps every stored OHLCV field.
#[cfg(test)]
pub(super) fn migrate_candle_source_identity(
    connection: &Connection,
) -> Result<(), rusqlite::Error> {
    let bars_have_source = sqlite_table_column_exists(connection, "bars", "source")?;
    let series_has_source_key = sqlite_column_is_primary_key(connection, "series", "source")?;
    if bars_have_source && series_has_source_key {
        let index_columns_match = {
            let mut statement =
                connection.prepare("PRAGMA main.index_info('idx_bars_series_ts')")?;
            let columns = statement
                .query_map([], |row| row.get::<_, Option<String>>(2))?
                .collect::<Result<Vec<_>, _>>()?;
            let expected = ["symbol", "interval", "source", "ts"];
            columns.len() == expected.len()
                && columns
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| actual.as_deref() == Some(expected))
        };
        if !index_columns_match {
            connection.execute_batch(
                "DROP INDEX IF EXISTS idx_bars_series_ts;
                 CREATE INDEX main.idx_bars_series_ts ON bars(symbol, interval, source, ts);",
            )?;
        }
        if !sqlite_table_column_exists(connection, "ingestion_runs", "source")? {
            connection.execute_batch("ALTER TABLE main.ingestion_runs ADD COLUMN source TEXT")?;
        }
        connection.execute(
            "UPDATE ingestion_runs
                SET source=COALESCE((
                    SELECT s.source FROM series s
                     WHERE s.symbol=ingestion_runs.symbol
                       AND (s.interval=ingestion_runs.interval OR ingestion_runs.interval='scheduler')
                     ORDER BY CASE WHEN s.source LIKE '%_native' THEN 0 ELSE 1 END,
                              COALESCE(s.last_success, '') DESC
                     LIMIT 1
                ), 'unknown')
              WHERE source IS NULL",
            [],
        )?;
        return Ok(());
    }

    connection.execute_batch(
        "BEGIN IMMEDIATE;
         ALTER TABLE main.bars RENAME TO bars_before_source_identity;
         ALTER TABLE main.series RENAME TO series_before_source_identity;
         CREATE TABLE main.bars (
             symbol TEXT NOT NULL, interval TEXT NOT NULL,
             source TEXT NOT NULL DEFAULT 'unknown', ts INTEGER NOT NULL,
             open REAL NOT NULL, high REAL NOT NULL, low REAL NOT NULL,
             close REAL NOT NULL, volume REAL NOT NULL, adj_close REAL,
             dividends REAL, splits REAL, ingested_at TEXT NOT NULL,
             PRIMARY KEY(symbol, interval, source, ts)
         );
         CREATE TABLE main.series (
             symbol TEXT NOT NULL, interval TEXT NOT NULL,
             source TEXT NOT NULL, timezone TEXT NOT NULL,
             price_basis TEXT NOT NULL, instrument_kind TEXT NOT NULL,
             last_success TEXT, last_attempt TEXT, last_error TEXT,
             PRIMARY KEY(symbol, interval, source)
         );
         INSERT INTO series (
             symbol, interval, source, timezone, price_basis, instrument_kind,
             last_success, last_attempt, last_error
         )
         SELECT symbol, interval,
                CASE WHEN source='yfinance' THEN 'yfinance_fallback' ELSE source END,
                timezone, price_basis, instrument_kind,
                last_success, last_attempt, last_error
           FROM series_before_source_identity;
         INSERT INTO bars (
             symbol, interval, source, ts, open, high, low, close, volume,
             adj_close, dividends, splits, ingested_at
         )
         SELECT b.symbol, b.interval,
                COALESCE(CASE WHEN s.source='yfinance' THEN 'yfinance_fallback' ELSE s.source END, 'unknown'),
                b.ts, b.open, b.high, b.low,
                b.close, b.volume, b.adj_close, b.dividends, b.splits, b.ingested_at
           FROM bars_before_source_identity AS b
           LEFT JOIN series_before_source_identity AS s
             ON s.symbol=b.symbol AND s.interval=b.interval;
         INSERT OR IGNORE INTO series (
             symbol, interval, source, timezone, price_basis, instrument_kind
         )
         SELECT DISTINCT b.symbol, b.interval, 'unknown', 'UTC',
                         'unknown', 'unknown'
           FROM bars_before_source_identity AS b
          WHERE NOT EXISTS (
               SELECT 1 FROM series_before_source_identity AS s
               WHERE s.symbol=b.symbol AND s.interval=b.interval
          );
         ",
    )?;
    let old_bar_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM bars_before_source_identity",
        [],
        |row| row.get(0),
    )?;
    let old_series_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM series_before_source_identity",
        [],
        |row| row.get(0),
    )?;
    let migrated_bar_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM bars", [], |row| row.get(0))?;
    let migrated_series_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM series", [], |row| row.get(0))?;
    if migrated_bar_count != old_bar_count || migrated_series_count < old_series_count {
        connection.execute_batch("ROLLBACK")?;
        return Err(rusqlite::Error::InvalidQuery);
    }
    connection.execute_batch(
        "DROP TABLE main.bars_before_source_identity;
         DROP TABLE main.series_before_source_identity;
         DROP INDEX IF EXISTS main.idx_bars_series_ts;
         CREATE INDEX main.idx_bars_series_ts ON bars(symbol, interval, source, ts);
         COMMIT;",
    )?;
    if !sqlite_table_column_exists(connection, "ingestion_runs", "source")? {
        connection.execute_batch("ALTER TABLE main.ingestion_runs ADD COLUMN source TEXT")?;
    }
    connection.execute(
        "UPDATE ingestion_runs
            SET source=COALESCE((
                SELECT s.source FROM series s
                 WHERE s.symbol=ingestion_runs.symbol
                   AND (s.interval=ingestion_runs.interval OR ingestion_runs.interval='scheduler')
                 ORDER BY CASE WHEN s.source LIKE '%_native' THEN 0 ELSE 1 END,
                          COALESCE(s.last_success, '') DESC
                 LIMIT 1
            ), 'unknown')
          WHERE source IS NULL",
        [],
    )?;
    Ok(())
}

/// Keep the app exchange and the remote market-data provider as independent
/// provenance fields. The source key remains a unique feed identity so two
/// apps can store separate proxy histories for the same underlying symbol.
#[cfg(test)]
pub(super) fn migrate_candle_provider_identity(
    connection: &Connection,
) -> Result<(), rusqlite::Error> {
    if !sqlite_table_column_exists(connection, "series", "exchange_id")? {
        connection.execute_batch(
            "ALTER TABLE main.series ADD COLUMN exchange_id TEXT NOT NULL DEFAULT 'unknown'",
        )?;
    }
    if !sqlite_table_column_exists(connection, "series", "data_provider")? {
        connection.execute_batch(
            "ALTER TABLE main.series ADD COLUMN data_provider TEXT NOT NULL DEFAULT 'unknown'",
        )?;
    }
    if !sqlite_table_column_exists(connection, "ingestion_runs", "exchange_id")? {
        connection.execute_batch(
            "ALTER TABLE main.ingestion_runs ADD COLUMN exchange_id TEXT NOT NULL DEFAULT 'unknown'",
        )?;
    }
    if !sqlite_table_column_exists(connection, "ingestion_runs", "data_provider")? {
        connection.execute_batch(
            "ALTER TABLE main.ingestion_runs ADD COLUMN data_provider TEXT NOT NULL DEFAULT 'unknown'",
        )?;
    }

    connection.execute(
        "UPDATE series SET
             exchange_id=CASE
                 WHEN source='stockbit' OR source LIKE 'stockbit_%' THEN 'stockbit'
                 WHEN source='pluang' OR source LIKE 'pluang_%' THEN 'pluang'
                 WHEN source IN ('pintu','pintu_futures') OR source LIKE 'pintu_futures_%' THEN 'pintu_futures'
                 WHEN source='tokocrypto' OR source LIKE 'tokocrypto_%' THEN 'tokocrypto'
                 ELSE exchange_id END,
             data_provider=CASE
                 WHEN source LIKE '%_binance_%' THEN 'binance'
                 WHEN source='tokocrypto_native' OR source='tokocrypto' THEN 'tokocrypto'
                 WHEN source LIKE '%_yfinance_fallback' OR source IN ('yfinance','yfinance_fallback') THEN 'yfinance'
                 ELSE data_provider END
         WHERE exchange_id='unknown' OR data_provider='unknown'",
        [],
    )?;
    connection.execute(
        "UPDATE ingestion_runs SET
             exchange_id=COALESCE((SELECT s.exchange_id FROM series s
                                    WHERE s.symbol=ingestion_runs.symbol
                                      AND s.source=ingestion_runs.source
                                    ORDER BY COALESCE(s.last_success,'') DESC LIMIT 1),
                                  CASE
                                    WHEN source='stockbit' OR source LIKE 'stockbit_%' THEN 'stockbit'
                                    WHEN source='pluang' OR source LIKE 'pluang_%' THEN 'pluang'
                                    WHEN source IN ('pintu','pintu_futures') OR source LIKE 'pintu_futures_%' THEN 'pintu_futures'
                                    WHEN source='tokocrypto' OR source LIKE 'tokocrypto_%' THEN 'tokocrypto'
                                    ELSE 'unknown' END),
             data_provider=COALESCE((SELECT s.data_provider FROM series s
                                      WHERE s.symbol=ingestion_runs.symbol
                                        AND s.source=ingestion_runs.source
                                      ORDER BY COALESCE(s.last_success,'') DESC LIMIT 1),
                                    CASE
                                      WHEN source LIKE '%_binance_%' THEN 'binance'
                                      WHEN source='tokocrypto_native' OR source='tokocrypto' THEN 'tokocrypto'
                                      WHEN source LIKE '%_yfinance_fallback' OR source IN ('yfinance','yfinance_fallback') THEN 'yfinance'
                                      ELSE 'unknown' END)
         WHERE exchange_id='unknown' OR data_provider='unknown'",
        [],
    )?;
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS main.idx_ingestion_runs_daily_target
             ON ingestion_runs(symbol, exchange_id, interval, attempted_at);",
    )?;
    Ok(())
}
