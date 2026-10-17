use super::*;

pub(crate) fn research_eligibility(
    exchange: &str,
    listing_status: &str,
    venue_verified: bool,
) -> &'static str {
    if listing_status == "venue_delisted" {
        "excluded"
    } else if matches!(
        crate::normalize_exchange_id(exchange).as_str(),
        "pintu" | "pluang"
    ) && !venue_verified
    {
        "pending_verification"
    } else {
        "eligible"
    }
}

pub(crate) fn seed_pintu(conn: &Connection) -> Result<(), rusqlite::Error> {
    let source_url = "https://www.pintu.co.id/en/pages/daftar-kontrak-futures";
    let encoded = conn.query_row(
        "SELECT payload_json FROM reference_catalogs
          WHERE catalog_id='pintu_futures_contracts'",
        [],
        |row| row.get::<_, String>(0),
    )?;
    let contracts: Value =
        crate::wire::decode(&encoded).expect("SQLite Pintu contracts catalog must be valid JSON");
    let contracts = contracts
        .as_array()
        .expect("SQLite Pintu contracts catalog must be an array");

    for contract in contracts {
        let field = |name: &str| contract.get(name).and_then(Value::as_str).unwrap_or("");
        let venue_symbol = field("venue_symbol");
        let Some(base) = venue_symbol.strip_suffix("USDT-PERP") else {
            continue;
        };
        let base = base.trim();
        if base.is_empty() {
            continue;
        }
        let min_lot = field("minimum_lot").parse::<u64>().unwrap_or(0);
        let max_position = field("maximum_position_usdt").parse::<f64>().unwrap_or(0.0);
        let price_tick = field("price_tick").parse::<f64>().unwrap_or(0.0);
        let metadata = crate::wire::encode(&json!({
            "contract_unit": field("contract_unit"),
            "minimum_lot": min_lot,
            "maximum_position_usdt": max_position,
            "price_tick": price_tick,
            "quote_currency": "USDT"
        }))
        .unwrap_or_default();
        insert_seed(
            conn,
            &format!("pintu.pintu_pro_futures.{}", base.to_ascii_lowercase()),
            "pintu",
            "crypto_perpetual",
            base,
            venue_symbol,
            &format!("{base}USDT"),
            &format!("{base}-USD"),
            "USDT",
            "regulator_approved",
            false,
            "pintu_regulator_approved_code_list",
            source_url,
            "2026-09-25",
            &metadata,
        )?;
    }
    Ok(())
}
pub(crate) fn seed_pluang(conn: &Connection) -> Result<(), rusqlite::Error> {
    let encoded = conn.query_row(
        "SELECT payload_json FROM reference_catalogs
          WHERE catalog_id='pluang_crypto_listings'",
        [],
        |row| row.get::<_, String>(0),
    )?;
    let seed: Value =
        crate::wire::decode(&encoded).expect("SQLite Pluang listing catalog must be valid JSON");
    let snapshot_at = seed
        .get("snapshot_date")
        .and_then(Value::as_str)
        .unwrap_or("");
    let spot_source = seed
        .get("spot_source_url")
        .and_then(Value::as_str)
        .unwrap_or("https://pluang.com/en/explore/crypto/spot");
    let futures_source = seed
        .get("futures_source_url")
        .and_then(Value::as_str)
        .unwrap_or("https://pluang.com/en/explore/crypto/futures");
    let spot_symbols = seed
        .get("spot_symbols")
        .and_then(Value::as_str)
        .unwrap_or("");
    for base in spot_symbols.split_ascii_whitespace() {
        insert_seed(
            conn,
            &format!("pluang.pluang_spot.{}", base.to_ascii_lowercase()),
            "pluang",
            "crypto_spot",
            base,
            base,
            base,
            &format!("{base}-USD"),
            "IDR",
            "public_listed_snapshot",
            true,
            "pluang_public_spot_market_snapshot",
            spot_source,
            snapshot_at,
            "<value kind=\"object\"></value>",
        )?;
    }

    let futures_symbols = seed
        .get("futures_symbols")
        .and_then(Value::as_str)
        .unwrap_or("");
    for venue_symbol in futures_symbols.split_ascii_whitespace() {
        let Some(base) = venue_symbol.strip_suffix("USDT-PERP") else {
            continue;
        };
        insert_seed(
            conn,
            &format!("pluang.pluang_perpetual.{}", base.to_ascii_lowercase()),
            "pluang",
            "crypto_perpetual",
            base,
            venue_symbol,
            base,
            &format!("{base}-USD"),
            "USDT",
            "public_listed_snapshot",
            true,
            "pluang_public_futures_market_snapshot",
            futures_source,
            snapshot_at,
            "<value kind=\"object\"></value>",
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_seed(
    conn: &Connection,
    instrument_id: &str,
    exchange_id: &str,
    asset_category_id: &str,
    symbol: &str,
    venue_symbol: &str,
    underlying_symbol: &str,
    storage_symbol: &str,
    quote_currency: &str,
    listing_status: &str,
    venue_verified: bool,
    registration_source: &str,
    source_url: &str,
    source_snapshot_at: &str,
    contract_metadata_json: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        r#"INSERT OR IGNORE INTO instruments (
               instrument_id, exchange_id, product_id, asset_category_id, symbol,
               venue_symbol, underlying_symbol, storage_symbol, quote_currency,
               listing_status, venue_verified, execution_supported,
               registration_source, source_url, source_snapshot_at,
               price_source_policy, contract_metadata_json, registered_at,
               updated_at, is_active
           ) VALUES (
               ?1, ?2, CASE WHEN ?2='pintu' THEN 'pintu_pro_futures'
                            WHEN ?2='pluang' AND ?3='crypto_spot' THEN 'pluang_spot'
                            WHEN ?2='pluang' THEN 'pluang_perpetual' ELSE '' END,
               ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0, ?11, ?12,
               ?13, 'yfinance_research_proxy', ?14, unixepoch(), unixepoch(), 1
           )"#,
        params![
            instrument_id,
            exchange_id,
            asset_category_id,
            symbol,
            venue_symbol,
            underlying_symbol,
            storage_symbol,
            quote_currency,
            listing_status,
            if venue_verified { 1_i64 } else { 0_i64 },
            registration_source,
            source_url,
            source_snapshot_at,
            contract_metadata_json
        ],
    )?;
    Ok(())
}

/// Look up the registered exchange and category for a ticker.
///
/// The caller supplies the raw storage symbol (e.g. `"BBCA.JK"`, `"BTC-USD"`,
/// `"BTCUSDT"`) **or** a clean base ticker (e.g. `"BTC"`, `"BBCA"`).  The
/// function tries several normalizations in order and returns the first
/// active registry row that matches:
///
/// 1. Exact `storage_symbol` match.
/// 2. Exact `symbol` match (base ticker stored in the registry).
/// 3. Symbol derived by stripping common suffixes (`.JK`, `-USD`, `USDT`).
///
/// Returns `None` when no active registration exists for the ticker, so the
/// caller can fall back to its heuristics.
#[cfg(test)]
pub(crate) fn lookup_for_exchange(
    conn: &Connection,
    exchange: &str,
    raw: &str,
) -> Option<(String, String, String)> {
    let exchange = crate::normalize_exchange_id(exchange);
    let raw = raw.trim().to_ascii_uppercase();
    let base = raw
        .strip_suffix(".JK")
        .or_else(|| raw.strip_suffix("-USD"))
        .or_else(|| raw.strip_suffix("USDT-PERP"))
        .unwrap_or(&raw);
    let mut stmt = conn
        .prepare_cached(
            "SELECT asset_category_id, storage_symbol, symbol FROM instruments
          WHERE exchange_id=?1 AND is_active=1 AND listing_status <> 'venue_delisted'
            AND (exchange_id NOT IN ('pintu','pluang') OR venue_verified=1)
            AND (upper(storage_symbol)=?2 OR upper(symbol)=?2 OR upper(symbol)=?3)
          ORDER BY CASE WHEN upper(storage_symbol)=?2 THEN 0
                        WHEN upper(symbol)=?2 THEN 1 ELSE 2 END,
                   venue_verified DESC
          LIMIT 1",
        )
        .ok()?;
    stmt.query_row(params![exchange, raw, base], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })
    .optional()
    .ok()
    .flatten()
}

pub(crate) fn lookup_tokocrypto_market(
    conn: &Connection,
    product_id: &str,
    storage_symbol: &str,
) -> Result<Option<(String, String, String)>, rusqlite::Error> {
    conn.query_row(
        "SELECT venue_symbol,quote_currency,contract_metadata_json FROM instruments
          WHERE exchange_id='tokocrypto' AND product_id=?1 AND storage_symbol=?2
            AND is_active=1 AND venue_verified=1
            AND listing_status IN ('venue_active','public_listed_snapshot')
          LIMIT 1",
        params![product_id, storage_symbol],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()
}

pub(crate) fn storage_symbol_for_exchange_category(
    conn: &Connection,
    exchange: &str,
    category: &str,
    alias: &str,
) -> Result<Option<String>, rusqlite::Error> {
    let exchange = crate::normalize_exchange_id(exchange);
    let alias = alias.trim().to_ascii_uppercase();
    let mut statement = conn.prepare(
        r#"SELECT DISTINCT storage_symbol, listing_status, venue_verified FROM app.instruments
            WHERE exchange_id=?1 AND asset_category_id=?2 AND is_active=1
              AND (upper(storage_symbol)=?3 OR upper(symbol)=?3
                   OR upper(venue_symbol)=?3 OR upper(underlying_symbol)=?3
                   OR upper(replace(venue_symbol,'_',''))=?3)
            ORDER BY storage_symbol"#,
    )?;
    let matches = statement
        .query_map(params![exchange, category, alias], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|(_, listing_status, venue_verified)| {
            research_eligibility(&exchange, listing_status, *venue_verified) == "eligible"
        })
        .map(|(storage_symbol, _, _)| storage_symbol)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [storage_symbol] => Ok(Some(storage_symbol.clone())),
        [] => Ok(None),
        _ => Err(rusqlite::Error::InvalidParameterName(
            "ticker alias matches multiple registered storage symbols".into(),
        )),
    }
}
