use super::*;

pub(crate) fn sync_pintu(conn: &Connection) -> SyncResult {
    let exchange_id = "pintu".to_string();
    let listings = match crate::pintu_listing::fetch_all() {
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
                deactivated: 0,
                error: Some(format!(
                    "Pintu listing refresh failed; previous listing retained: {error}"
                )),
            };
        }
    };
    reconcile_pintu(conn, &listings)
}

pub(crate) fn reconcile_pintu(
    conn: &Connection,
    listings: &[crate::pintu_listing::Listing],
) -> SyncResult {
    let exchange_id = "pintu".to_string();
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
            "UPDATE instruments SET listing_status='reconcile_pending'
              WHERE exchange_id='pintu' AND is_active=1
                AND registration_source LIKE '%app_listing%'
                AND listing_status IN ('venue_active','classification_pending')",
            [],
        )
        .map_err(|e| e.to_string())?;
        let mut inserted = 0;
        for listing in listings {
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
                          'binance_then_yfinance_research_proxy',?14,unixepoch(),unixepoch(),1)
                   ON CONFLICT(instrument_id) DO UPDATE SET
                     venue_symbol=excluded.venue_symbol,
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
