use super::schema::sync_normalized_dimensions;
use super::*;

pub(crate) const DEFAULT_PAGE_SIZE_ROWS: usize = 120;
pub(crate) const MAX_PAGE_SIZE_ROWS: usize = 500;

pub(crate) fn list_page(
    conn: &Connection,
    exchange_filter: Option<&str>,
    product_filter: Option<&str>,
    category_filter: Option<&str>,
    symbol_filter: Option<&str>,
    search_filter: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Value, rusqlite::Error> {
    let mut query_values = Vec::new();
    let mut where_clause = String::from(" WHERE is_active=1");
    for (column, value) in [
        ("exchange_id", exchange_filter),
        ("product_id", product_filter),
        ("asset_category_id", category_filter),
        ("symbol", symbol_filter),
    ] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            let normalized = if column == "symbol" {
                value.to_ascii_uppercase()
            } else {
                value.to_ascii_lowercase()
            };
            query_values.push(rusqlite::types::Value::Text(normalized));
            where_clause.push_str(&format!(" AND {column}=?{}", query_values.len()));
        }
    }
    if let Some(search) = search_filter
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let escaped_search = search
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let parameter = query_values.len() + 1;
        query_values.push(rusqlite::types::Value::Text(format!("%{escaped_search}%")));
        where_clause.push_str(&format!(
            " AND (symbol LIKE ?{parameter} ESCAPE '\\' COLLATE NOCASE
                   OR venue_symbol LIKE ?{parameter} ESCAPE '\\' COLLATE NOCASE
                   OR exchange_id LIKE ?{parameter} ESCAPE '\\' COLLATE NOCASE
                   OR product_id LIKE ?{parameter} ESCAPE '\\' COLLATE NOCASE
                   OR asset_category_id LIKE ?{parameter} ESCAPE '\\' COLLATE NOCASE)"
        ));
    }
    let total_active: usize = conn.query_row(
        "SELECT COUNT(*) FROM instruments WHERE is_active=1",
        [],
        |row| row.get(0),
    )?;
    let matching_count = if query_values.is_empty() {
        total_active
    } else {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM instruments{where_clause}"),
            rusqlite::params_from_iter(query_values.iter()),
            |row| row.get(0),
        )?
    };
    let page_size = limit.clamp(1, MAX_PAGE_SIZE_ROWS);
    let page_offset = offset.min(matching_count);
    let query = format!(
        r#"SELECT instrument_id, exchange_id, product_id, asset_category_id, symbol,
                  venue_symbol, underlying_symbol, storage_symbol,
                  quote_currency, listing_status, venue_verified,
                  execution_supported, registration_source, source_url,
                  source_snapshot_at, price_source_policy,
                  contract_metadata_json, registered_at, updated_at
             FROM instruments{where_clause}
            ORDER BY exchange_id, asset_category_id, symbol
            LIMIT {page_size} OFFSET {page_offset}"#
    );
    let mut statement = conn.prepare(&query)?;
    let rows = statement.query_map(rusqlite::params_from_iter(query_values.iter()), |row| {
        let metadata_raw = row.get::<_, String>(16)?;
        let metadata = crate::wire::decode::<Value>(&metadata_raw).unwrap_or(Value::Null);
        Ok(json!({
            "instrument_id": row.get::<_, String>(0)?,
            "exchange_id": row.get::<_, String>(1)?,
            "product_id": row.get::<_, String>(2)?,
            "asset_category_id": row.get::<_, String>(3)?,
            "symbol": row.get::<_, String>(4)?,
            "venue_symbol": row.get::<_, String>(5)?,
            "underlying_symbol": row.get::<_, String>(6)?,
            "storage_symbol": row.get::<_, String>(7)?,
            "quote_currency": row.get::<_, String>(8)?,
            "listing_status": row.get::<_, String>(9)?,
            "venue_verified": row.get::<_, bool>(10)?,
            "research_eligibility": research_eligibility(
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(9)?,
                row.get::<_, bool>(10)?,
            ),
            "execution_supported": row.get::<_, bool>(11)?,
            "registration_source": row.get::<_, String>(12)?,
            "source_url": row.get::<_, Option<String>>(13)?,
            "source_snapshot_at": row.get::<_, Option<String>>(14)?,
            "price_source_policy": row.get::<_, String>(15)?,
            "contract_metadata": metadata,
            "registered_at": row.get::<_, i64>(17)?,
            "updated_at": row.get::<_, i64>(18)?
        }))
    })?;
    let instruments = rows.collect::<Result<Vec<_>, _>>()?;
    let next_offset = page_offset.saturating_add(instruments.len());
    Ok(json!({
        "registered_count": total_active,
        "matching_count": matching_count,
        "offset": page_offset,
        "next_offset": next_offset,
        "limit": page_size,
        "has_more": next_offset < matching_count,
        "instruments": instruments,
    }))
}

pub(crate) fn register_manual(conn: &Connection, body: &Value) -> Result<Value, String> {
    let exchange_id = body
        .get("exchange_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let category_id = body
        .get("asset_category_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let product_id = body
        .get("product_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let input_symbol = body
        .get("symbol")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_uppercase();
    if input_symbol.is_empty() {
        return Err("ticker is required".to_string());
    }

    let allowed_category = match exchange_id.as_str() {
        "stockbit" => category_id == "idx_stock",
        "pintu" => matches!(
            (product_id.as_str(), category_id.as_str()),
            ("pintu_retail_spot" | "pintu_pro_spot", "crypto_spot")
                | ("pintu_pro_futures", "crypto_perpetual")
        ),
        "tokocrypto" => matches!(category_id.as_str(), "crypto_spot" | "tokenized_stock"),
        "pluang" => matches!(
            (product_id.as_str(), category_id.as_str()),
            ("pluang_spot", "crypto_spot") | ("pluang_perpetual", "crypto_perpetual")
        ),
        _ => false,
    };
    if !allowed_category {
        return Err("choose a supported venue and asset category".to_string());
    }

    let normalized = match exchange_id.as_str() {
        "stockbit" => input_symbol.strip_suffix(".JK").unwrap_or(&input_symbol),
        _ if category_id == "crypto_perpetual" => input_symbol
            .strip_suffix("-USDT-PERP")
            .or_else(|| input_symbol.strip_suffix("USDT-PERP"))
            .unwrap_or(&input_symbol),
        _ if category_id == "crypto_spot" => input_symbol
            .strip_suffix("/IDR")
            .or_else(|| input_symbol.strip_suffix("-IDR"))
            .unwrap_or(&input_symbol),
        _ => &input_symbol,
    };
    if normalized.is_empty()
        || normalized.len() > 20
        || !normalized
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Err("ticker must contain only letters and numbers (optional .JK, USDT-PERP, or /IDR suffix accepted)".to_string());
    }

    let (symbol, venue_symbol, underlying, storage_symbol, quote_currency) =
        match (exchange_id.as_str(), category_id.as_str()) {
            ("stockbit", "idx_stock") => (
                normalized.to_string(),
                normalized.to_string(),
                normalized.to_string(),
                format!("{normalized}.JK"),
                "IDR",
            ),
            ("pintu", "crypto_perpetual") => (
                normalized.to_string(),
                format!("{normalized}USDT-PERP"),
                format!("{normalized}USDT"),
                format!("{normalized}-USD"),
                "USDT",
            ),
            ("pluang", "crypto_perpetual") => (
                normalized.to_string(),
                format!("{normalized}USDT-PERP"),
                normalized.to_string(),
                format!("{normalized}-USD"),
                "USDT",
            ),
            ("tokocrypto", "crypto_spot")
            | ("pluang", "crypto_spot")
            | ("pintu", "crypto_spot") => (
                normalized.to_string(),
                format!("{normalized}/IDR"),
                normalized.to_string(),
                format!("{normalized}-USD"),
                "IDR",
            ),
            ("tokocrypto", "tokenized_stock") => (
                normalized.to_string(),
                normalized.to_string(),
                normalized.to_string(),
                normalized.to_string(),
                "USD",
            ),
            _ => return Err("venue and asset category do not match".to_string()),
        };
    let instrument_id = if product_id.is_empty() {
        format!(
            "{exchange_id}.{category_id}.{}",
            symbol.to_ascii_lowercase()
        )
    } else {
        format!("{exchange_id}.{product_id}.{}", symbol.to_ascii_lowercase())
    };
    let metadata = crate::wire::encode(&json!({
        "registered_manually": true,
        "verification_required": true
    }))
    .unwrap_or_default();
    let inserted = conn
        .execute(
            r#"INSERT OR IGNORE INTO instruments (
                   instrument_id, exchange_id, product_id, asset_category_id, symbol,
                   venue_symbol, underlying_symbol, storage_symbol, quote_currency,
                   listing_status, venue_verified, execution_supported,
                   registration_source, source_url, source_snapshot_at,
                   price_source_policy, contract_metadata_json, registered_at,
                   updated_at, is_active
               ) VALUES (
                   ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'manual_unverified',
                   0, 0, 'dashboard_manual_registration', NULL, NULL,
                   'yfinance_research_proxy', ?10, unixepoch(), unixepoch(), 1
               )"#,
            params![
                instrument_id,
                exchange_id,
                product_id,
                category_id,
                symbol,
                venue_symbol,
                underlying,
                storage_symbol,
                quote_currency,
                metadata
            ],
        )
        .map_err(|error| error.to_string())?;
    sync_normalized_dimensions(conn).map_err(|error| error.to_string())?;

    let mut entry = conn
        .query_row(
            r#"SELECT instrument_id, exchange_id, asset_category_id, symbol,
                      venue_symbol, underlying_symbol, storage_symbol,
                      quote_currency, listing_status, venue_verified,
                      execution_supported, registration_source, source_url,
                      source_snapshot_at, price_source_policy,
                      contract_metadata_json, registered_at, updated_at
                 FROM instruments WHERE instrument_id=?1 AND is_active=1"#,
            [&instrument_id],
            |row| {
                let metadata_raw = row.get::<_, String>(15)?;
                let metadata = crate::wire::decode::<Value>(&metadata_raw).unwrap_or(Value::Null);
                Ok(json!({
                    "instrument_id": row.get::<_, String>(0)?,
                    "exchange_id": row.get::<_, String>(1)?,
                    "asset_category_id": row.get::<_, String>(2)?,
                    "symbol": row.get::<_, String>(3)?,
                    "venue_symbol": row.get::<_, String>(4)?,
                    "underlying_symbol": row.get::<_, String>(5)?,
                    "storage_symbol": row.get::<_, String>(6)?,
                    "quote_currency": row.get::<_, String>(7)?,
                    "listing_status": row.get::<_, String>(8)?,
                    "venue_verified": row.get::<_, bool>(9)?,
                    "execution_supported": row.get::<_, bool>(10)?,
                    "registration_source": row.get::<_, String>(11)?,
                    "source_url": row.get::<_, Option<String>>(12)?,
                    "source_snapshot_at": row.get::<_, Option<String>>(13)?,
                    "price_source_policy": row.get::<_, String>(14)?,
                    "contract_metadata": metadata,
                    "registered_at": row.get::<_, i64>(16)?,
                    "updated_at": row.get::<_, i64>(17)?
                }))
            },
        )
        .map_err(|error| error.to_string())?;
    if let Some(object) = entry.as_object_mut() {
        object.insert("created".to_string(), json!(inserted == 1));
        object.insert("product_id".to_string(), json!(product_id));
    }
    Ok(entry)
}
