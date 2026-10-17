use super::*;

pub fn load_markov_payload(
    connection: &Connection,
    market_database: &str,
    timeframe: &str,
    limit: i64,
    selected_symbol: Option<&str>,
    selected_exchange: Option<&str>,
    selected_category: Option<&str>,
) -> Result<Value, rusqlite::Error> {
    const MAX_MARKOV_SYMBOLS: i64 = 128;
    let mut statement = connection.prepare(
        "SELECT DISTINCT storage_symbol, exchange_id, asset_category_id FROM app.instruments
          WHERE is_active=1 AND listing_status <> 'venue_delisted'
            AND (?1 IS NULL OR storage_symbol = ?1)
            AND (?2 IS NULL OR exchange_id = ?2)
            AND (?3 IS NULL OR asset_category_id = ?3)
         ORDER BY upper(storage_symbol), exchange_id, asset_category_id LIMIT ?4",
    )?;
    let symbols = statement
        .query_map(
            rusqlite::params![
                selected_symbol,
                selected_exchange,
                selected_category,
                MAX_MARKOV_SYMBOLS
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut assets = Vec::with_capacity(symbols.len());
    for (symbol, exchange_id, asset_category_id) in symbols {
        let Ok(Some(market_connection)) =
            open_market_history_for_symbol(market_database, &symbol, false)
        else {
            continue;
        };
        let Ok(candles) = load_candles(&market_connection, &symbol, timeframe, limit) else {
            continue;
        };
        let bars = candles
            .into_iter()
            .map(|candle| StrategyBar {
                time: candle.time,
                open: candle.open,
                high: candle.high,
                low: candle.low,
                close: candle.close,
                volume: candle.volume,
            })
            .collect::<Vec<_>>();
        assets.push(quant_core::MarkovAsset {
            storage_symbol: symbol,
            exchange_id,
            asset_category_id,
            bars,
        });
    }
    Ok(quant_core::build_markov_payload_for_timeframe(
        &assets,
        &quant_core::MarkovRegimeConfig::default(),
        timeframe,
    ))
}
