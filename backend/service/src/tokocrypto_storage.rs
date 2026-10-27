//! Tokocrypto's public market-data adapter and the SQLite ingestion boundary.
//!
//! The provider response is decoded in memory by the Tokocrypto client. Only
//! validated typed bars and instrument metadata cross this module boundary and
//! are persisted in the active market database.

#[cfg(test)]
use super::master_data::tokenized_stock_symbols;
use super::tokocrypto::{fetch_klines, MarketBar, VenueSymbol};
use rusqlite::{params, Connection, OptionalExtension};

const SOURCE: &str = "tokocrypto_native";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncResult {
    pub storage_symbol: String,
    pub venue_symbol: String,
    pub quote_currency: String,
    pub asset_category: String,
    pub rows_15m: usize,
    pub rows_1h: usize,
    pub rows_1d: usize,
}

/// Refresh registered 15-minute and hourly bars using the validated venue
/// symbol saved with the app's verified instrument listing.
pub fn sync_registered_intraday_asset(
    connection: &Connection,
    registered_storage_symbol: &str,
    venue_symbol: &str,
    quote_currency: &str,
    contract_metadata_json: &str,
    requested_tokenized: bool,
) -> Result<SyncResult, String> {
    let venue = registered_venue(
        venue_symbol,
        quote_currency,
        contract_metadata_json,
        requested_tokenized,
    )?;
    sync_intraday_market(
        connection,
        &venue,
        requested_tokenized,
        registered_storage_symbol,
    )
}

fn sync_intraday_market(
    connection: &Connection,
    venue: &VenueSymbol,
    tokenized: bool,
    registered_storage_symbol: &str,
) -> Result<SyncResult, String> {
    sync_intraday_market_with_fetch(
        connection,
        venue,
        tokenized,
        registered_storage_symbol,
        fetch_klines,
    )
}

fn sync_intraday_market_with_fetch(
    connection: &Connection,
    venue: &VenueSymbol,
    tokenized: bool,
    registered_storage_symbol: &str,
    fetch: impl Fn(&VenueSymbol, &str, usize) -> Result<Vec<MarketBar>, String>,
) -> Result<SyncResult, String> {
    let storage_symbol = resolve_storage_symbol(venue, tokenized, registered_storage_symbol)?;
    let bars_15m = fetch(
        &venue,
        "15m",
        recent_candle_limit(connection, &storage_symbol, "15m", 720, 501, 900)?,
    )
    .map_err(|error| {
        record_failure(connection, &storage_symbol, "15m", &error);
        error
    })?;
    let category = if tokenized {
        "tokenized_stock"
    } else {
        "crypto_spot"
    };
    // Commit fresh 15m prices before requesting the independent hourly series.
    persist_sync(connection, &storage_symbol, category, &[("15m", &bars_15m)]).map_err(
        |error| {
            record_failure(connection, &storage_symbol, "15m", &error);
            error
        },
    )?;
    let bars_1h = fetch(
        &venue,
        "1h",
        recent_candle_limit(connection, &storage_symbol, "1h", 500, 499, 3600)?,
    )
    .map_err(|error| {
        record_failure(connection, &storage_symbol, "1h", &error);
        error
    })?;
    persist_sync(connection, &storage_symbol, category, &[("1h", &bars_1h)]).map_err(|error| {
        record_failure(connection, &storage_symbol, "1h", &error);
        error
    })?;

    Ok(SyncResult {
        storage_symbol,
        venue_symbol: venue.symbol.clone(),
        quote_currency: venue.quote_asset.clone(),
        asset_category: if tokenized {
            "tokenized_stock"
        } else {
            "crypto_spot"
        }
        .to_string(),
        rows_15m: bars_15m.len(),
        rows_1h: bars_1h.len(),
        rows_1d: 0,
    })
}

fn recent_candle_limit(
    connection: &Connection,
    symbol: &str,
    interval: &str,
    bootstrap_limit: usize,
    warmup_bars: usize,
    interval_seconds: i64,
) -> Result<usize, String> {
    let (count, oldest, newest): (i64, Option<i64>, Option<i64>) = connection.query_row(
        "SELECT COUNT(*),MIN(ts),MAX(ts) FROM (SELECT ts FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3 ORDER BY ts DESC LIMIT ?4)",
        params![symbol, interval, SOURCE, warmup_bars as i64],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).map_err(|error| error.to_string())?;
    if count < warmup_bars as i64 {
        return Ok(bootstrap_limit);
    }
    let (Some(oldest), Some(newest)) = (oldest, newest) else {
        return Ok(bootstrap_limit);
    };
    let now = i64::try_from(crate::now_secs()).map_err(|error| error.to_string())?;
    let expected_span = (count - 1)
        .saturating_mul(interval_seconds)
        .saturating_mul(crate::NANOS_PER_SECOND);
    // A short incremental request cannot repair holes or future timestamps.
    if newest.saturating_sub(oldest) != expected_span
        || newest.div_euclid(crate::NANOS_PER_SECOND) > now
    {
        return Ok(bootstrap_limit);
    }
    let elapsed = now
        .saturating_sub(newest.div_euclid(crate::NANOS_PER_SECOND))
        .max(0);
    // Replay the last completed bar and allow for the currently forming bar.
    let needed = elapsed.div_euclid(interval_seconds).saturating_add(3);
    Ok(usize::try_from(needed)
        .unwrap_or(bootstrap_limit)
        .clamp(3, bootstrap_limit))
}

/// Refresh one registered daily candle window without downloading the venue
/// symbol catalog again.
pub fn sync_registered_daily_asset(
    connection: &Connection,
    registered_storage_symbol: &str,
    venue_symbol: &str,
    quote_currency: &str,
    contract_metadata_json: &str,
    requested_tokenized: bool,
) -> Result<usize, String> {
    let venue = registered_venue(
        venue_symbol,
        quote_currency,
        contract_metadata_json,
        requested_tokenized,
    )?;
    sync_daily_market(
        connection,
        &venue,
        requested_tokenized,
        registered_storage_symbol,
    )
}

fn sync_daily_market(
    connection: &Connection,
    venue: &VenueSymbol,
    tokenized: bool,
    registered_storage_symbol: &str,
) -> Result<usize, String> {
    let storage_symbol = resolve_storage_symbol(venue, tokenized, registered_storage_symbol)?;
    let bars_1d = fetch_klines(&venue, "1d", 500).map_err(|error| {
        record_failure(connection, &storage_symbol, "1d", &error);
        error
    })?;
    persist_sync(
        connection,
        &storage_symbol,
        if tokenized {
            "tokenized_stock"
        } else {
            "crypto_spot"
        },
        &[("1d", &bars_1d)],
    )
    .map_err(|error| {
        record_failure(connection, &storage_symbol, "1d", &error);
        error
    })?;
    Ok(bars_1d.len())
}

fn registered_venue(
    venue_symbol: &str,
    quote_currency: &str,
    contract_metadata_json: &str,
    tokenized: bool,
) -> Result<VenueSymbol, String> {
    let venue = super::tokocrypto::venue_symbol_from_registration(
        venue_symbol,
        quote_currency,
        contract_metadata_json,
    )?;
    if (tokenized && venue.quote_asset != "USDT")
        || (!tokenized && !matches!(venue.quote_asset.as_str(), "IDR" | "USDT"))
    {
        return Err("registered Tokocrypto quote does not match its app category".into());
    }
    Ok(venue)
}

fn resolve_storage_symbol(
    venue: &VenueSymbol,
    tokenized: bool,
    registered_storage_symbol: &str,
) -> Result<String, String> {
    let canonical = storage_symbol(venue, tokenized);
    if registered_storage_symbol != canonical {
        return Err("registered Tokocrypto storage symbol does not match its venue symbol".into());
    }
    Ok(canonical)
}

#[cfg(test)]
pub(super) fn normalize_asset(asset: &str) -> String {
    let mut value = asset.trim().to_ascii_uppercase();
    if let Some((_, suffix)) = value.rsplit_once(':') {
        value = suffix.to_string();
    }
    for suffix in [
        ".JK",
        "USDT-PERP",
        "-USD",
        "-USDT",
        "/USDT",
        "_USDT",
        "-IDR",
        "/IDR",
        "_IDR",
    ] {
        if let Some(stripped) = value.strip_suffix(suffix) {
            value = stripped.to_string();
            break;
        }
    }
    value
}

#[cfg(test)]
fn tokenized_asset(clean: &str) -> bool {
    tokenized_stock_symbols().any(|symbol| {
        normalize_asset(symbol) == clean
            || symbol
                .trim()
                .trim_end_matches("-USD")
                .eq_ignore_ascii_case(clean)
    })
}

#[cfg(test)]
fn select_symbol(
    symbols: &[VenueSymbol],
    clean: &str,
    requested_tokenized: bool,
) -> Result<(VenueSymbol, bool), String> {
    let tokenized = requested_tokenized || tokenized_asset(clean);
    let tokenized_base = clean.strip_suffix('B').unwrap_or(clean);
    let bases = if tokenized {
        vec![format!("{tokenized_base}B"), tokenized_base.to_string()]
    } else {
        vec![clean.to_string()]
    };

    let mut matches = symbols
        .iter()
        .filter(|symbol| {
            symbol.spot_trading_enabled
                && matches!(symbol.symbol_type, 1 | 2 | 3)
                && (if tokenized {
                    symbol.quote_asset == "USDT"
                } else {
                    matches!(symbol.quote_asset.as_str(), "IDR" | "USDT")
                })
                && bases.iter().any(|base| symbol.base_asset == *base)
        })
        .cloned()
        .collect::<Vec<_>>();
    matches.sort_by_key(|symbol| {
        (
            if tokenized && symbol.base_asset.ends_with('B') {
                0
            } else {
                1
            },
            if tokenized || symbol.quote_asset == "IDR" {
                0
            } else {
                1
            },
            match symbol.symbol_type {
                1 => 0,
                3 => 1,
                2 => 2,
                _ => 3,
            },
            symbol.symbol.clone(),
        )
    });
    if let Some(preferred) = matches.first().cloned() {
        matches.retain(|symbol| {
            symbol.quote_asset == preferred.quote_asset
                && symbol.symbol_type == preferred.symbol_type
        });
    }
    if tokenized
        && matches
            .iter()
            .any(|symbol| symbol.base_asset.ends_with('B'))
    {
        matches.retain(|symbol| symbol.base_asset.ends_with('B'));
    }
    matches.dedup_by(|left, right| left.symbol == right.symbol);
    match matches.len() {
        1 => Ok((matches.remove(0), tokenized)),
        0 => Err(format!(
            "Tokocrypto has no enabled {} market for {clean}",
            if tokenized {
                "tokenized USDT"
            } else {
                "IDR or USDT spot"
            }
        )),
        _ => Err(format!(
            "Tokocrypto returned multiple enabled markets for {clean}; symbol selection is ambiguous"
        )),
    }
}

fn storage_symbol(symbol: &VenueSymbol, tokenized: bool) -> String {
    let category = if tokenized { "TOKENIZED" } else { "SPOT" };
    format!("TOKOCRYPTO:{category}:{}", symbol.symbol)
}

fn persist_sync(
    connection: &Connection,
    storage_symbol: &str,
    instrument_kind: &str,
    series: &[(&str, &[MarketBar])],
) -> Result<(), String> {
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let result = (|| {
        let registered_instruments: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM instruments
                  WHERE exchange_id='tokocrypto' AND storage_symbol=?1
                    AND asset_category_id=?2 AND listing_status='venue_active'
                    AND venue_verified=1 AND is_active=1",
                params![storage_symbol, instrument_kind],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if registered_instruments != 1 {
            return Err(format!(
                "Tokocrypto native candles require exactly one active, venue-verified registered instrument; found {registered_instruments}"
            ));
        }

        for (interval, bars) in series {
            persist_series(connection, storage_symbol, interval, instrument_kind, bars)?;
        }
        Ok::<(), String>(())
    })();
    match result {
        Ok(()) => connection
            .execute_batch("COMMIT")
            .map_err(|error| error.to_string()),
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn persist_series(
    connection: &Connection,
    symbol: &str,
    interval: &str,
    instrument_kind: &str,
    bars: &[MarketBar],
) -> Result<(), String> {
    let now = super::format_timestamp(super::now_secs());
    let registered: Option<(String, String)> = connection
        .query_row(
            "SELECT instrument_id,product_id FROM instruments
          WHERE exchange_id='tokocrypto' AND storage_symbol=?1
            AND listing_status='venue_active' AND venue_verified=1 AND is_active=1 LIMIT 1",
            [symbol],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (instrument_id, product_id) = registered
        .map(|(id, product)| (Some(id), product))
        .unwrap_or((None, String::new()));
    connection
        .execute(
            "INSERT INTO series (
                symbol, interval, source, exchange_id, product_id, instrument_id, data_provider,
                timezone, price_basis, instrument_kind, last_success, last_attempt, last_error
             ) VALUES (?1, ?2, ?3, 'tokocrypto', ?4, ?5, 'tokocrypto',
                       'UTC', 'venue_native', ?6, ?7, ?7, NULL)
             ON CONFLICT(symbol, interval, source) DO UPDATE SET
                exchange_id=excluded.exchange_id,
                product_id=excluded.product_id,
                instrument_id=COALESCE(excluded.instrument_id,series.instrument_id),
                data_provider=excluded.data_provider,
                timezone=excluded.timezone,
                price_basis=excluded.price_basis,
                instrument_kind=excluded.instrument_kind,
                last_success=excluded.last_success,
                last_attempt=excluded.last_attempt,
                last_error=NULL",
            params![
                symbol,
                interval,
                SOURCE,
                product_id,
                instrument_id,
                instrument_kind,
                now
            ],
        )
        .map_err(|error| error.to_string())?;

    for bar in bars {
        let timestamp = bar
            .open_time_ms
            .checked_mul(quant_core::NANOS_PER_MILLISECOND)
            .ok_or_else(|| "Tokocrypto candle timestamp overflow".to_string())?;
        connection
            .execute(
                "INSERT INTO bars (
                    symbol, interval, source, ts, open, high, low, close, volume,
                    adj_close, dividends, splits, ingested_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, NULL, ?10)
                 ON CONFLICT(symbol, interval, source, ts) DO UPDATE SET
                    open=excluded.open, high=excluded.high, low=excluded.low,
                    close=excluded.close, volume=excluded.volume,
                    ingested_at=excluded.ingested_at",
                params![
                    symbol, interval, SOURCE, timestamp, bar.open, bar.high, bar.low, bar.close,
                    bar.volume, now
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    connection
        .execute(
            "INSERT INTO ingestion_runs (
                symbol, interval, source, exchange_id, product_id, data_provider,
                attempted_at, status, rows_received, error
             ) VALUES (?1, ?2, ?3, 'tokocrypto', ?4, 'tokocrypto', ?5, 'success', ?6, NULL)",
            params![symbol, interval, SOURCE, product_id, now, bars.len() as i64],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn record_failure(connection: &Connection, symbol: &str, interval: &str, error: &str) {
    let now = super::format_timestamp(super::now_secs());
    let _ = connection.execute(
        "INSERT INTO ingestion_runs (
            symbol, interval, source, exchange_id, data_provider,
            attempted_at, status, rows_received, error
         ) VALUES (?1, ?2, ?3, 'tokocrypto', 'tokocrypto', ?4, 'error', 0, ?5)",
        params![symbol, interval, SOURCE, now, error],
    );
    let _ = connection.execute(
        "UPDATE series SET exchange_id='tokocrypto', data_provider='tokocrypto',
                           last_attempt=?1, last_error=?2
          WHERE symbol=?3 AND interval=?4 AND source=?5",
        params![now, error, symbol, interval, SOURCE],
    );
}

#[cfg(test)]
#[path = "tokocrypto_storage_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tokocrypto_storage_registry_tests.rs"]
mod registry_tests;

#[cfg(test)]
#[path = "tokocrypto_storage_incremental_tests.rs"]
mod incremental_tests;
