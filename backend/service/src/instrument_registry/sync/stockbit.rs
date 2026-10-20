use super::*;

pub(crate) fn sync_stockbit(conn: &Connection) -> SyncResult {
    let exchange_id = "stockbit".to_string();
    let listings = match crate::stockbit_listing::fetch_all() {
        Ok(listings) => listings,
        Err(error) => {
            record_listing_failure(
                conn,
                "stockbit",
                STOCKBIT_PRODUCT_ID,
                crate::stockbit_listing::LISTED_COMPANY_DIRECTORY_URL,
                &error,
            );
            return SyncResult {
                exchange_id,
                inserted: 0,
                deactivated: 0,
                error: Some(format!(
                    "Stockbit app listing refresh failed; previous listing retained: {error}"
                )),
            };
        }
    };
    reconcile_stockbit(conn, &listings)
}

pub(crate) fn reconcile_stockbit(
    conn: &Connection,
    listings: &[crate::stockbit_listing::Listing],
) -> SyncResult {
    let exchange_id = "stockbit".to_string();
    if let Err(validation_error) = crate::stockbit_listing::validate_complete(listings) {
        let error = format!(
            "Stockbit IDX directory refresh is incomplete; previous listing retained: {validation_error}"
        );
        record_listing_failure(
            conn,
            "stockbit",
            STOCKBIT_PRODUCT_ID,
            crate::stockbit_listing::LISTED_COMPANY_DIRECTORY_URL,
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
            "UPDATE instruments SET listing_status='reconcile_pending'
              WHERE exchange_id='stockbit' AND is_active=1
                AND product_id=?1 AND registration_source IN (?2,?3)
                AND listing_status IN ('venue_active','public_listed_snapshot','reconcile_pending')",
            params![STOCKBIT_PRODUCT_ID, STOCKBIT_LISTING_SOURCE, LEGACY_STOCKBIT_LISTING_SOURCE],
        )
        .map_err(|error| error.to_string())?;
        let mut inserted = 0usize;
        for listing in listings {
            let symbol = listing.symbol.to_ascii_uppercase();
            if !(3..=6).contains(&symbol.len())
                || !symbol
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
            {
                return Err(format!("Stockbit listing has invalid symbol {symbol}"));
            }
            let instrument_id = format!(
                "stockbit.{STOCKBIT_PRODUCT_ID}.{}",
                symbol.to_ascii_lowercase()
            );
            let existing: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM instruments WHERE instrument_id=?1)",
                    [&instrument_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            let metadata = crate::wire::encode(
                &json!({"listing_source":crate::stockbit_listing::LISTED_COMPANY_DIRECTORY_URL,
                "asset_category_id":"idx_stock","price_source":"research_proxy"}),
            )
            .map_err(|error| error.to_string())?;
            conn.execute(
                r#"INSERT INTO instruments(instrument_id,exchange_id,product_id,asset_category_id,
                    symbol,venue_symbol,underlying_symbol,storage_symbol,quote_currency,
                    listing_status,venue_verified,execution_supported,registration_source,
                    source_url,source_snapshot_at,price_source_policy,contract_metadata_json,
                    registered_at,updated_at,is_active)
                   VALUES(?1,'stockbit',?2,'idx_stock',?3,?3,?3,?4,'IDR','public_listed_snapshot',1,0,
                    ?5,?6,?7,'yfinance_research_proxy',?8,unixepoch(),unixepoch(),1)
                   ON CONFLICT(instrument_id) DO UPDATE SET
                    listing_status='public_listed_snapshot',venue_verified=1,registration_source=excluded.registration_source,
                    source_url=excluded.source_url,source_snapshot_at=excluded.source_snapshot_at,
                    price_source_policy=excluded.price_source_policy,contract_metadata_json=excluded.contract_metadata_json,
                    updated_at=unixepoch(),is_active=1"#,
                params![instrument_id,STOCKBIT_PRODUCT_ID,symbol,format!("{symbol}.JK"),
                    STOCKBIT_LISTING_SOURCE,crate::stockbit_listing::LISTED_COMPANY_DIRECTORY_URL,snapshot,metadata],
            ).map_err(|error| error.to_string())?;
            if !existing {
                inserted += 1;
            }
            conn.execute(
                "UPDATE instruments SET is_active=0,listing_status='superseded_product',updated_at=unixepoch()
                  WHERE exchange_id='stockbit' AND symbol=?1 AND instrument_id<>?2 AND is_active=1",
                params![symbol,instrument_id],
            ).map_err(|error| error.to_string())?;
            conn.execute(
                "UPDATE series SET product_id=?1,instrument_id=?2
                WHERE exchange_id='stockbit' AND symbol=?3",
                params![STOCKBIT_PRODUCT_ID, instrument_id, format!("{symbol}.JK")],
            )
            .map_err(|error| error.to_string())?;
            conn.execute("UPDATE ingestion_runs SET product_id=?1 WHERE exchange_id='stockbit' AND symbol=?2",
                params![STOCKBIT_PRODUCT_ID,format!("{symbol}.JK")]).map_err(|error| error.to_string())?;
        }
        let deactivated = conn
            .execute(
                "UPDATE instruments SET is_active=0,listing_status='registration_unconfirmed',
                venue_verified=0,updated_at=unixepoch()
              WHERE exchange_id='stockbit' AND product_id=?1 AND is_active=1
                AND registration_source IN (?2,?3) AND listing_status='reconcile_pending'",
                params![
                    STOCKBIT_PRODUCT_ID,
                    STOCKBIT_LISTING_SOURCE,
                    LEGACY_STOCKBIT_LISTING_SOURCE
                ],
            )
            .map_err(|error| error.to_string())?;
        clear_listing_failure(conn, "stockbit")?;
        record_listing_success(
            conn,
            "stockbit",
            STOCKBIT_PRODUCT_ID,
            crate::stockbit_listing::LISTED_COMPANY_DIRECTORY_URL,
            &snapshot,
            listings.len(),
        )?;
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
                    "stockbit",
                    STOCKBIT_PRODUCT_ID,
                    crate::stockbit_listing::LISTED_COMPANY_DIRECTORY_URL,
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
                "stockbit",
                STOCKBIT_PRODUCT_ID,
                crate::stockbit_listing::LISTED_COMPANY_DIRECTORY_URL,
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
