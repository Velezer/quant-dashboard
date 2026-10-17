use super::*;

pub(super) fn is_tokenized_stock(sym: &str) -> bool {
    let normalized = sym.trim().to_ascii_uppercase();
    if normalized.ends_with(".JK") {
        return false;
    }
    master_data_records("assets").iter().any(|asset| {
        if catalog_string(asset, "asset_category_id") != Some("tokenized_stock") {
            return false;
        }
        catalog_string(asset, "symbol")
            .into_iter()
            .chain(
                asset
                    .get("aliases")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str),
            )
            .any(|candidate| normalized_asset_symbol(candidate) == normalized_asset_symbol(sym))
    })
}

pub(super) fn normalized_asset_symbol(symbol: &str) -> String {
    let mut normalized = symbol.trim().to_ascii_uppercase();
    for suffix in ["USDT-PERP", ".JK", "-USD", "/USDT", "-USDT"] {
        normalized = normalized.replace(suffix, "");
    }
    normalized
        .strip_suffix("USDT")
        .unwrap_or(&normalized)
        .to_string()
}

/// Normal crypto symbols are the Tokocrypto spot universe when they are
/// represented by a Yahoo-compatible USD pair in the durable store. Keeping
/// this predicate separate from tokenized-stock detection prevents a normal
/// BTC/ETH row from being mislabeled as a synthetic equity.
pub(super) fn is_crypto_spot_symbol(sym: &str) -> bool {
    let normalized = sym.trim().to_uppercase();
    normalized.ends_with("-USD") && !normalized.ends_with(".JK") && !is_tokenized_stock(&normalized)
}

/// Map a Tokocrypto tokenized-stock symbol to the real listed equity ticker
/// used by the yfinance fallback. The requested symbol remains the SQLite
/// series key so the plan stays attached to Tokocrypto in the app.
pub(super) fn underlying_yfinance_symbol(sym: &str) -> String {
    if sym.trim().to_uppercase().ends_with(".JK") {
        return sym.trim().to_uppercase();
    }
    let normalized = normalized_asset_symbol(sym);
    if let Some(asset) = master_data_records("assets").iter().find(|asset| {
        catalog_string(asset, "asset_category_id") == Some("tokenized_stock")
            && normalized_asset_symbol(catalog_string(asset, "symbol").unwrap_or("")) == normalized
    }) {
        if let Some(provider_symbol) = catalog_string(asset, "fallback_symbol") {
            return provider_symbol.to_string();
        }
    }
    let clean = sym
        .trim()
        .to_uppercase()
        .replace(".JK", "")
        .replace("-USD", "")
        .replace("USDT-PERP", "")
        .replace("/USDT", "")
        .replace("-USDT", "");
    let clean = clean.strip_suffix("USDT").unwrap_or(&clean).to_string();
    if is_tokenized_stock(&clean) {
        clean
    } else {
        sym.trim().to_uppercase()
    }
}

pub(super) fn annotate_plan_market_data(
    connection: &Connection,
    plan: &mut Value,
    symbol: &str,
    exchange: &str,
    timeframe: &str,
    is_tokenized: bool,
) {
    let source =
        selected_candle_source_for_timeframe(connection, symbol, Some(exchange), timeframe)
            .ok()
            .flatten();
    let provider = source.as_deref().and_then(|selected| {
        connection
            .query_row(
                "SELECT data_provider FROM series WHERE symbol=?1 AND source=?2
                 ORDER BY COALESCE(last_success, '') DESC LIMIT 1",
                params![symbol, selected],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten()
    });
    annotate_plan_market_data_fields(
        plan,
        symbol,
        is_tokenized,
        source.as_deref(),
        provider.as_deref(),
    );
}

fn annotate_plan_market_data_fields(
    plan: &mut Value,
    symbol: &str,
    is_tokenized: bool,
    source: Option<&str>,
    provider: Option<&str>,
) {
    if let Some(object) = plan.as_object_mut() {
        object.insert("Data_Symbol".to_string(), serde_json::json!(symbol));
        object.insert("Data_Source".to_string(), serde_json::json!(source));
        object.insert("Data_Provider".to_string(), serde_json::json!(provider));
        if let Some(selected) = source.as_deref() {
            object.insert(
                "Data_Quality".to_string(),
                serde_json::json!(if is_research_proxy_source(selected) {
                    "Research price proxy; verify app quote before execution"
                } else {
                    "App venue history; research display only"
                }),
            );
        }
        if is_tokenized {
            object.insert("Class".to_string(), serde_json::json!("TOKENIZED_STOCK"));
            object.insert(
                "Strategy_ID".to_string(),
                serde_json::json!(quant_core::TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID),
            );
            object.insert(
                "Underlying_Symbol".to_string(),
                serde_json::json!(underlying_yfinance_symbol(symbol)),
            );
            object.insert(
                "Execution_Instrument".to_string(),
                serde_json::json!("Tokocrypto tokenized stock"),
            );
        }
    }
}

pub(super) fn annotate_tokocrypto_spot_plan(plan: &mut Value) {
    if !plan
        .get("Exchange")
        .and_then(Value::as_str)
        .is_some_and(|exchange| exchange.eq_ignore_ascii_case("TOKOCRYPTO"))
    {
        return;
    }
    if let Some(object) = plan.as_object_mut() {
        object.insert("Exchange".to_string(), serde_json::json!("TOKOCRYPTO"));
        object.insert("Class".to_string(), serde_json::json!("CRYPTO_SPOT"));
        object.insert(
            "Strategy_ID".to_string(),
            serde_json::json!(quant_core::TOKOCRYPTO_SPOT_CONFLUENCE_STRATEGY_ID),
        );
        object.insert(
            "DSS_Regime".to_string(),
            serde_json::json!(format!(
                "Tokocrypto confluence: {} of {} directional votes",
                quant_core::TokocryptoConfluenceConfig::default().minimum_agreeing_indicators,
                quant_core::tokocrypto_confluence_indicator_ids().len()
            )),
        );
        object.insert(
            "Time_Window".to_string(),
            serde_json::json!("Tokocrypto spot / 24/7"),
        );
        object.insert(
            "Execution_Instrument".to_string(),
            serde_json::json!("Tokocrypto crypto spot"),
        );
        object.insert(
            "Catalyst_Thesis".to_string(),
            serde_json::json!(format!(
                "Tokocrypto crypto spot, long-only: at least {} of {} directional votes align. Research only; unvalidated; stored proxy candles are not Tokocrypto IDR execution evidence.",
                quant_core::TokocryptoConfluenceConfig::default().minimum_agreeing_indicators,
                quant_core::tokocrypto_confluence_indicator_ids().len()
            )),
        );
    }
}

pub(super) fn signal_strategy_id(strategy: &str) -> &'static str {
    let normalized = strategy.trim().to_ascii_lowercase();
    if let Some(model) = master_data_records("signal_strategies")
        .iter()
        .find(|model| catalog_string(model, "id") == Some(normalized.as_str()))
    {
        return catalog_string(model, "id").unwrap_or("registered_signal_model");
    }
    master_data::profile_for_strategy(strategy)
        .and_then(|profile| catalog_string(profile, "signal_strategy_id"))
        .unwrap_or("registered_signal_model")
}

pub(super) fn signal_strategy_name(strategy: &str) -> &'static str {
    let strategy_id = signal_strategy_id(strategy);
    master_data_records("signal_strategies")
        .iter()
        .find(|model| catalog_string(model, "id") == Some(strategy_id))
        .and_then(|model| catalog_string(model, "label"))
        .unwrap_or("Registered signal model")
}

pub(super) fn trade_style_id(strategy: &str) -> &'static str {
    master_data::profile_for_strategy(strategy)
        .and_then(|profile| catalog_string(profile, "trade_style_id"))
        .unwrap_or("day_trade")
}

pub(super) fn plan_objective(strategy: &str) -> &'static str {
    let style_id = trade_style_id(strategy);
    master_data_records("trade_styles")
        .iter()
        .find(|style| catalog_string(style, "id") == Some(style_id))
        .and_then(|style| catalog_string(style, "objective"))
        .unwrap_or("Use the selected app's execution rules.")
}

pub(super) fn annotate_plan_semantics(
    plan: &mut Value,
    strategy: &str,
    class: &str,
    exchange: &str,
) {
    let app_product_profile = plan
        .get("Product_ID")
        .and_then(Value::as_str)
        .and_then(|product_id| master_data::profile_for_app_product(exchange, product_id));
    let explicit_profile = plan
        .get("Plan_Profile_ID")
        .and_then(Value::as_str)
        .and_then(master_data::profile_by_id);
    let profile = app_product_profile
        .or(explicit_profile)
        .or_else(|| master_data::profile_for_strategy(strategy));
    let has_confirmed_direction = plan
        .get("Signal_Direction")
        .or_else(|| plan.get("signal_direction"))
        .or_else(|| plan.get("Direction"))
        .and_then(Value::as_str)
        .is_some_and(|direction| {
            let direction = direction.to_ascii_uppercase();
            direction.contains("LONG") || direction.contains("SHORT")
        });
    let fallback_execution_profile = plan
        .get("Engine_Strategy_ID")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| strategy.to_owned());
    if let Some(object) = plan.as_object_mut() {
        object.insert(
            "Required_Signal_Confirmations".to_string(),
            serde_json::json!({
                "status": if has_confirmed_direction { "CONFIRMED" } else { "WAITING" },
                "required_ids": quant_core::REQUIRED_SIGNAL_CONFIRMATION_IDS,
                "dss_near_cross_score_threshold_points": quant_core::DSS_BRESSERT_NEAR_SPREAD_THRESHOLD,
                "nadaraya_endpoint_envelope_warmup_bars": quant_core::NadarayaWatsonEnvelopeConfig::default().minimum_endpoint_envelope_bars(),
            }),
        );
        object.insert(
            "Execution_Profile_ID".to_string(),
            serde_json::json!(app_product_profile
                .and_then(|profile| catalog_string(profile, "id"))
                .unwrap_or(&fallback_execution_profile)),
        );
        if let Some(profile) = profile {
            if let Some(evidence_strategy_id) = catalog_string(profile, "evidence_strategy_id") {
                object.insert(
                    "Strategy_ID".to_string(),
                    serde_json::json!(evidence_strategy_id),
                );
            }
            if let Some(profile_id) = catalog_string(profile, "id") {
                object.insert("Plan_Profile_ID".to_string(), serde_json::json!(profile_id));
            }
            if let Some(category_id) = catalog_string(profile, "asset_category_id") {
                object.insert(
                    "Asset_Category_ID".to_string(),
                    serde_json::json!(category_id),
                );
            }
            if let Some(category_code) = catalog_string(profile, "asset_class_code") {
                object.insert("Asset_Class".to_string(), serde_json::json!(category_code));
            } else {
                object.insert("Asset_Class".to_string(), serde_json::json!(class));
            }
            if let Some(exchange_code) = catalog_string(profile, "exchange_code") {
                object.insert("Exchange".to_string(), serde_json::json!(exchange_code));
            }
        } else {
            object.insert("Asset_Class".to_string(), serde_json::json!(class));
        }
        object.insert(
            "Signal_Strategy_ID".to_string(),
            serde_json::json!(signal_strategy_id(strategy)),
        );
        object.insert(
            "Signal_Strategy_Name".to_string(),
            serde_json::json!(signal_strategy_name(strategy)),
        );
        object.insert(
            "Trade_Style".to_string(),
            serde_json::json!(trade_style_id(strategy)),
        );
        object.insert(
            "Plan_Objective".to_string(),
            serde_json::json!(plan_objective(strategy)),
        );
        object.insert(
            "Semantic_Contract".to_string(),
            serde_json::json!(
                "Exchange selects execution app; Asset_Class selects instrument; Trade_Style selects holding objective; Signal_Strategy_ID selects signal rules"
            ),
        );
        if master_data::profile_for_strategy(strategy).is_none() && !exchange.trim().is_empty() {
            object.insert("Exchange".to_string(), serde_json::json!(exchange));
        }
    }
}

pub(super) fn annotate_plan_identity(plan: &mut Value, symbol: &str) {
    let (engine_strategy, class, exchange, product_id, explicit_profile_id) = plan
        .as_object()
        .map(|object| {
            (
                object
                    .get("Engine_Strategy_ID")
                    .or_else(|| object.get("Strategy_ID"))
                    .and_then(Value::as_str)
                    .unwrap_or("plan")
                    .to_string(),
                object
                    .get("Class")
                    .or_else(|| object.get("Asset_Class"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                object
                    .get("Exchange")
                    .or_else(|| object.get("Venue"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                object
                    .get("Product_ID")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                object
                    .get("Plan_Profile_ID")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            )
        })
        .unwrap_or_else(|| {
            (
                "plan".to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            )
        });

    let profile = crate::master_data::profile_for_app_product(&exchange, &product_id)
        .or_else(|| crate::master_data::profile_by_id(&explicit_profile_id))
        .or_else(|| master_data::profile_for_strategy(&engine_strategy));
    let strategy = profile
        .and_then(|profile| catalog_string(profile, "evidence_strategy_id"))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| engine_strategy.clone());

    if let Some(object) = plan.as_object_mut() {
        let plan_id_strategy = strategy.to_ascii_lowercase();
        object.insert("Symbol".to_string(), serde_json::json!(symbol));
        object.insert(
            "Engine_Strategy_ID".to_string(),
            serde_json::json!(engine_strategy),
        );
        object.insert("Strategy_ID".to_string(), serde_json::json!(strategy));
        object.insert(
            "Plan_ID".to_string(),
            serde_json::json!(if product_id.is_empty() {
                format!("{plan_id_strategy}::{symbol}")
            } else {
                format!(
                    "{}::{product_id}::{plan_id_strategy}::{symbol}",
                    exchange.to_ascii_lowercase()
                )
            }),
        );
    }
    annotate_plan_semantics(plan, &strategy, &class, &exchange);
}

pub(super) fn is_multibagger_symbol(symbol: &str) -> bool {
    let normalized = symbol.trim();
    multibagger_seed_symbols().any(|candidate| {
        candidate.eq_ignore_ascii_case(normalized)
            || candidate
                .trim_end_matches(".JK")
                .eq_ignore_ascii_case(normalized.trim_end_matches(".JK"))
    })
}

pub(super) fn annotate_multibagger_plan(plan: &mut Value, symbol: &str) {
    if let Some(object) = plan.as_object_mut() {
        object.insert("Symbol".to_string(), serde_json::json!(symbol));
        object.insert("Exchange".to_string(), serde_json::json!("STOCKBIT"));
        object.insert("Class".to_string(), serde_json::json!("IDX_STOCK"));
        object.insert("Strategy_ID".to_string(), serde_json::json!("multibagger"));
        object.insert(
            "Strategy_Name".to_string(),
            serde_json::json!("Multibagger objective"),
        );
        object.insert(
            "Target_Calendar_Month_Return_Min_Pct".to_string(),
            serde_json::json!(MIN_MULTIBAGGER_CALENDAR_MONTH_RETURN_PCT),
        );
        object.insert("timeframe".to_string(), serde_json::json!("1d"));
        object.insert("is_day_trade".to_string(), serde_json::json!(false));
        object.insert(
            "DSS_Regime".to_string(),
            serde_json::json!("STOCKBIT_DAILY_MULTIBAGGER"),
        );
        object.insert(
            "Time_Window".to_string(),
            serde_json::json!("Stockbit multi-year holding / daily close"),
        );
        object.insert(
            "Est_Hold".to_string(),
            serde_json::json!("Multi-year compounder candidate"),
        );
        object.insert(
            "Invalidation".to_string(),
            serde_json::json!(
                "Daily close below structural stop or calendar-month return proof failure"
            ),
        );
        object.insert(
            "Catalyst_Thesis".to_string(),
            serde_json::json!(format!(
                "{symbol} Stockbit multibagger candidate from the local daily price screen: long-only daily structure with a hard goal of at least 100% fee-adjusted, risk-sized account growth in a complete calendar month. Price screening is not performance validation."
            )),
        );
    }
}
