use super::*;

#[derive(Clone, Copy)]
pub(super) struct ChartIndicatorSettings {
    nadaraya_watson_envelope: NadarayaWatsonEnvelopeConfig,
}

#[derive(Serialize)]
pub(super) struct ChartIndicatorConfig {
    nadaraya_watson_envelope: quant_core::NadarayaWatsonEnvelopeMetadata,
}

impl Default for ChartIndicatorSettings {
    fn default() -> Self {
        Self {
            nadaraya_watson_envelope: NadarayaWatsonEnvelopeConfig::default(),
        }
    }
}

impl ChartIndicatorSettings {
    pub(super) fn attach(&self, candles: &mut [Candle]) {
        attach_nadaraya_watson_envelope(candles, &self.nadaraya_watson_envelope);
    }

    pub(super) fn metadata(&self) -> ChartIndicatorConfig {
        ChartIndicatorConfig {
            nadaraya_watson_envelope: self.nadaraya_watson_envelope.metadata(),
        }
    }
}

pub(super) fn finite_or_none(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

pub(super) fn new_candle(
    time: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
) -> Candle {
    Candle {
        time,
        open,
        high,
        low,
        close,
        volume,
        dss: None,
        trigger: None,
        gradient: None,
        accel: None,
        indicators: None,
    }
}

pub(super) fn candle_from_row(row: &rusqlite::Row<'_>) -> Result<Candle, rusqlite::Error> {
    Ok(new_candle(
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

pub(super) fn candle_source_for_exchange(
    exchange: Option<&str>,
) -> (Option<String>, Option<String>) {
    let Some(exchange) = exchange.map(str::trim).filter(|value| !value.is_empty()) else {
        return (None, None);
    };
    let normalized = normalize_exchange_id(exchange);
    let native = match normalized.as_str() {
        "tokocrypto" => "tokocrypto_native".to_string(),
        "pintu" | "pintu_futures" => "pintu_futures_native".to_string(),
        _ => format!("{normalized}_native"),
    };
    (
        Some(native),
        Some(format!("{normalized}_yfinance_fallback")),
    )
}

pub(super) fn normalize_exchange_id(exchange: &str) -> String {
    exchange.trim().to_ascii_lowercase()
}

pub(super) fn is_research_proxy_source(source: &str) -> bool {
    source == "yfinance"
        || source == "yfinance_fallback"
        || source.ends_with("_yfinance_fallback")
        || source.contains("_binance_")
        || source.ends_with("_proxy")
        || source.contains("research_proxy")
}

pub(super) fn is_native_candle_source_for_exchange(source: &str, exchange: Option<&str>) -> bool {
    let (native, _) = candle_source_for_exchange(exchange);
    native.as_deref() == Some(source)
}

pub(super) fn candle_source_label(source: &str) -> String {
    let label = match source {
        "tokocrypto_native" => "Tokocrypto native candles".to_string(),
        "stockbit_native" => "Stockbit native candles".to_string(),
        "pluang_native" => "Pluang native candles".to_string(),
        "pintu_futures_native" => "Pintu native candles".to_string(),
        "yfinance" | "yfinance_fallback" => "Yahoo Finance research proxy".to_string(),
        value if value.ends_with("_binance_spot_proxy") => {
            "Binance Spot research proxy (USDT quote)".to_string()
        }
        value if value.ends_with("_binance_usdm_futures_proxy") => {
            "Binance USDⓈ-M Futures research proxy (USDT quote)".to_string()
        }
        value if value.ends_with("_coinbase_spot_proxy") => {
            "Coinbase Spot research proxy (USD quote)".to_string()
        }
        value if value.ends_with("_bybit_spot_proxy") => {
            "Bybit Spot research proxy (USDT quote)".to_string()
        }
        value if value.ends_with("_bybit_linear_proxy") => {
            "Bybit Linear research proxy (USDT contract)".to_string()
        }
        value if value.ends_with("_okx_spot_proxy") => {
            "OKX Spot research proxy (USDT quote)".to_string()
        }
        value if value.ends_with("_yfinance_fallback") => {
            "Yahoo Finance research proxy".to_string()
        }
        value => value.replace('_', " "),
    };
    if research_proxy::shared::is_shared_source(source) {
        format!("{label} · shared reference")
    } else {
        label
    }
}

pub(super) fn normalize_registered_ticker(symbol: &str) -> String {
    let mut value = symbol.trim().to_ascii_uppercase();
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
        if let Some(clean) = value.strip_suffix(suffix) {
            value = clean.to_string();
            break;
        }
    }
    value
}

pub(super) fn resolve_candle_storage_symbol(
    connection: &Connection,
    exchange: &str,
    request_symbol: &str,
    asset_class: Option<&str>,
) -> Result<String, rusqlite::Error> {
    let exchange_key = exchange.trim().to_ascii_lowercase();
    let exchange = exchange_key.as_str();
    let requested = request_symbol.trim().to_ascii_uppercase();
    let clean = normalize_registered_ticker(&requested);
    let class = asset_class.unwrap_or("").trim().to_ascii_lowercase();
    let resolved = connection
        .query_row(
            "SELECT storage_symbol FROM instruments
              WHERE exchange_id=?1 AND is_active=1 AND (
                    upper(storage_symbol)=?2 OR upper(venue_symbol)=?2 OR
                    upper(symbol)=?3 OR upper(underlying_symbol)=?3 OR
                    upper(replace(venue_symbol, '_', ''))=?3
              )
              ORDER BY CASE
                  WHEN (?4 LIKE '%tokenized%' OR ?4 LIKE '%token%')
                       AND asset_category_id='tokenized_stock' THEN 0
                  WHEN (?4 LIKE '%spot%' OR ?4='tokocrypto')
                       AND asset_category_id='crypto_spot' THEN 0
                  WHEN (?4 LIKE '%future%' OR ?4 LIKE '%perpetual%')
                       AND asset_category_id IN ('crypto_futures','crypto_perpetual') THEN 0
                  WHEN (?4='idx' OR ?4 LIKE '%stock%')
                       AND asset_category_id='idx_stock' THEN 0
                  WHEN upper(storage_symbol)=?2 THEN 1
                  ELSE 2 END,
                       upper(storage_symbol) ASC
              LIMIT 1",
            params![exchange, requested, clean, class],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(resolved.unwrap_or(requested))
}

pub(super) fn resolve_registered_candle_storage_symbol(
    connection: &Connection,
    exchange: &str,
    product_id: &str,
    request_storage_symbol: &str,
) -> Result<Option<String>, rusqlite::Error> {
    let exchange = exchange.trim().to_ascii_lowercase();
    let product_id = product_id.trim();
    let request_storage_symbol = request_storage_symbol.trim();
    let registered_symbol = connection
        .query_row(
            "SELECT storage_symbol FROM instruments
              WHERE exchange_id=?1 AND is_active=1 AND venue_verified=1
                AND upper(storage_symbol)=upper(?2)
                AND (?3='' OR product_id=?3)
                AND listing_status IN ('venue_active','public_listed_snapshot')
              LIMIT 1",
            params![exchange, request_storage_symbol, product_id],
            |row| row.get(0),
        )
        .optional()?;
    if registered_symbol.is_some() {
        return Ok(registered_symbol);
    }

    // Signal rows may point at a verified peer's shared candle database. Only
    // allow this when the requested asset resolves back to this app's active
    // registration and both registrations agree on category and underlying.
    let underlying = normalize_registered_ticker(request_storage_symbol);
    if underlying.is_empty() {
        return Ok(None);
    }
    connection
        .query_row(
            "SELECT peer.storage_symbol
               FROM instruments AS target
               JOIN instruments AS peer
                 ON peer.asset_category_id=target.asset_category_id
                AND upper(trim(peer.underlying_symbol))=upper(trim(target.underlying_symbol))
              WHERE target.exchange_id=?1 AND target.is_active=1 AND target.venue_verified=1
                AND target.listing_status IN ('venue_active','public_listed_snapshot')
                AND (?2='' OR target.product_id=?2)
                AND upper(trim(target.underlying_symbol))=?3
                AND target.asset_category_id IN ('crypto_spot','crypto_perpetual','crypto_futures')
                AND peer.exchange_id<>target.exchange_id
                AND peer.is_active=1 AND peer.venue_verified=1
                AND peer.listing_status IN ('venue_active','public_listed_snapshot')
                AND upper(peer.storage_symbol)=upper(?4)
              ORDER BY peer.exchange_id,peer.product_id
              LIMIT 1",
            params![exchange, product_id, underlying, request_storage_symbol],
            |row| row.get(0),
        )
        .optional()
}

pub(super) fn resolve_candle_storage_symbol_any_exchange(
    connection: &Connection,
    request_symbol: &str,
    asset_class: Option<&str>,
) -> Result<String, rusqlite::Error> {
    let requested = request_symbol.trim().to_ascii_uppercase();
    let clean = normalize_registered_ticker(&requested);
    let class = asset_class.unwrap_or("").trim().to_ascii_lowercase();
    let mut statement = connection.prepare(
        "SELECT storage_symbol FROM app.instruments
         WHERE is_active=1 AND (
             upper(storage_symbol)=?1 OR upper(venue_symbol)=?1 OR
             upper(symbol)=?2 OR upper(underlying_symbol)=?2 OR
             upper(replace(venue_symbol,'_',''))=?2
         )
         GROUP BY storage_symbol
         ORDER BY MIN(CASE
             WHEN (?3 LIKE '%tokenized%' OR ?3 LIKE '%token%')
                  AND asset_category_id='tokenized_stock' THEN 0
             WHEN (?3 LIKE '%spot%' OR ?3='tokocrypto')
                  AND asset_category_id='crypto_spot' THEN 0
             WHEN (?3 LIKE '%future%' OR ?3 LIKE '%perpetual%')
                  AND asset_category_id IN ('crypto_futures','crypto_perpetual') THEN 0
             WHEN (?3='idx' OR ?3 LIKE '%stock%')
                  AND asset_category_id='idx_stock' THEN 0
             ELSE 1 END),
             upper(storage_symbol)
         LIMIT 2",
    )?;
    let matches = statement
        .query_map(params![requested, clean, class], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    match matches.as_slice() {
        [storage_symbol] => Ok(storage_symbol.clone()),
        [] => Ok(requested),
        _ => Err(rusqlite::Error::InvalidParameterName(
            "symbol matches multiple registered instruments; provide exchange".into(),
        )),
    }
}

pub(super) fn selected_candle_source(
    connection: &Connection,
    symbol: &str,
    exchange: Option<&str>,
) -> Result<Option<String>, rusqlite::Error> {
    if let Some((app, product)) = exchange.and_then(candle_product_scope) {
        return selected_candle_source_for_product(connection, symbol, app, product, "15m");
    }
    let (native, proxy) = candle_source_for_exchange(exchange);
    let exchange_scope = exchange.map(normalize_exchange_id);
    connection
        .query_row(
            "SELECT candidate.source FROM series AS candidate WHERE candidate.symbol=?1
               AND (EXISTS (SELECT 1 FROM bars
                             WHERE symbol=candidate.symbol AND source=candidate.source)
                    OR candidate.last_error IS NULL OR TRIM(candidate.last_error)='')
               AND (?4 IS NULL OR candidate.source IN (?2, ?3, 'yfinance_fallback')
                    OR candidate.source LIKE (?4 || '_binance_%')
                    OR (candidate.exchange_id=?4 AND
                        (candidate.source LIKE '%_proxy'
                         OR candidate.source LIKE '%_yfinance_fallback')))
             ORDER BY CASE
                 WHEN ?4 IS NOT NULL AND candidate.exchange_id=?4
                      AND (candidate.source LIKE '%_proxy'
                           OR candidate.source LIKE '%_yfinance_fallback')
                      AND EXISTS (
                     SELECT 1 FROM series AS native
                      WHERE native.symbol=candidate.symbol AND native.source=?2
                        AND ((SELECT MAX(unixepoch(last_success)) FROM series
                               WHERE symbol=candidate.symbol AND source=?2) IS NULL OR
                             (SELECT MAX(unixepoch(last_success)) FROM series
                               WHERE symbol=candidate.symbol AND source=?2) <
                             unixepoch('now') - CASE WHEN candidate.symbol LIKE '%.JK' THEN 259200 ELSE 21600 END)
                        AND COALESCE(candidate.last_success, '') > COALESCE((
                            SELECT MAX(last_success) FROM series
                             WHERE symbol=candidate.symbol AND source=?2
                        ), '')
                ) THEN 0
                 WHEN ?2 IS NOT NULL AND candidate.source=?2 THEN 1
                 WHEN candidate.data_provider='binance' THEN 2
                 WHEN candidate.data_provider='coinbase' THEN 3
                 WHEN candidate.data_provider='bybit' THEN 4
                 WHEN candidate.data_provider='okx' THEN 5
                 WHEN candidate.data_provider='yfinance' THEN 6
                 WHEN ?4 IS NULL AND candidate.source LIKE '%_native' THEN 1
                 ELSE 7 END,
             COALESCE(candidate.last_success, '') DESC, candidate.interval ASC
             LIMIT 1",
            params![symbol, native, proxy, exchange_scope],
            |row| row.get::<_, String>(0),
        )
        .optional()
}

pub(super) fn source_has_timeframe_history(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    source: &str,
) -> Result<bool, rusqlite::Error> {
    let mut intervals = vec![timeframe];
    intervals.extend_from_slice(lower_timeframe_candidates(timeframe));
    for interval in intervals {
        let exists = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM bars WHERE symbol=?1 AND interval=?2 AND source=?3)",
            params![symbol, interval, source],
            |row| row.get::<_, bool>(0),
        )?;
        if exists {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Keep the selected app when its source can satisfy this timeframe. If that
/// source only has (for example) daily bars, use the best shared provider
/// source that actually has this interval for the same registered ticker.
pub(super) fn selected_candle_source_for_timeframe(
    connection: &Connection,
    symbol: &str,
    exchange: Option<&str>,
    timeframe: &str,
) -> Result<Option<String>, rusqlite::Error> {
    if let Some((app, product)) = exchange.and_then(candle_product_scope) {
        return selected_candle_source_for_product(connection, symbol, app, product, timeframe);
    }
    if let Some(app) = exchange {
        // An app-only request may share references only when its registered
        // ticker has one unambiguous category. Product requests never use a
        // global fallback that could substitute Spot for a Perpetual.
        let product: Option<String> = connection
            .query_row(
                "SELECT MIN(product_id) FROM app.instruments WHERE exchange_id=?1
                AND storage_symbol=?2 AND is_active=1 HAVING COUNT(DISTINCT asset_category_id)=1",
                params![app, symbol],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if let Some(product) = product {
            return selected_candle_source_for_product(
                connection, symbol, app, &product, timeframe,
            );
        }
        let app_source = selected_candle_source(connection, symbol, exchange)?;
        return Ok(match app_source {
            Some(source)
                if source_has_timeframe_history(connection, symbol, timeframe, &source)? =>
            {
                Some(source)
            }
            _ => None,
        });
    }
    let app_source = selected_candle_source(connection, symbol, exchange)?;
    if let Some(source) = app_source.as_deref() {
        if source_has_timeframe_history(connection, symbol, timeframe, source)? {
            return Ok(app_source);
        }
    }

    let shared_source = selected_candle_source(connection, symbol, None)?;
    if let Some(source) = shared_source.as_deref() {
        if source_has_timeframe_history(connection, symbol, timeframe, source)? {
            return Ok(shared_source);
        }
    }
    Ok(app_source.or(shared_source))
}

pub(super) fn candle_product_scope(scope: &str) -> Option<(&'static str, &'static str)> {
    match scope {
        "stockbit_regular" => Some(("stockbit", "stockbit_regular")),
        "pintu_retail_spot" => Some(("pintu", "pintu_retail_spot")),
        "pintu_pro_spot" => Some(("pintu", "pintu_pro_spot")),
        "pintu_futures" | "pintu_pro_futures" => Some(("pintu", "pintu_pro_futures")),
        "pluang_spot" => Some(("pluang", "pluang_spot")),
        "pluang_perpetual" => Some(("pluang", "pluang_perpetual")),
        "tokocrypto_spot" => Some(("tokocrypto", "tokocrypto_spot")),
        "tokocrypto_tokenized_stock" => Some(("tokocrypto", "tokocrypto_tokenized_stock")),
        _ => None,
    }
}

/// Select history for one registered product, not merely an app or ticker.
/// Registered crypto products may reuse canonical research references in their
/// exact category. Native series retain their app/product ownership.
pub(super) fn selected_candle_source_for_product(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product_id: &str,
    timeframe: &str,
) -> Result<Option<String>, rusqlite::Error> {
    Ok(selected_candle_source_metadata_for_product(
        connection, symbol, exchange, product_id, timeframe,
    )?
    .map(|(source, _)| source))
}

pub(super) fn selected_candle_source_metadata_for_product(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product_id: &str,
    timeframe: &str,
) -> Result<Option<(String, String)>, rusqlite::Error> {
    selected_candle_source_metadata_for_product_with_preferred_source(
        connection, symbol, exchange, product_id, timeframe, None,
    )
}

pub(super) fn selected_candle_source_metadata_for_product_with_preferred_source(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product_id: &str,
    timeframe: &str,
    preferred_source: Option<&str>,
) -> Result<Option<(String, String)>, rusqlite::Error> {
    let category = research_proxy::shared::category_for_product(exchange, product_id).unwrap_or("");
    let shared_identities = research_proxy::shared::source_identities_json(category);
    let shared_exchange = research_proxy::shared::SOURCE_EXCHANGE_ID;
    if let Some(preferred_source) = preferred_source {
        let lower_intervals = lower_timeframe_candidates(timeframe);
        let first_lower_interval = lower_intervals.first().copied().unwrap_or("");
        let second_lower_interval = lower_intervals.get(1).copied().unwrap_or("");
        let metadata = connection
            .query_row(
                "SELECT candidate.source,candidate.data_provider
                   FROM series AS candidate
                  WHERE candidate.symbol=?1 AND (
                    (candidate.exchange_id=?2 AND candidate.product_id=?3)
                    OR (candidate.exchange_id=?8 AND candidate.product_id=''
                        AND candidate.instrument_id IS NULL AND candidate.instrument_kind=?9
                        AND EXISTS(SELECT 1 FROM json_each(?10) AS identity
                            WHERE json_extract(identity.value,'$.source')=candidate.source
                              AND json_extract(identity.value,'$.provider')=candidate.data_provider
                              AND json_extract(identity.value,'$.price_basis')=candidate.price_basis)))
                    AND candidate.interval IN (?4,?5,?6)
                    AND candidate.source=?7
                    AND EXISTS (
                        SELECT 1 FROM bars
                         WHERE symbol=candidate.symbol AND interval=candidate.interval
                           AND source=candidate.source
                    )
                  ORDER BY CASE candidate.interval
                    WHEN ?4 THEN 0 WHEN ?5 THEN 1 WHEN ?6 THEN 2 ELSE 3 END
                  LIMIT 1",
                params![
                    symbol,
                    exchange,
                    product_id,
                    timeframe,
                    first_lower_interval,
                    second_lower_interval,
                    preferred_source,
                    shared_exchange,
                    category,
                    shared_identities,
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?
                            .unwrap_or_else(|| "unknown".to_string()),
                    ))
                },
            )
            .optional()?;
        if metadata.is_some() {
            return Ok(metadata);
        }
    }

    for interval in
        std::iter::once(timeframe).chain(lower_timeframe_candidates(timeframe).iter().copied())
    {
        let metadata = candle_source_metadata_at_interval(
            connection, symbol, exchange, product_id, timeframe, interval,
        )?;
        if metadata.is_some() {
            return Ok(metadata);
        }
    }
    Ok(None)
}

/// Direct stored coverage uses the same source/ownership policy without rollups.
pub(super) fn selected_candle_source_metadata_for_product_at_interval(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product_id: &str,
    interval: &str,
) -> Result<Option<(String, String)>, rusqlite::Error> {
    candle_source_metadata_at_interval(connection, symbol, exchange, product_id, interval, interval)
}

fn candle_source_metadata_at_interval(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product_id: &str,
    timeframe: &str,
    interval: &str,
) -> Result<Option<(String, String)>, rusqlite::Error> {
    let category = research_proxy::shared::category_for_product(exchange, product_id).unwrap_or("");
    let shared_identities = research_proxy::shared::source_identities_json(category);
    let shared_exchange = research_proxy::shared::SOURCE_EXCHANGE_ID;
    let now_ns = (now_secs() as i64).saturating_mul(NANOS_PER_SECOND);
    let oldest_fresh_ns = now_ns.saturating_sub(
        freshness_threshold_seconds(symbol, timeframe).saturating_mul(NANOS_PER_SECOND),
    );
    let latest_completed_ns =
        now_ns.saturating_sub(timeframe_duration_ns(interval).unwrap_or_default());
    connection
            .query_row(
                "SELECT candidate.source,candidate.data_provider
                   FROM series AS candidate
                  WHERE candidate.symbol=?1 AND (
                    (candidate.exchange_id=?2 AND candidate.product_id=?3)
                    OR (candidate.exchange_id=?7 AND candidate.product_id=''
                        AND candidate.instrument_id IS NULL AND candidate.instrument_kind=?8
                        AND EXISTS(SELECT 1 FROM json_each(?9) AS identity
                            WHERE json_extract(identity.value,'$.source')=candidate.source
                              AND json_extract(identity.value,'$.provider')=candidate.data_provider
                              AND json_extract(identity.value,'$.price_basis')=candidate.price_basis))) AND candidate.interval=?4
                    AND EXISTS (
                        SELECT 1 FROM bars
                         WHERE symbol=candidate.symbol AND interval=candidate.interval
                           AND source=candidate.source
                    )
                  ORDER BY CASE WHEN EXISTS (
                      SELECT 1 FROM bars AS fresh
                       WHERE fresh.symbol=candidate.symbol AND fresh.interval=candidate.interval
                         AND fresh.source=candidate.source AND fresh.close>0
                         AND fresh.ts BETWEEN ?5 AND ?6
                  ) THEN 0 ELSE 1 END,
                  CASE
                      WHEN candidate.data_provider=?2 OR candidate.source=?2 || '_native' THEN 0
                      WHEN candidate.data_provider='binance' THEN 1
                      WHEN candidate.data_provider='coinbase' THEN 2
                      WHEN candidate.data_provider='bybit' THEN 3
                      WHEN candidate.data_provider='okx' THEN 4
                      WHEN candidate.data_provider='yfinance' THEN 5
                      ELSE 6 END,
                           COALESCE(candidate.last_success,'') DESC,
                           candidate.source ASC
                  LIMIT 1",
                params![symbol, exchange, product_id, interval, oldest_fresh_ns, latest_completed_ns,
                    shared_exchange, category, shared_identities],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?
                            .unwrap_or_else(|| "unknown".to_string()),
                    ))
                },
            )
            .optional()
}

pub(super) fn load_candles(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    limit: i64,
) -> Result<Vec<Candle>, rusqlite::Error> {
    let Some(source) = selected_candle_source(connection, symbol, None)? else {
        return Ok(Vec::new());
    };
    load_candles_from_source(connection, symbol, timeframe, limit, &source)
}

pub(super) fn load_candles_from_source(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    limit: i64,
    source: &str,
) -> Result<Vec<Candle>, rusqlite::Error> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let daily = timeframe == "1d";
    let mut statement = connection.prepare(if daily {
        // Keep the latest stored version of each exchange-local trading date.
        // The indexed timestamp range in NOT EXISTS lets LIMIT count sessions,
        // rather than raw rows, without loading duplicate history into Rust.
        "SELECT b.ts,b.open,b.high,b.low,b.close,b.volume
         FROM bars b
         WHERE b.symbol=?1 AND b.interval=?2 AND b.source=?3
           AND NOT EXISTS (
               SELECT 1 FROM bars newer
               WHERE newer.symbol=b.symbol AND newer.interval=b.interval
                 AND newer.source=b.source
                 AND newer.ts>b.ts
                 AND newer.ts<((b.ts+?5)/?6+1)*?6-?5
           )
         ORDER BY b.ts DESC LIMIT ?4"
    } else {
        "SELECT ts,open,high,low,close,volume
         FROM bars
         WHERE symbol=?1 AND interval=?2 AND source=?3
         ORDER BY ts DESC LIMIT ?4"
    })?;
    let rows = if daily {
        let offset = timezone_offset_ns_from_source(connection, symbol, timeframe, source)?;
        statement.query_map(
            params![symbol, timeframe, source, limit, offset, NANOS_PER_DAY],
            candle_from_row,
        )?
    } else {
        statement.query_map(params![symbol, timeframe, source, limit], candle_from_row)?
    };
    let mut candles: Vec<Candle> = rows.collect::<Result<Vec<_>, _>>()?;
    candles.reverse();
    Ok(candles)
}

pub(super) fn load_candles_from_source_between(
    connection: &Connection,
    symbol: &str,
    timeframe: &str,
    limit: i64,
    source: &str,
    from_time: i64,
    to_time_exclusive: i64,
) -> Result<Vec<Candle>, rusqlite::Error> {
    if limit <= 0 || from_time >= to_time_exclusive {
        return Ok(Vec::new());
    }
    let daily = timeframe == "1d";
    let mut statement = connection.prepare(if daily {
        "SELECT b.ts,b.open,b.high,b.low,b.close,b.volume
         FROM bars b
         WHERE b.symbol=?1 AND b.interval=?2 AND b.source=?3
           AND b.ts>=?4 AND b.ts<?5
           AND NOT EXISTS (
               SELECT 1 FROM bars newer
               WHERE newer.symbol=b.symbol AND newer.interval=b.interval
                 AND newer.source=b.source AND newer.ts>b.ts AND newer.ts<?5
                 AND newer.ts<((b.ts+?6)/?7+1)*?7-?6
           )
         ORDER BY b.ts DESC LIMIT ?8"
    } else {
        "SELECT ts,open,high,low,close,volume FROM bars
         WHERE symbol=?1 AND interval=?2 AND source=?3 AND ts>=?4 AND ts<?5
         ORDER BY ts DESC LIMIT ?6"
    })?;
    let rows = if daily {
        let offset = timezone_offset_ns_from_source(connection, symbol, timeframe, source)?;
        statement.query_map(
            params![
                symbol,
                timeframe,
                source,
                from_time,
                to_time_exclusive,
                offset,
                NANOS_PER_DAY,
                limit
            ],
            candle_from_row,
        )?
    } else {
        statement.query_map(
            params![
                symbol,
                timeframe,
                source,
                from_time,
                to_time_exclusive,
                limit
            ],
            candle_from_row,
        )?
    };
    let mut candles = rows.collect::<Result<Vec<_>, _>>()?;
    candles.reverse();
    Ok(candles)
}

pub(super) fn attach_dss(candles: &mut [Candle], config: &DssConfig) {
    let highs: Vec<f64> = candles.iter().map(|candle| candle.high).collect();
    let lows: Vec<f64> = candles.iter().map(|candle| candle.low).collect();
    let closes: Vec<f64> = candles.iter().map(|candle| candle.close).collect();
    let result = calculate_dss(&highs, &lows, &closes, config);
    for (index, candle) in candles.iter_mut().enumerate() {
        candle.dss = result.dss.get(index).copied().and_then(finite_or_none);
        candle.trigger = result.trigger.get(index).copied().and_then(finite_or_none);
        candle.gradient = result.gradient.get(index).copied().and_then(finite_or_none);
        candle.accel = if index > 0 {
            result
                .gradient
                .get(index)
                .zip(result.gradient.get(index - 1))
                .map(|(now, previous)| now - previous)
                .and_then(finite_or_none)
        } else {
            None
        };
    }
}

pub(super) fn attach_nadaraya_watson_envelope(
    candles: &mut [Candle],
    config: &NadarayaWatsonEnvelopeConfig,
) {
    let closes: Vec<f64> = candles.iter().map(|candle| candle.close).collect();
    let result = calculate_nadaraya_watson_envelope(&closes, config);
    for (index, candle) in candles.iter_mut().enumerate() {
        let repainting = nadaraya_watson_mode_value(&result.repainting, index);
        let endpoint = nadaraya_watson_mode_value(&result.endpoint, index);
        let has_values = repainting.upper.is_some()
            || repainting.lower.is_some()
            || repainting.crossed_above_upper.is_some()
            || repainting.crossed_below_lower.is_some()
            || endpoint.upper.is_some()
            || endpoint.lower.is_some()
            || endpoint.crossed_above_upper.is_some()
            || endpoint.crossed_below_lower.is_some();
        if has_values {
            candle
                .indicators
                .get_or_insert_with(ChartIndicatorValues::default)
                .nadaraya_watson_envelope = Some(NadarayaWatsonEnvelopeCandleValue {
                repainting,
                endpoint,
            });
        }
    }
}

fn nadaraya_watson_mode_value(
    mode: &NadarayaWatsonEnvelopeMode,
    index: usize,
) -> NadarayaWatsonEnvelopeModeCandleValue {
    NadarayaWatsonEnvelopeModeCandleValue {
        upper: mode.upper.get(index).copied().and_then(finite_or_none),
        lower: mode.lower.get(index).copied().and_then(finite_or_none),
        crossed_above_upper: mode
            .crossed_above_upper
            .get(index)
            .copied()
            .filter(|crossed| *crossed),
        crossed_below_lower: mode
            .crossed_below_lower
            .get(index)
            .copied()
            .filter(|crossed| *crossed),
    }
}
