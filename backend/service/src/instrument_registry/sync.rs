use super::*;

mod tokocrypto;
#[cfg(test)]
pub(crate) use tokocrypto::reconcile_tokocrypto;
pub(crate) use tokocrypto::sync_tokocrypto;

/// Sync the instrument registry from live sources for every enabled exchange.
///
/// Called once per day from the background registry-sync daemon.
///
/// | Exchange       | Source                                        |
/// |----------------|-----------------------------------------------|
/// | 	okocrypto   | Live /open/v1/common/symbols API via adapter |
/// | pintu        | Retail, Pro Spot, and Pro Futures app listings |
/// | pluang       | Saved app snapshot; no live sync              |
///
/// Each exchange sync is independent. A failure for one exchange is recorded
/// in the result but does not abort the others.
pub(crate) fn sync_from_live_sources(conn: &Connection) -> Vec<SyncResult> {
    let mut results = Vec::new();
    results.push(sync_pintu(conn));
    results.push(sync_stockbit(conn));
    results.push(sync_tokocrypto(conn));
    results
}

#[path = "sync/pintu.rs"]
mod pintu;
#[cfg(test)]
pub(crate) use pintu::reconcile_pintu;
pub(crate) use pintu::sync_pintu;

#[cfg(test)]
pub(crate) fn listing_status(
    conn: &Connection,
    _market_directory: &str,
) -> Result<Value, rusqlite::Error> {
    listing_status_for_exchange_inner(conn, _market_directory, None, true)
}

pub(crate) fn listing_status_for_exchange(
    conn: &Connection,
    _market_directory: &str,
    exchange_filter: Option<&str>,
) -> Result<Value, rusqlite::Error> {
    listing_status_for_exchange_inner(conn, _market_directory, exchange_filter, false)
}

fn listing_status_for_exchange_inner(
    conn: &Connection,
    _market_directory: &str,
    exchange_filter: Option<&str>,
    include_signal_counts: bool,
) -> Result<Value, rusqlite::Error> {
    let exchange_clause = if exchange_filter.is_some() {
        " AND i.exchange_id=?1"
    } else {
        ""
    };
    let signal_count_column = if include_signal_counts {
        "(SELECT COUNT(*) FROM signals.signal_screener_rows sr WHERE sr.exchange_id=i.exchange_id
                    AND sr.product_id=i.product_id)"
    } else {
        "NULL"
    };
    let query = format!(
        "SELECT i.exchange_id,i.product_id,COUNT(*),
                SUM(CASE WHEN i.venue_verified=1 AND
                    i.listing_status IN ('venue_active','public_listed_snapshot') THEN 1 ELSE 0 END),
                MAX(i.source_snapshot_at),
                0,0,{signal_count_column}
           FROM instruments i
          WHERE i.exchange_id IN ('stockbit','tokocrypto','pintu','pluang')
             AND i.is_active=1{exchange_clause}
          GROUP BY i.exchange_id,i.product_id ORDER BY i.exchange_id,i.product_id"
    );
    let mut stmt = conn.prepare(&query)?;
    let map_row = |row: &rusqlite::Row<'_>| {
        Ok(json!({
            "exchange_id": row.get::<_, String>(0)?,
            "product_id": row.get::<_, String>(1)?,
            "registered": row.get::<_, i64>(2)?,
            "confirmed": row.get::<_, i64>(3)?,
            "snapshot_date": row.get::<_, Option<String>>(4)?,
            "with_15m_candles": row.get::<_, i64>(5)?,
            "with_daily_candles": row.get::<_, i64>(6)?,
            "signals": row.get::<_, Option<i64>>(7)?,
        }))
    };
    let rows = match exchange_filter {
        Some(exchange) => stmt.query_map([exchange], map_row)?,
        None => stmt.query_map([], map_row)?,
    };
    let mut products = rows.collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut coverage_by_symbol =
        std::collections::HashMap::<(String, String, String), (bool, bool)>::new();
    for product in &mut products {
        let exchange = product["exchange_id"].as_str().unwrap_or("");
        let product_id = product["product_id"].as_str().unwrap_or("");
        let mut instruments = conn.prepare(
            "SELECT storage_symbol,venue_verified FROM instruments
             WHERE exchange_id=?1 AND product_id=?2 AND is_active=1",
        )?;
        let rows = instruments.query_map(params![exchange, product_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
        })?;
        // Opening a ticker also opens the app database. Finish this read before
        // that connection performs its initialization writes under DELETE journals.
        let registered = rows.collect::<Result<Vec<_>, _>>()?;
        drop(instruments);
        let mut with_15m = 0_i64;
        let mut with_daily = 0_i64;
        for (symbol, venue_verified) in registered {
            if !venue_verified {
                continue;
            }
            let coverage_key = (symbol.clone(), exchange.to_owned(), product_id.to_owned());
            let coverage = if let Some(coverage) = coverage_by_symbol.get(&coverage_key) {
                *coverage
            } else {
                #[cfg(test)]
                let coverage = market_candle_coverage(conn, &symbol, exchange, product_id)?;
                #[cfg(not(test))]
                let coverage = match crate::database::open_market_history_for_symbol(
                    _market_directory,
                    &symbol,
                    false,
                )? {
                    Some(market) => market_candle_coverage(&market, &symbol, exchange, product_id)?,
                    None => (false, false),
                };
                coverage_by_symbol.insert(coverage_key, coverage);
                coverage
            };
            with_15m += if coverage.0 { 1 } else { 0 };
            with_daily += if coverage.1 { 1 } else { 0 };
        }
        if let Some(object) = product.as_object_mut() {
            object.insert("with_15m_candles".into(), json!(with_15m));
            object.insert("with_daily_candles".into(), json!(with_daily));
        }
    }
    for (exchange, product, category) in [
        ("stockbit", "stockbit_regular", "idx_stock"),
        ("tokocrypto", "tokocrypto_spot", "crypto_spot"),
        (
            "tokocrypto",
            "tokocrypto_tokenized_stock",
            "tokenized_stock",
        ),
        ("pintu", "pintu_retail_spot", "crypto_spot"),
        ("pintu", "pintu_pro_spot", "crypto_spot"),
        ("pintu", "pintu_pro_futures", "crypto_perpetual"),
        ("pluang", "pluang_spot", "crypto_spot"),
        ("pluang", "pluang_perpetual", "crypto_perpetual"),
    ] {
        if exchange_filter.is_some_and(|filter| filter != exchange) {
            continue;
        }
        let present = products.iter().any(|row| {
            row.get("exchange_id").and_then(Value::as_str) == Some(exchange)
                && row.get("product_id").and_then(Value::as_str) == Some(product)
        });
        if !present {
            products.push(json!({
                "exchange_id":exchange,"product_id":product,"asset_category_id":category,
                "registered":0,"confirmed":0,"snapshot_date":Value::Null,
                "with_15m_candles":0,"with_daily_candles":0,"signals":0
            }));
        }
    }
    for product_row in &mut products {
        let exchange = product_row
            .get("exchange_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        let product = product_row
            .get("product_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        let latest: Option<Value> = conn.query_row(
            "SELECT status,attempted_at,completed_at,snapshot_at,records_received,error
               FROM listing_sync_runs WHERE exchange_id=?1 AND product_id IN (?2,'')
              ORDER BY id DESC LIMIT 1",
            params![exchange, product],
            |row| Ok(json!({
                "status":row.get::<_,String>(0)?,"attempted_at":row.get::<_,String>(1)?,
                "completed_at":row.get::<_,Option<String>>(2)?,"snapshot_at":row.get::<_,Option<String>>(3)?,
                "records_received":row.get::<_,i64>(4)?,"error":row.get::<_,String>(5)?,
            })),
        ).optional()?;
        if let Some(object) = product_row.as_object_mut() {
            object.insert("latest_sync".into(), latest.unwrap_or(Value::Null));
        }
    }
    products.sort_by(|a, b| {
        a.get("exchange_id")
            .and_then(Value::as_str)
            .cmp(&b.get("exchange_id").and_then(Value::as_str))
            .then_with(|| {
                a.get("product_id")
                    .and_then(Value::as_str)
                    .cmp(&b.get("product_id").and_then(Value::as_str))
            })
    });
    let mut refresh_errors = serde_json::Map::new();
    for exchange in ["stockbit", "tokocrypto", "pintu"] {
        let key = format!("{exchange}_refresh_error");
        let error: String = conn
            .query_row(
                "SELECT value FROM instrument_registry_meta WHERE key=?1",
                [&key],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_default();
        refresh_errors.insert(exchange.to_string(), Value::String(error));
    }
    let pintu_refresh_error = refresh_errors
        .get("pintu")
        .and_then(Value::as_str)
        .unwrap_or("");
    let backlog: Option<String> = conn
        .query_row(
            "SELECT value FROM instrument_registry_meta WHERE key='daily_backlog_remaining'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(
        json!({ "products": products, "refresh_errors": refresh_errors,
        "pintu_refresh_error": pintu_refresh_error,
        "daily_backlog_remaining": backlog.and_then(|value| value.parse::<i64>().ok()) }),
    )
}

fn market_candle_coverage(
    connection: &Connection,
    storage_symbol: &str,
    exchange_id: &str,
    product_id: &str,
) -> Result<(bool, bool), rusqlite::Error> {
    let has_interval = |interval| -> Result<bool, rusqlite::Error> {
        // Coverage counts directly stored candles in the requested product or
        // shared category, independently of chart rollups.
        Ok(
            crate::selected_candle_source_metadata_for_product_at_interval(
                connection,
                storage_symbol,
                exchange_id,
                product_id,
                interval,
            )?
            .is_some(),
        )
    };
    Ok((has_interval("15m")?, has_interval("1d")?))
}

pub(crate) const STOCKBIT_PRODUCT_ID: &str = "stockbit_regular";
pub(crate) const STOCKBIT_LISTING_SOURCE: &str = "idx_listed_company_directory";
const LEGACY_STOCKBIT_LISTING_SOURCE: &str = "stockbit_public_symbol_sitemap";
pub(crate) const TOKOCRYPTO_LISTING_SOURCE: &str = "tokocrypto_live_symbols_api";

pub(crate) fn record_listing_failure(
    conn: &Connection,
    exchange: &str,
    product: &str,
    source: &str,
    error: &str,
) {
    let key = format!("{exchange}_refresh_error");
    let _ = conn.execute(
        "INSERT INTO instrument_registry_meta(key,value) VALUES(?1,?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, error],
    );
    let _ = conn.execute(
        "INSERT INTO instrument_registry_meta(key,value) VALUES(?1,datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [format!("{exchange}_refresh_attempted_at")],
    );
    let _ = conn.execute(
        "INSERT INTO listing_sync_runs(exchange_id,product_id,source_url,attempted_at,
            completed_at,status,error) VALUES(?1,?2,?3,datetime('now'),datetime('now'),'error',?4)",
        params![exchange, product, source, error],
    );
}

pub(crate) fn clear_listing_failure(conn: &Connection, exchange: &str) -> Result<(), String> {
    let key = format!("{exchange}_refresh_error");
    conn.execute(
        "INSERT INTO instrument_registry_meta(key,value) VALUES(?1,'')
         ON CONFLICT(key) DO UPDATE SET value=''",
        [&key],
    )
    .map_err(|error| error.to_string())?;
    let key = format!("{exchange}_refresh_attempted_at");
    conn.execute(
        "INSERT INTO instrument_registry_meta(key,value) VALUES(?1,datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [&key],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn record_listing_success(
    conn: &Connection,
    exchange: &str,
    product: &str,
    source: &str,
    snapshot: &str,
    received: usize,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO listing_sync_runs(exchange_id,product_id,source_url,attempted_at,
            completed_at,snapshot_at,status,records_received,error)
         VALUES(?1,?2,?3,datetime('now'),datetime('now'),?4,'success',?5,'')",
        params![exchange, product, source, snapshot, received as i64],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[path = "sync/stockbit.rs"]
mod stockbit;
#[cfg(test)]
pub(crate) use stockbit::reconcile_stockbit;
pub(crate) use stockbit::sync_stockbit;

pub(crate) fn today_date_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Convert current UTC days using the shared Gregorian calendar helper.
    let days = secs / crate::SECONDS_PER_DAY as u64;
    let (year, month, day) = quant_core::gregorian_date_from_unix_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}")
}
