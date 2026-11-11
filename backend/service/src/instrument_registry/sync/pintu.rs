use super::*;

pub(crate) fn sync_pintu(conn: &Connection) -> SyncResult {
    sync_pintu_with_fetch(conn, crate::pintu_listing::fetch_all)
}

pub(crate) fn sync_pintu_with_fetch(
    conn: &Connection,
    fetch: impl FnOnce() -> Result<Vec<crate::pintu_listing::Listing>, String>,
) -> SyncResult {
    let exchange_id = "pintu".to_string();
    // Published delistings remain valid even when a live feed is unavailable.
    let announced_delisted = match apply_announced_futures_delistings(conn) {
        Ok(count) => count,
        Err(error) => return SyncResult {
            exchange_id,
            inserted: 0,
            deactivated: 0,
            error: Some(format!("Failed to apply Pintu's announced futures delistings: {error}")),
        },
    };
    let listings = match fetch() {
        Ok(list) => list,
        Err(error) => {
            let _ = conn.execute("INSERT INTO instrument_registry_meta(key,value)
                VALUES('pintu_refresh_error',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [&error]);
            let _ = conn.execute(
                "INSERT INTO instrument_registry_meta(key,value)
                VALUES('pintu_refresh_attempted_at',datetime('now'))
                ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [],
            );
            let _ = conn.execute(
                "INSERT INTO listing_sync_runs(exchange_id,product_id,source_url,attempted_at,
                    completed_at,status,error)
                 VALUES('pintu','','pintu_listing_bundle',datetime('now'),datetime('now'),'error',?1)",
                [&error],
            );
            return SyncResult {
                exchange_id,
                inserted: 0,
                deactivated: announced_delisted,
                error: Some(format!(
                    "Pintu listing refresh failed; other previous listings retained: {error}"
                )),
            };
        }
    };
    let mut result = reconcile_pintu(conn, &listings);
    result.deactivated = result.deactivated.saturating_add(announced_delisted);
    result
}

/// Apply known first-party notices independently of the live ticker feed.
/// Keep unrelated registrations and candle history intact.
fn apply_announced_futures_delistings(conn: &Connection) -> Result<usize, rusqlite::Error> {
    let rows = {
        let mut stmt = conn.prepare(
            "SELECT instrument_id,symbol,venue_symbol FROM instruments
              WHERE exchange_id='pintu' AND product_id='pintu_pro_futures' AND is_active=1
                AND (listing_status<>'venue_delisted' OR venue_verified<>0)",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?.collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let ids = rows.into_iter().filter_map(|(id, symbol, venue)| {
        let venue = venue.trim().to_ascii_uppercase();
        let contract = if venue.is_empty() {
            format!("{}USDT-PERP", symbol.trim().to_ascii_uppercase())
        } else {
            venue
        };
        crate::pintu_listing::is_announced_delisted_pintu_future(&contract).then_some(id)
    }).collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(0);
    }

    conn.execute_batch("SAVEPOINT pintu_announced_delistings")?;
    let updated = (|| {
        let mut count = 0usize;
        for id in ids {
            count += conn.execute(
                "UPDATE instruments SET listing_status='venue_delisted',venue_verified=0,
                        execution_supported=0,updated_at=unixepoch()
                  WHERE instrument_id=?1 AND exchange_id='pintu'
                    AND product_id='pintu_pro_futures'",
                [&id],
            )?;
        }
        Ok::<usize, rusqlite::Error>(count)
    })();
    match updated {
        Ok(count) => {
            if let Err(error) = conn.execute_batch("RELEASE SAVEPOINT pintu_announced_delistings") {
                let _ = conn.execute_batch("ROLLBACK TO SAVEPOINT pintu_announced_delistings; RELEASE SAVEPOINT pintu_announced_delistings");
                return Err(error);
            }
            Ok(count)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK TO SAVEPOINT pintu_announced_delistings; RELEASE SAVEPOINT pintu_announced_delistings");
            Err(error)
        }
    }
}

pub(crate) fn reconcile_pintu(
    conn: &Connection,
    listings: &[crate::pintu_listing::Listing],
) -> SyncResult {
    let exchange_id = "pintu".to_string();
    // This app connection also has the signal store attached. Reserve only the
    // app writer when the first registry update runs.
    if let Err(error) = conn.execute_batch("BEGIN DEFERRED") {
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
            "UPDATE instruments SET listing_status='reconcile_pending'
              WHERE exchange_id='pintu' AND is_active=1
                AND registration_source LIKE '%app_listing%'
                AND listing_status IN ('venue_active','classification_pending')",
            [],
        )
        .map_err(|e| e.to_string())?;
        let mut inserted = 0;
        for listing in listings {
            if listing.product_id == "pintu_pro_futures"
                && crate::pintu_listing::is_announced_delisted_pintu_future(&listing.venue_symbol)
            {
                continue;
            }
            let instrument_id = format!(
                "pintu.{}.{}",
                listing.product_id,
                listing.base.to_ascii_lowercase()
            );
            let registration_source = match listing.product_id {
                "pintu_retail_spot" => "pintu_retail_market_app_listing",
                "pintu_pro_spot" => "pintu_pro_spot_app_listing",
                _ => "pintu_pro_futures_app_listing",
            };
            let metadata = crate::wire::encode(&json!({
                "listing_source": listing.source_url,
                "classification": if listing.product_id == "pintu_retail_spot" {
                    "pintu_retail_crypto_market"
                } else { "pintu_pro_crypto_market" },
            }))
            .map_err(|e| e.to_string())?;
            let existing_source: Option<String> = conn
                .query_row(
                    "SELECT registration_source FROM instruments WHERE instrument_id=?1",
                    [&instrument_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let effective_source =
                if existing_source.as_deref() == Some("pintu_regulator_approved_code_list") {
                    "pintu_regulator_approved_code_list+app_listing"
                } else {
                    registration_source
                };
            let underlying = if listing.category == "crypto_perpetual" {
                format!("{}USDT", listing.base)
            } else {
                listing.base.clone()
            };
            let storage = format!("{}-USD", listing.base);
            inserted += conn.execute(
                r#"INSERT INTO instruments(instrument_id,exchange_id,product_id,asset_category_id,
                     symbol,venue_symbol,underlying_symbol,storage_symbol,quote_currency,
                     listing_status,venue_verified,execution_supported,registration_source,
                     source_url,source_snapshot_at,price_source_policy,
                     contract_metadata_json,registered_at,updated_at,is_active)
                   VALUES(?1,'pintu',?2,?3,?4,?5,?6,?7,?8,?9,?10,0,?11,?12,?13,
                          'pintu_native_then_research_proxy',?14,unixepoch(),unixepoch(),1)
                   ON CONFLICT(instrument_id) DO UPDATE SET
                     venue_symbol=excluded.venue_symbol,
                     price_source_policy=excluded.price_source_policy,
                     listing_status=excluded.listing_status,
                     venue_verified=excluded.venue_verified,
                     registration_source=excluded.registration_source,
                     source_url=excluded.source_url,
                     source_snapshot_at=excluded.source_snapshot_at,
                     updated_at=unixepoch(),is_active=1"#,
                params![instrument_id, listing.product_id, listing.category, listing.base,
                    listing.venue_symbol, underlying, storage, listing.quote, "venue_active",
                    1_i64, effective_source,
                    listing.source_url, snapshot, metadata]
            ).map_err(|e| e.to_string())?;
        }
        // Rows not seen in this complete refresh are delisted, even when the
        // previous refresh happened on the same day. Other sources stay pending.
        let deactivated = conn.execute(
            "UPDATE instruments SET listing_status='venue_delisted',venue_verified=0,updated_at=unixepoch()
              WHERE exchange_id='pintu' AND is_active=1
                AND listing_status='reconcile_pending'",
            []).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO instrument_registry_meta(key,value)
            VALUES('pintu_refresh_error','') ON CONFLICT(key) DO UPDATE SET value=''",
            [],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO instrument_registry_meta(key,value)
            VALUES('pintu_refresh_attempted_at',datetime('now'))
            ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [],
        )
        .map_err(|e| e.to_string())?;
        let mut counts = std::collections::BTreeMap::<(&str, &str), usize>::new();
        for listing in listings {
            if listing.product_id == "pintu_pro_futures"
                && crate::pintu_listing::is_announced_delisted_pintu_future(&listing.venue_symbol)
            {
                continue;
            }
            *counts
                .entry((listing.product_id, listing.source_url))
                .or_default() += 1;
        }
        for ((product_id, source_url), count) in counts {
            conn.execute(
                "INSERT INTO listing_sync_runs(exchange_id,product_id,source_url,attempted_at,
                    completed_at,snapshot_at,status,records_received,error)
                 VALUES('pintu',?1,?2,datetime('now'),datetime('now'),?3,'success',?4,'')",
                params![product_id, source_url, snapshot, count as i64],
            )
            .map_err(|e| e.to_string())?;
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
            Err(error) => SyncResult {
                exchange_id,
                inserted: 0,
                deactivated: 0,
                error: Some(error.to_string()),
            },
        },
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            SyncResult {
                exchange_id,
                inserted: 0,
                deactivated: 0,
                error: Some(error),
            }
        }
    }
}
