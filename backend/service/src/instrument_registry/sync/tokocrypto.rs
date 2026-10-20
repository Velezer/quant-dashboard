use super::*;

pub(crate) fn sync_tokocrypto(conn: &Connection) -> SyncResult {
    let exchange_id = "tokocrypto".to_string();
    let symbols = match crate::tokocrypto::fetch_symbols() {
        Ok(symbols) => symbols,
        Err(error) => {
            record_listing_failure(
                conn,
                "tokocrypto",
                "",
                crate::tokocrypto::SYMBOLS_URL,
                &error,
            );
            return SyncResult {
                exchange_id,
                inserted: 0,
                deactivated: 0,
                error: Some(format!(
                    "Tokocrypto symbol refresh failed; previous listing retained: {error}"
                )),
            };
        }
    };
    reconcile_tokocrypto(conn, &symbols)
}

pub(crate) fn reconcile_tokocrypto(
    conn: &Connection,
    symbols: &[crate::tokocrypto::VenueSymbol],
) -> SyncResult {
    let exchange_id = "tokocrypto".to_string();
    let mut candidates = symbols
        .iter()
        .filter_map(|venue| {
            let (product_id, category, kind) = tokocrypto_product(venue)?;
            let symbol = tokocrypto_registry_symbol(venue, category);
            let key = format!("{product_id}:{symbol}");
            Some((key, product_id, category, kind, symbol, venue))
        })
        .collect::<Vec<_>>();
    // One app ticker can have multiple quote-market rows. Keep a single,
    // deterministic instrument identity per app product and ticker.
    candidates.sort_by(|left, right| {
        left.0.cmp(&right.0).then_with(|| {
            tokocrypto_market_rank(left.2, left.5).cmp(&tokocrypto_market_rank(right.2, right.5))
        })
    });
    candidates.dedup_by(|left, right| left.0 == right.0);
    if candidates.is_empty() {
        let error =
            "Tokocrypto symbol refresh contains no supported spot or tokenized-stock instruments"
                .to_string();
        record_listing_failure(
            conn,
            "tokocrypto",
            "",
            crate::tokocrypto::SYMBOLS_URL,
            &error,
        );
        return SyncResult {
            exchange_id,
            inserted: 0,
            deactivated: 0,
            error: Some(error),
        };
    }
    if let Err(error) = conn.execute_batch("BEGIN IMMEDIATE") {
        return SyncResult {
            exchange_id,
            inserted: 0,
            deactivated: 0,
            error: Some(error.to_string()),
        };
    }
    let result = (|| -> Result<(usize, usize), String> {
        let snapshot = today_date_utc();
        conn.execute(
            "UPDATE instruments SET listing_status='reconcile_pending',venue_verified=0
              WHERE exchange_id='tokocrypto' AND is_active=1
                AND registration_source IN (?1,'tokocrypto_public_symbols')
                AND listing_status IN ('venue_active','public_listed','reconcile_pending')",
            [TOKOCRYPTO_LISTING_SOURCE],
        )
        .map_err(|error| error.to_string())?;
        let mut inserted = 0usize;
        let mut product_counts = std::collections::BTreeMap::<&str, usize>::new();
        for (_, product_id, category, kind, symbol, venue) in candidates {
            let instrument_id = format!("tokocrypto.{category}.{}", symbol.to_ascii_lowercase());
            let storage_symbol = format!("TOKOCRYPTO:{kind}:{}", venue.symbol);
            let compact_venue_symbol = venue.symbol.replace('_', "");
            let yahoo_symbol = format!("{symbol}-USD");
            let idr_symbol = format!("{symbol}-IDR");
            let usdt_symbol = format!("{symbol}-USDT");
            let metadata = crate::wire::encode(&json!({"symbol_type":venue.symbol_type,
                "quote_asset":venue.quote_asset,"data_provider":"tokocrypto"}))
            .map_err(|error| error.to_string())?;
            let existing: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM instruments WHERE instrument_id=?1)",
                    [&instrument_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            conn.execute(
                r#"INSERT INTO instruments(instrument_id,exchange_id,product_id,asset_category_id,
                    symbol,venue_symbol,underlying_symbol,storage_symbol,quote_currency,
                    listing_status,venue_verified,execution_supported,registration_source,
                    source_url,source_snapshot_at,price_source_policy,contract_metadata_json,
                    registered_at,updated_at,is_active)
                   VALUES(?1,'tokocrypto',?2,?3,?4,?5,?4,?6,?7,'venue_active',1,0,?8,?9,?10,
                    'tokocrypto_native',?11,unixepoch(),unixepoch(),1)
                   ON CONFLICT(instrument_id) DO UPDATE SET product_id=excluded.product_id,
                    venue_symbol=excluded.venue_symbol,underlying_symbol=excluded.underlying_symbol,
                    storage_symbol=excluded.storage_symbol,quote_currency=excluded.quote_currency,
                    listing_status='venue_active',venue_verified=1,registration_source=excluded.registration_source,
                    source_url=excluded.source_url,source_snapshot_at=excluded.source_snapshot_at,
                    price_source_policy=excluded.price_source_policy,contract_metadata_json=excluded.contract_metadata_json,
                    updated_at=unixepoch(),is_active=1"#,
                params![instrument_id,product_id,category,symbol,venue.symbol,storage_symbol,venue.quote_asset,
                    TOKOCRYPTO_LISTING_SOURCE,crate::tokocrypto::SYMBOLS_URL,snapshot,metadata],
            ).map_err(|error|error.to_string())?;
            if !existing {
                inserted += 1;
            }
            conn.execute(
                "UPDATE series SET product_id=?1,instrument_id=?2
                  WHERE exchange_id='tokocrypto' AND upper(symbol) IN
                    (upper(?3),upper(?4),upper(?5),upper(?6),upper(?7),upper(?8),upper(?9))",
                params![
                    product_id,
                    instrument_id,
                    storage_symbol,
                    venue.symbol,
                    compact_venue_symbol,
                    symbol,
                    yahoo_symbol,
                    idr_symbol,
                    usdt_symbol
                ],
            )
            .map_err(|error| error.to_string())?;
            conn.execute(
                "UPDATE ingestion_runs SET product_id=?1 WHERE exchange_id='tokocrypto' AND upper(symbol) IN
                    (upper(?2),upper(?3),upper(?4),upper(?5),upper(?6),upper(?7),upper(?8))",
                params![product_id,storage_symbol,venue.symbol,compact_venue_symbol,symbol,
                    yahoo_symbol,idr_symbol,usdt_symbol],
            ).map_err(|error|error.to_string())?;
            *product_counts.entry(product_id).or_default() += 1;
        }
        let deactivated=conn.execute(
            "UPDATE instruments SET listing_status='venue_delisted',venue_verified=0,updated_at=unixepoch()
              WHERE exchange_id='tokocrypto' AND is_active=1 AND registration_source=?1
                AND listing_status='reconcile_pending'",
            [TOKOCRYPTO_LISTING_SOURCE],
        ).map_err(|error|error.to_string())?;
        clear_listing_failure(conn, "tokocrypto")?;
        for (product, count) in product_counts {
            record_listing_success(
                conn,
                "tokocrypto",
                product,
                crate::tokocrypto::SYMBOLS_URL,
                &snapshot,
                count,
            )?;
        }
        super::super::schema::sync_normalized_dimensions(conn)
            .map_err(|error| error.to_string())?;
        Ok((inserted, deactivated))
    })();
    match result {
        Ok((inserted, deactivated)) => match conn.execute_batch("COMMIT") {
            Ok(()) => {
                let _ = crate::record_daily_backlog(conn);
                SyncResult {
                    exchange_id,
                    inserted,
                    deactivated,
                    error: None,
                }
            }
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                let message = error.to_string();
                record_listing_failure(
                    conn,
                    "tokocrypto",
                    "",
                    crate::tokocrypto::SYMBOLS_URL,
                    &message,
                );
                SyncResult {
                    exchange_id,
                    inserted: 0,
                    deactivated: 0,
                    error: Some(message),
                }
            }
        },
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            record_listing_failure(
                conn,
                "tokocrypto",
                "",
                crate::tokocrypto::SYMBOLS_URL,
                &error,
            );
            SyncResult {
                exchange_id,
                inserted: 0,
                deactivated: 0,
                error: Some(error),
            }
        }
    }
}

fn tokocrypto_product(
    venue: &crate::tokocrypto::VenueSymbol,
) -> Option<(&'static str, &'static str, &'static str)> {
    if !venue.spot_trading_enabled || !matches!(venue.symbol_type, 1 | 2 | 3) {
        return None;
    }
    // The exchange can list international-name assets. Preserve those rows
    // while parsing its complete response, but only register ASCII tickers our
    // app can address in chart/provider routes.
    if !venue
        .base_asset
        .bytes()
        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        || !venue
            .symbol
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return None;
    }
    // Symbol type identifies Tokocrypto's market system (MAIN/NEXT/NextMe),
    // not whether the asset is a tokenized stock. Use the configured app-side
    // stock-token catalog for that classification.
    if venue.quote_asset == "USDT" && tokocrypto_known_stock_base(&venue.base_asset) {
        return Some(("tokocrypto_tokenized_stock", "tokenized_stock", "TOKENIZED"));
    }
    matches!(venue.quote_asset.as_str(), "IDR" | "USDT").then_some((
        "tokocrypto_spot",
        "crypto_spot",
        "SPOT",
    ))
}

fn tokocrypto_known_stock_base(base_asset: &str) -> bool {
    let base = base_asset.strip_suffix('B').unwrap_or(base_asset);
    crate::master_data::tokenized_stock_symbols()
        .any(|ticker| ticker.trim_end_matches("-USD").eq_ignore_ascii_case(base))
}

fn tokocrypto_registry_symbol(venue: &crate::tokocrypto::VenueSymbol, category: &str) -> String {
    if category == "tokenized_stock" {
        if let Some(ticker) = crate::master_data::tokenized_stock_symbols().find(|ticker| {
            ticker.trim_end_matches("-USD").eq_ignore_ascii_case(
                venue
                    .base_asset
                    .strip_suffix('B')
                    .unwrap_or(&venue.base_asset),
            )
        }) {
            return ticker.trim_end_matches("-USD").to_ascii_uppercase();
        }
    }
    venue.base_asset.to_ascii_uppercase()
}

fn tokocrypto_market_rank<'a>(
    category: &str,
    venue: &'a crate::tokocrypto::VenueSymbol,
) -> (u8, u8, u8, &'a str) {
    let token_rank = if category == "tokenized_stock" && venue.base_asset.ends_with('B') {
        0
    } else {
        1
    };
    let type_rank = match venue.symbol_type {
        1 => 0,
        3 => 1,
        2 => 2,
        _ => 3,
    };
    let quote_rank = match venue.quote_asset.as_str() {
        "IDR" => 0,
        "USDT" => 1,
        _ => 2,
    };
    (token_rank, quote_rank, type_rank, &venue.symbol)
}
