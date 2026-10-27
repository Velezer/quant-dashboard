use super::*;

mod dss_bressert_mtf;
mod run;
use run::run_rust_scan_inner;
#[cfg(test)]
#[path = "scan_rotation_tests.rs"]
mod scan_rotation_tests;
#[path = "scan_timeframes.rs"]
mod scan_timeframes;

pub(super) fn build_dss_bressert_mtf_annotation(
    connection: &Connection,
    symbol: &str,
    exchange: &str,
    product_id: &str,
) -> Value {
    dss_bressert_mtf::build_annotation(connection, symbol, exchange, product_id)
}
#[derive(Clone, Copy, Default)]
pub(super) struct ScanPlanOptions<'a> {
    pub(super) timeframe: Option<&'a str>,
    pub(super) signal_context: Option<quant_core::PintuSignalTimeframeContext<'a>>,
    pub(super) current_price: Option<f64>,
    pub(super) isolate_evidence: bool,
    pub(super) suffix_plan_id: bool,
    pub(super) signals_only: bool,
}

/// Keep an exchange visible when the latest bounded candle window has no
/// actionable signal. This is explicitly a watchlist row: it carries no
/// directional recommendation or simulated trade levels.

pub(super) fn build_watchlist_plan(
    bars: &[StrategyBar],
    asset: &str,
    class: &str,
    exchange: &str,
    strategy: &str,
    time_window: &str,
) -> Option<Value> {
    let last = bars.last()?;
    if !last.close.is_finite() || last.close <= 0.0 {
        return None;
    }

    let is_day_trade = !strategy.eq_ignore_ascii_case("multibagger");
    let reference_price = last.close;
    let decision_reason = format!(
        "Waiting for a completed DSS Bressert extreme turn and a candle whose wick or body touches the directionally matching causal Nadaraya-Watson outer envelope and closes inside it. The endpoint needs {} completed candles; matching validation evidence is also required before directional guidance.",
        quant_core::NadarayaWatsonEnvelopeConfig::default().minimum_endpoint_envelope_bars()
    );

    Some(serde_json::json!({
        "Class": class,
        "Asset": asset,
        "Exchange": exchange,
        "Direction": "WAIT",
        "Decision": "WAIT",
        "Decision_Reason": decision_reason,
        "Action": "MONITOR",
        "Compute_Engine": "rust_quant_engine",
        "Regime": "WATCHLIST_NO_CURRENT_SIGNAL",
        "DSS_Regime": "WATCHLIST",
        "Strategy_ID": strategy,
        "Strategy_Status": "WATCHLIST_NO_CURRENT_SIGNAL",
        "Is_Watchlist": true,
        "Is_Optimized_Edge": false,
        "timeframe": if is_day_trade { "15m" } else { "1d" },
        "is_day_trade": is_day_trade,
        "Price": reference_price,
        "Current_Price": reference_price,
        "Price_Time_Zone": format!("{reference_price} - {reference_price}"),
        "Zone_Status": "WATCHLIST",
        "State": "WATCHLIST",
        "Time_Window": time_window,
        "Est_Hold": "No holding period is estimated without matching simulation evidence and a current strategy signal.",
        "Invalidation": "No order levels until the matching simulation passes",
        "Catalyst_Thesis": format!("{exchange} reference row: no current strategy signal has passed both required confirmations and the matching evidence gate."),
    }))
}

#[cfg(test)]
pub(super) fn append_scan_plan(
    connection: &Connection,
    running_plans: &mut Vec<Value>,
    potential_plans: &mut Vec<Value>,
    bars: &[StrategyBar],
    symbol: &str,
    asset: &str,
    strategy: &str,
    evidence_class: &str,
    class: &str,
    exchange: &str,
    time_window: &str,
    is_tokenized: bool,
    is_multibagger: bool,
    allow_watchlist: bool,
    cagr_stats: &mut HashMap<String, Value>,
    product_id: &str,
) {
    append_scan_plan_with_options(
        connection,
        connection,
        running_plans,
        potential_plans,
        bars,
        symbol,
        asset,
        strategy,
        evidence_class,
        class,
        exchange,
        time_window,
        is_tokenized,
        is_multibagger,
        allow_watchlist,
        cagr_stats,
        product_id,
        ScanPlanOptions::default(),
    );
}

pub(super) fn append_scan_plan_with_options(
    connection: &Connection,
    registry_connection: &Connection,
    running_plans: &mut Vec<Value>,
    potential_plans: &mut Vec<Value>,
    bars: &[StrategyBar],
    symbol: &str,
    asset: &str,
    strategy: &str,
    evidence_class: &str,
    class: &str,
    exchange: &str,
    time_window: &str,
    is_tokenized: bool,
    is_multibagger: bool,
    allow_watchlist: bool,
    cagr_stats: &mut HashMap<String, Value>,
    product_id: &str,
    options: ScanPlanOptions<'_>,
) {
    if exchange.eq_ignore_ascii_case("stockbit")
        && instrument_registry::is_excluded(registry_connection, exchange, "idx_stock", asset)
            .unwrap_or(true)
    {
        return;
    }
    if exchange.eq_ignore_ascii_case("stockbit")
        && !bars
            .last()
            .is_some_and(|bar| stockbit_candidate_price_eligible(bar.close))
    {
        return;
    }
    let mut isolated_evidence = HashMap::new();
    let plan_evidence = if options.isolate_evidence {
        &mut isolated_evidence
    } else {
        cagr_stats
    };
    if !options.isolate_evidence && !options.signals_only {
        // Screener rows only use the current directional signal and its
        // indicator score. Historical trade-plan validation remains available
        // to research callers, but must not backtest every product per scan.
        let _ = merge_local_historical_evidence(
            connection,
            plan_evidence,
            symbol,
            strategy,
            Some(exchange),
        );
    }

    let mut fresh_reg_plan = options.signal_context.map_or_else(
        || build_registered_plan(bars, symbol, Some(asset), strategy),
        |context| {
            quant_core::build_registered_plan_with_signal_context(
                bars,
                symbol,
                Some(asset),
                strategy,
                Some(context),
            )
        },
    );
    if let Some(reg_plan) = fresh_reg_plan.as_mut() {
        if let Some(timeframe) = options.timeframe {
            reg_plan.plan["timeframe"] = serde_json::json!(timeframe);
            reg_plan.plan["Zone_Timeframe"] = serde_json::json!(timeframe);
        }
        if let Some(current_price) = options
            .current_price
            .filter(|price| price.is_finite() && *price > 0.0)
        {
            reg_plan.plan["Current_Price"] = serde_json::json!(current_price);
        }
    }
    fresh_reg_plan = fresh_reg_plan.filter(|plan| {
        if options.signals_only
            && !plan.signal.direction.eq_ignore_ascii_case("LONG")
            && !plan.signal.direction.eq_ignore_ascii_case("SHORT")
        {
            return false;
        }
        let timeframe = options.timeframe.unwrap_or_else(|| {
            plan.plan
                .get("timeframe")
                .and_then(Value::as_str)
                .unwrap_or("15m")
        });
        bars.last().is_some_and(|bar| {
            !signal_alerts::signal_candle_is_expired(bar.time, timeframe, now_secs() as i64)
        })
    });
    if let Some(reg_plan) = fresh_reg_plan {
        let mut plan = reg_plan.plan;
        if let Some(object) = plan.as_object_mut() {
            object.insert("Exchange".to_string(), serde_json::json!(exchange));
            object.insert(
                "Signal_Direction".to_string(),
                serde_json::json!(reg_plan.signal.direction),
            );
            object.insert(
                "Signal_Strength".to_string(),
                serde_json::json!(reg_plan.signal.strength),
            );
            object.insert(
                "Strategy_Strength".to_string(),
                serde_json::json!(reg_plan.signal.strategy_strength),
            );
            if let Some(confluence) = &reg_plan.signal.confluence {
                object.insert(
                    "Signal_Confluence_Score".to_string(),
                    serde_json::json!(confluence.score),
                );
                object.insert(
                    "Signal_Confluence".to_string(),
                    serde_json::json!(confluence),
                );
                object.insert(
                    "Signal_Scoring_Model_ID".to_string(),
                    serde_json::json!(quant_core::SIGNAL_CONFLUENCE_SCORING_MODEL_ID),
                );
            }
        }
        annotate_product_identity(&mut plan, product_id);
        let timeframe = plan
            .get("timeframe")
            .and_then(Value::as_str)
            .unwrap_or("15m")
            .to_string();
        annotate_plan_market_data(
            connection,
            &mut plan,
            symbol,
            exchange,
            &timeframe,
            is_tokenized,
        );
        if is_multibagger {
            annotate_multibagger_plan(&mut plan, symbol);
        }
        if is_tokenized {
            if let Some(object) = plan.as_object_mut() {
                object.insert("Class".to_string(), serde_json::json!("TOKENIZED_STOCK"));
                object.insert(
                    "Strategy_ID".to_string(),
                    serde_json::json!(TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID),
                );
                object.insert(
                    "Time_Window".to_string(),
                    serde_json::json!("Tokocrypto tokenized stock · 15m proxy research signal"),
                );
            }
        } else if quant_core::is_tokocrypto_spot_confluence_strategy(&strategy) {
            annotate_tokocrypto_spot_plan(&mut plan);
        }
        annotate_plan_identity(&mut plan, symbol);
        if let Some(timeframe) = options.timeframe {
            scan_timeframes::annotate_plan_signal_timeframe(&mut plan, timeframe);
            if options.suffix_plan_id {
                scan_timeframes::append_timeframe_to_plan_id(&mut plan, timeframe);
            }
        }
        foreign_flow::attach_to_plan(connection, &mut plan, symbol);
        attach_strategy_invention(
            &mut plan,
            bars,
            strategy,
            plan_evidence.get(&strategy_backtest_key(symbol, evidence_class, strategy)),
        );
        if let (Some(signal_bar_time), Some(object)) =
            (bars.last().map(|bar| bar.time), plan.as_object_mut())
        {
            object.insert(
                "Signal_Bar_Time".to_string(),
                serde_json::json!(signal_bar_time),
            );
        }
        let validated = apply_backtest_gate(&mut plan, plan_evidence);
        let state_str = plan.get("State").and_then(Value::as_str).unwrap_or("");
        if validated
            && (state_str.eq_ignore_ascii_case("RUNNING")
                || state_str.eq_ignore_ascii_case("IN_ZONE"))
        {
            running_plans.push(plan);
        } else {
            potential_plans.push(plan);
        }
    } else if allow_watchlist {
        if let Some(mut plan) =
            build_watchlist_plan(bars, asset, class, exchange, strategy, time_window)
        {
            if let Some(timeframe) = options.timeframe {
                plan["timeframe"] = serde_json::json!(timeframe);
                plan["Zone_Timeframe"] = serde_json::json!(timeframe);
            }
            if let Some(current_price) = options
                .current_price
                .filter(|price| price.is_finite() && *price > 0.0)
            {
                plan["Price"] = serde_json::json!(current_price);
                plan["Current_Price"] = serde_json::json!(current_price);
                plan["Price_Time_Zone"] =
                    serde_json::json!(format!("{current_price} - {current_price}"));
            }
            let timeframe = plan
                .get("timeframe")
                .and_then(Value::as_str)
                .unwrap_or("15m")
                .to_string();
            annotate_plan_market_data(
                connection,
                &mut plan,
                symbol,
                exchange,
                &timeframe,
                is_tokenized,
            );
            if is_multibagger {
                annotate_multibagger_plan(&mut plan, symbol);
            }
            if is_tokenized {
                if let Some(object) = plan.as_object_mut() {
                    object.insert("Class".to_string(), serde_json::json!("TOKENIZED_STOCK"));
                    object.insert(
                        "Strategy_ID".to_string(),
                        serde_json::json!(TOKENIZED_STOCK_CONFLUENCE_STRATEGY_ID),
                    );
                    object.insert(
                        "Time_Window".to_string(),
                        serde_json::json!("Tokocrypto bStock / 24/7 on-chain"),
                    );
                }
            } else if quant_core::is_tokocrypto_spot_confluence_strategy(&strategy) {
                annotate_tokocrypto_spot_plan(&mut plan);
            }
            annotate_product_identity(&mut plan, product_id);
            annotate_plan_identity(&mut plan, symbol);
            if let Some(timeframe) = options.timeframe {
                scan_timeframes::annotate_plan_signal_timeframe(&mut plan, timeframe);
                if options.suffix_plan_id {
                    scan_timeframes::append_timeframe_to_plan_id(&mut plan, timeframe);
                }
            }
            foreign_flow::attach_to_plan(connection, &mut plan, symbol);
            attach_strategy_invention(
                &mut plan,
                bars,
                strategy,
                plan_evidence.get(&strategy_backtest_key(symbol, evidence_class, strategy)),
            );
            apply_backtest_gate(&mut plan, plan_evidence);
            potential_plans.push(plan);
        }
    }
}

pub(super) fn annotate_product_identity(plan: &mut Value, product_id: &str) {
    if !product_id.is_empty() {
        if let Some(object) = plan.as_object_mut() {
            object.insert("Product_ID".to_string(), serde_json::json!(product_id));
        }
    }
}

/// Add research-only Stockbit day-trade candidates from registered research
/// profiles. These sit beside the baseline plan so the Trade Plans strategy
/// filter can inspect each research-derived model over the same stored 15m
/// universe. A monitor-only row remains available when a model has no active
/// signal; it does not imply an entry.
#[cfg(test)]
pub(super) fn append_stockbit_day_trade_research_plans(
    connection: &Connection,
    running_plans: &mut Vec<Value>,
    potential_plans: &mut Vec<Value>,
    bars: &[StrategyBar],
    symbol: &str,
    asset: &str,
    cagr_stats: &mut HashMap<String, Value>,
) {
    let eligible_profiles = master_data_records("plan_profiles")
        .iter()
        .filter(|profile| {
            catalog_string(profile, "exchange_id") == Some("stockbit")
                && catalog_string(profile, "asset_category_id") == Some("idx_stock")
                && catalog_string(profile, "trade_style_id") == Some("day_trade")
                && profile
                    .get("research_enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                && catalog_string(profile, "execution_scope") == Some("research_only")
        })
        .filter(|profile| {
            let Some(strategy_id) = catalog_string(profile, "signal_strategy_id") else {
                return false;
            };
            master_data_records("signal_strategies")
                .iter()
                .any(|model| {
                    catalog_string(model, "id") == Some(strategy_id)
                        && (catalog_string(model, "status") == Some("research_only")
                            || model
                                .get("research_only")
                                .and_then(Value::as_bool)
                                .unwrap_or(false))
                })
        })
        .collect::<Vec<_>>();

    for profile in eligible_profiles {
        let Some(strategy) = catalog_string(profile, "engine_strategy_id") else {
            continue;
        };
        append_scan_plan(
            connection,
            running_plans,
            potential_plans,
            bars,
            symbol,
            asset,
            strategy,
            catalog_string(profile, "evidence_class").unwrap_or("idx"),
            catalog_string(profile, "asset_class_code").unwrap_or("IDX_STOCK"),
            catalog_string(profile, "exchange_code").unwrap_or("STOCKBIT"),
            catalog_string(profile, "time_window")
                .unwrap_or("Stockbit day-trade research · not validated"),
            false,
            false,
            true,
            cagr_stats,
            "",
        );
    }
}

pub(super) fn run_rust_scan(
    app_connection: &Connection,
    market_database: &str,
    interval_min: u64,
) -> Result<Value, rusqlite::Error> {
    run_rust_scan_inner(app_connection, market_database, interval_min, None)
}

pub(super) fn run_rust_scan_with_state(
    app_connection: &Connection,
    market_database: &str,
    interval_min: u64,
    state: &AppState,
) -> Result<Value, rusqlite::Error> {
    run_rust_scan_inner(app_connection, market_database, interval_min, Some(state))
}

pub(super) fn chrono_or_simple_timestamp() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let (days, hours, minutes, seconds) = utc_day_time_parts(duration.as_secs());
    format!("Day-{days} {hours:02}:{minutes:02}:{seconds:02} UTC")
}

pub(super) fn format_timestamp(secs: u64) -> String {
    // Format unix seconds as YYYY-MM-DD HH:MM:SS UTC
    let (days_since_epoch, hours, minutes, seconds) = utc_day_time_parts(secs);
    let (year, month, day) = quant_core::gregorian_date_from_unix_days(days_since_epoch as i64);
    format!("{year:04}-{month:02}-{day:02} {hours:02}:{minutes:02}:{seconds:02} UTC")
}

fn utc_day_time_parts(seconds_since_epoch: u64) -> (u64, u64, u64, u64) {
    let seconds_per_day = SECONDS_PER_DAY as u64;
    let days = seconds_since_epoch / seconds_per_day;
    let seconds_in_day = seconds_since_epoch % seconds_per_day;
    let seconds_per_hour = quant_core::SECONDS_PER_HOUR as u64;
    let seconds_per_minute = quant_core::SECONDS_PER_MINUTE as u64;
    let hours = seconds_in_day / seconds_per_hour;
    let minutes = (seconds_in_day % seconds_per_hour) / seconds_per_minute;
    let seconds = seconds_in_day % seconds_per_minute;
    (days, hours, minutes, seconds)
}

pub(super) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
