use super::*;

pub(super) fn sync_interval_for_exchange(
    connection: &Connection,
    storage_symbol: &str,
    kind: &str,
    exchange: &str,
    product_id: &str,
    yahoo_symbol: &str,
    interval: &str,
) -> Result<usize, String> {
    let (duration_seconds, maximum_rows) = match interval {
        "15m" => (FIFTEEN_MINUTE_INTERVAL_SECONDS, MAX_RESEARCH_15M_CANDLES),
        "1h" => (ONE_HOUR_INTERVAL_SECONDS, 200),
        "1d" => (ONE_DAY_INTERVAL_SECONDS, 4_000),
        _ => return Err("unsupported research candle interval".into()),
    };
    let normalized_exchange = match exchange.trim().to_ascii_lowercase().as_str() {
        "idx" | "stockbit_idx" => "stockbit".to_string(),
        "stockbit" | "pintu" | "pluang" | "tokocrypto" => exchange.trim().to_ascii_lowercase(),
        _ => return Err("unsupported exchange for research fallback".into()),
    };

    let category = kind.trim().to_ascii_lowercase();
    if let Some(result) = warmup::sync_stockbit_4h_history_if_required(
        connection,
        storage_symbol,
        &category,
        &normalized_exchange,
        product_id,
        yahoo_symbol,
        interval,
    ) {
        return result;
    }
    let source_prefix = if product_id.is_empty() {
        normalized_exchange.as_str()
    } else if matches!(
        (normalized_exchange.as_str(), product_id),
        ("stockbit", "stockbit_regular")
            | (
                "tokocrypto",
                "tokocrypto_spot" | "tokocrypto_tokenized_stock"
            )
            | (
                "pintu",
                "pintu_retail_spot" | "pintu_pro_spot" | "pintu_pro_futures"
            )
            | ("pluang", "pluang_spot" | "pluang_perpetual")
    ) {
        if product_id == "pintu_pro_futures" {
            "pintu_futures"
        } else {
            product_id
        }
    } else {
        return Err("product does not belong to the selected app".into());
    };
    let family = match category.as_str() {
        "crypto_spot" => Some(BinanceMarket::Spot),
        "crypto_perpetual" | "crypto_futures" => Some(BinanceMarket::UsdmFutures),
        _ => None,
    };

    let mut successful_sources = 0usize;
    let mut rows_received = 0usize;
    let mut errors = Vec::new();
    if let Some(family) = family {
        let source = format!("{source_prefix}_{}", family.source_suffix());
        let binance_result = match yahoo_base_asset(yahoo_symbol) {
            Ok(base) => {
                let binance_pair = format!("{base}USDT");
                recent_proxy_result(
                    connection,
                    storage_symbol,
                    &normalized_exchange,
                    product_id,
                    &source,
                    interval,
                    duration_seconds,
                )
                .unwrap_or_else(|| {
                    sync_with_binance(
                        connection,
                        storage_symbol,
                        &category,
                        &normalized_exchange,
                        &binance_pair,
                        family,
                        &source,
                        interval,
                        duration_seconds,
                        maximum_rows,
                    )
                })
            }
            Err(error) => Err(error),
        };
        match binance_result {
            Ok(rows) => {
                successful_sources += 1;
                rows_received += rows;
            }
            Err(error) => errors.push(format!("Binance: {error}")),
        }

        if successful_sources == 0 {
            for adapter in sources::ADAPTERS {
                if !adapter.supports(&category) {
                    continue;
                }
                let source = format!("{source_prefix}_{}", adapter.source_suffix());
                let provider_result = recent_proxy_result(
                    connection,
                    storage_symbol,
                    &normalized_exchange,
                    product_id,
                    &source,
                    interval,
                    duration_seconds,
                )
                .unwrap_or_else(|| {
                    sync_with_research_adapter(
                        connection,
                        storage_symbol,
                        &category,
                        &normalized_exchange,
                        yahoo_symbol,
                        &source,
                        interval,
                        duration_seconds,
                        maximum_rows,
                        adapter,
                    )
                });
                match provider_result {
                    Ok(rows) => {
                        successful_sources += 1;
                        rows_received += rows;
                        break;
                    }
                    Err(error) => errors.push(format!("{}: {error}", adapter.provider_id())),
                }
            }
        }
    }

    if successful_sources == 0 {
        let yahoo_source = format!("{source_prefix}_yfinance_fallback");
        let yahoo_result = recent_proxy_result(
            connection,
            storage_symbol,
            &normalized_exchange,
            product_id,
            &yahoo_source,
            interval,
            duration_seconds,
        )
        .unwrap_or_else(|| {
            sync_with_yahoo(
                connection,
                storage_symbol,
                &category,
                &normalized_exchange,
                yahoo_symbol,
                &yahoo_source,
                interval,
                duration_seconds,
                maximum_rows,
            )
        });
        match yahoo_result {
            Ok(rows) => {
                successful_sources += 1;
                rows_received += rows;
            }
            Err(error) => errors.push(format!("Yahoo Finance: {error}")),
        }
    }

    if successful_sources > 0 {
        Ok(rows_received)
    } else {
        Err(errors.join("; "))
    }
}
