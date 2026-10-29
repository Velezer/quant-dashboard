use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]

pub(crate) struct ResearchSelection {
    pub(crate) exchange: &'static str,
    pub(crate) exchange_label: &'static str,
    pub(crate) asset_class: &'static str,
    pub(crate) asset_category_id: &'static str,
    pub(crate) asset_category_label: &'static str,
    pub(crate) profile_id: &'static str,
    pub(crate) profile_label: &'static str,
    pub(crate) trade_style: &'static str,
    pub(crate) trade_style_label: &'static str,
    pub(crate) signal_strategy_id: &'static str,
    pub(crate) signal_strategy_label: &'static str,
    pub(crate) strategy: &'static str,
    pub(crate) strategy_label: &'static str,
    pub(crate) timeframe: &'static str,
    pub(crate) evidence_class: &'static str,
}

pub(crate) fn nwe_dss_research_selection(
    exchange: &str,
    asset_class: &str,
    timeframe: &str,
) -> Result<ResearchSelection, String> {
    let timeframe = quant_core::SIGNAL_SCREENER_TIMEFRAMES
        .iter()
        .find(|supported| **supported == timeframe)
        .copied()
        .ok_or_else(|| "timeframe is not supported for NWE/DSS research".to_string())?;
    let exchange_record = exchange_by_alias(exchange)
        .filter(|record| catalog_string(record, "status") == Some("enabled"))
        .ok_or_else(|| "exchange is not enabled in canonical master data".to_string())?;
    let exchange_id = catalog_string(exchange_record, "id")
        .ok_or_else(|| "enabled exchange is missing its canonical id".to_string())?;
    let category_record = category_by_id_or_research_alias(asset_class)
        .filter(|record| {
            record
                .get("exchange_ids")
                .and_then(Value::as_array)
                .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(exchange_id)))
        })
        .ok_or_else(|| "asset category is not supported by the selected app".to_string())?;
    let category_id = catalog_string(category_record, "id")
        .ok_or_else(|| "asset category is missing its canonical id".to_string())?;
    if !matches!(
        (exchange_id, category_id),
        ("stockbit", "idx_stock")
            | ("pintu", "crypto_perpetual")
            | ("pintu_futures", "crypto_perpetual")
            | ("tokocrypto", "crypto_spot")
            | ("tokocrypto", "tokenized_stock")
    ) {
        return Err("NWE/DSS research is not configured for this app category".to_string());
    }
    let base_profile = master_data_records("plan_profiles")
        .iter()
        .find(|profile| {
            catalog_string(profile, "exchange_id") == Some(exchange_id)
                && catalog_string(profile, "asset_category_id") == Some(category_id)
                && profile.get("research_enabled").and_then(Value::as_bool) == Some(true)
        })
        .and_then(|profile| catalog_string(profile, "id"))
        .ok_or_else(|| {
            "no research profile is configured for the selected app category".to_string()
        })?;
    let category_alias = category_record
        .get("research_aliases")
        .and_then(Value::as_array)
        .and_then(|aliases| aliases.first())
        .and_then(Value::as_str)
        .unwrap_or(category_id);
    let mut selection = research_selection(exchange, category_alias, base_profile)?;
    selection.profile_id = "nwe_dss_causal_bressert_hold_v1";
    selection.profile_label = "NWE + DSS Bressert chronological research";
    selection.signal_strategy_id = "nwe_dss_causal_bressert_hold_v1";
    selection.signal_strategy_label = "NWE + DSS Bressert · research";
    selection.strategy = "nwe_dss_causal_bressert_hold_v1";
    selection.strategy_label = "NWE + DSS Bressert · research";
    selection.timeframe = timeframe;
    Ok(selection)
}

/// Validate the app-first research contract. The selected app owns the
/// execution and asset namespace; the returned strategy label names the actual
/// signal rules so a result can never be silently attributed to another venue
/// or confused with the app itself.
pub(crate) fn research_selection(
    exchange: &str,
    asset_class: &str,
    strategy: &str,
) -> Result<ResearchSelection, String> {
    let exchange_record = exchange_by_alias(exchange)
        .filter(|record| catalog_string(record, "status") == Some("enabled"))
        .ok_or_else(|| "exchange is not enabled in canonical master data".to_string())?;
    let exchange_id = catalog_string(exchange_record, "id")
        .ok_or_else(|| "enabled exchange is missing its canonical id".to_string())?;

    let category_record = category_by_id_or_research_alias(asset_class)
        .filter(|record| {
            record
                .get("exchange_ids")
                .and_then(Value::as_array)
                .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(exchange_id)))
        })
        .ok_or_else(|| "asset category is not supported by the selected app".to_string())?;
    let category_id = catalog_string(category_record, "id")
        .ok_or_else(|| "asset category is missing its canonical id".to_string())?;
    let strategy = strategy.trim();
    let profile = master_data_records("plan_profiles")
        .iter()
        .find(|profile| {
            let dimensions_match = catalog_string(profile, "exchange_id") == Some(exchange_id)
                && catalog_string(profile, "asset_category_id") == Some(category_id);
            let enabled = profile
                .get("research_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let direct_match = ["id", "engine_strategy_id", "evidence_strategy_id"]
                .iter()
                .any(|field| {
                    catalog_string(profile, field)
                        .is_some_and(|value| value.eq_ignore_ascii_case(strategy))
                });
            dimensions_match && enabled && direct_match
        })
        .ok_or_else(|| {
            "no research profile matches this app, asset category, and profile id".to_string()
        })?;

    let research_asset_class = category_record
        .get("research_aliases")
        .and_then(Value::as_array)
        .and_then(|aliases| aliases.first())
        .and_then(Value::as_str)
        .unwrap_or(category_id);
    let trade_style_id = catalog_string(profile, "trade_style_id")
        .ok_or_else(|| "research profile is missing its trade style".to_string())?;
    let trade_style = master_data_records("trade_styles")
        .iter()
        .find(|style| catalog_string(style, "id") == Some(trade_style_id));
    let signal_strategy_id = catalog_string(profile, "signal_strategy_id")
        .ok_or_else(|| "research profile is missing its signal strategy".to_string())?;
    let signal_strategy = master_data_records("signal_strategies")
        .iter()
        .find(|model| catalog_string(model, "id") == Some(signal_strategy_id));
    let signal_strategy_label = signal_strategy
        .and_then(|model| catalog_string(model, "label"))
        .unwrap_or(signal_strategy_id);
    Ok(ResearchSelection {
        exchange: exchange_id,
        exchange_label: catalog_string(exchange_record, "label").unwrap_or(exchange_id),
        asset_class: research_asset_class,
        asset_category_id: category_id,
        asset_category_label: catalog_string(category_record, "label").unwrap_or(category_id),
        profile_id: catalog_string(profile, "id").unwrap_or("unknown_profile"),
        profile_label: catalog_string(profile, "label").unwrap_or("Registered plan profile"),
        trade_style: trade_style_id,
        trade_style_label: trade_style
            .and_then(|style| catalog_string(style, "label"))
            .unwrap_or(trade_style_id),
        signal_strategy_id,
        signal_strategy_label,
        strategy: catalog_string(profile, "evidence_strategy_id")
            .or_else(|| catalog_string(profile, "engine_strategy_id"))
            .ok_or_else(|| "research profile is missing an evidence strategy id".to_string())?,
        strategy_label: signal_strategy_label,
        timeframe: catalog_string(profile, "timeframe").unwrap_or("15m"),
        evidence_class: catalog_string(profile, "evidence_class").unwrap_or(category_id),
    })
}

pub(crate) fn normalize_research_symbol(
    selection: ResearchSelection,
    value: &str,
) -> Result<String, String> {
    let raw = value.trim().to_ascii_uppercase();
    if raw.is_empty() {
        return Err("symbols cannot contain an empty value".to_string());
    }
    let symbol = match selection.exchange {
        "stockbit" if !raw.ends_with(".JK") => format!("{raw}.JK"),
        "tokocrypto" if selection.asset_class == "crypto_spot" && !raw.ends_with("-USD") => {
            format!("{raw}-USD")
        }
        _ => raw,
    };
    let compatible = match selection.exchange {
        "stockbit" => symbol.ends_with(".JK"),
        "pintu" => !symbol.ends_with(".JK") && !is_tokenized_stock(&symbol),
        "tokocrypto" => {
            if selection.asset_class == "tokenized_stock" {
                is_tokenized_stock(&symbol)
            } else {
                is_crypto_spot_symbol(&symbol)
            }
        }
        _ => false,
    };
    if !compatible {
        return Err(format!(
            "symbol {symbol} does not belong to the selected {} research universe",
            selection.exchange_label
        ));
    }
    Ok(symbol)
}

pub(crate) fn research_symbols(
    connection: &Connection,
    selection: ResearchSelection,
    requested_symbols: Option<&str>,
) -> Result<Vec<String>, String> {
    if let Some(raw_symbols) = requested_symbols.filter(|value| !value.trim().is_empty()) {
        let mut symbols = Vec::new();
        for value in raw_symbols.split(',') {
            let normalized = normalize_research_symbol(selection, value)?;
            let raw = value.trim().to_ascii_uppercase();
            let resolve = |alias: &str| {
                instrument_registry::storage_symbol_for_exchange_category(
                    connection,
                    selection.exchange,
                    selection.asset_category_id,
                    alias,
                )
                .map_err(|error| error.to_string())
            };
            let symbol = resolve(&raw)?.or(resolve(&normalized)?).ok_or_else(|| {
                format!(
                    "ticker {raw} is not registered for the selected {} category",
                    selection.exchange_label
                )
            })?;
            if !symbols.iter().any(|existing| existing == &symbol) {
                symbols.push(symbol);
            }
            if symbols.len() >= MAX_RESEARCH_SYMBOLS {
                break;
            }
        }
        return Ok(symbols);
    }

    let mut statement = connection
        .prepare(
            "SELECT DISTINCT storage_symbol, listing_status, venue_verified FROM app.instruments
              WHERE exchange_id=?1 AND asset_category_id=?2 AND is_active=1
              ORDER BY upper(storage_symbol), storage_symbol",
        )
        .map_err(|error| error.to_string())?;
    let candidates = statement
        .query_map(
            params![selection.exchange, selection.asset_category_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;
    let mut symbols = Vec::new();
    for candidate in candidates {
        let (symbol, listing_status, venue_verified) =
            candidate.map_err(|error| error.to_string())?;
        if instrument_registry::research_eligibility(
            selection.exchange,
            &listing_status,
            venue_verified,
        ) != "eligible"
        {
            continue;
        }
        symbols.push(symbol);
        if symbols.len() >= MAX_RESEARCH_SYMBOLS {
            break;
        }
    }
    Ok(symbols)
}
