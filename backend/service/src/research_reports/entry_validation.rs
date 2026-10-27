use super::*;
use quant_core::{EntryModelKind, VenueFeePolicy};
use serde_json::json;

pub(super) const PROTOCOL_ID: &str = quant_core::ENTRY_SCORE_VALIDATION_PROTOCOL_ID;
#[path = "entry_validation_store.rs"]
mod store;

pub(crate) fn entry_validation_timeframe(timeframe: &str) -> Result<&str, String> {
    DSS_BRESSERT_MTF_TIMEFRAMES
        .iter()
        .any(|definition| definition.timeframe == timeframe)
        .then_some(timeframe)
        .ok_or_else(|| "Choose a registered entry-model timeframe.".to_string())
}

pub(crate) fn entry_validation_list(
    connection: &Connection,
    exchange: &str,
    product: Option<&str>,
    timeframe: &str,
    search: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Value, String> {
    entry_validation_timeframe(timeframe)?;
    let canonical = exchange_by_alias(exchange)
        .and_then(|record| catalog_string(record, "id"))
        .ok_or_else(|| "Choose a registered app.".to_string())?;
    store::list(
        connection,
        canonical,
        product.filter(|value| *value != "all"),
        timeframe,
        search,
        offset,
        limit,
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn entry_validation_saved(
    connection: &Connection,
    instrument_id: &str,
    timeframe: &str,
) -> Result<Option<Value>, String> {
    entry_validation_timeframe(timeframe)?;
    if store::instrument(connection, instrument_id)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err("The exact active registered instrument was not found.".into());
    }
    store::get_report(connection, instrument_id, timeframe).map_err(|error| error.to_string())
}

pub(crate) fn run_entry_validation(
    connection: &Connection,
    market_directory: &str,
    instrument_id: &str,
    timeframe: &str,
) -> Result<Value, String> {
    entry_validation_timeframe(timeframe)?;
    let instrument = store::instrument(connection, instrument_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "The exact active registered instrument was not found.".to_string())?;
    let report = build_report(connection, market_directory, instrument, timeframe)?;
    store::save_report(connection, &report).map_err(|error| error.to_string())?;
    Ok(report)
}

fn text<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or("")
}

fn matching_models(
    connection: &Connection,
    instrument: &Value,
    timeframe: &str,
) -> Result<Vec<Value>, String> {
    let catalog = signal_screener_master_data_for_connection(connection)
        .map_err(|error| error.to_string())?;
    Ok(catalog
        .get("signal_score_models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|model| {
            let matches = |key: &str, wanted: &str| {
                model
                    .get(key)
                    .and_then(Value::as_array)
                    .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(wanted)))
            };
            text(model, "timeframe") == timeframe
                && matches("exchange_ids", text(instrument, "exchange_id"))
                && matches("product_ids", text(instrument, "product_id"))
                && matches("asset_category_ids", text(instrument, "asset_category_id"))
                && [EntryModelKind::Nwe, EntryModelKind::Dss]
                    .iter()
                    .any(|kind| text(model, "score_model_id") == kind.score_model_id())
        })
        .cloned()
        .collect())
}

fn empty_history() -> Value {
    json!({"bars":0,"first_ts":null,"last_ts":null,"coverage_days":0,
        "stored_coverage_days":0,"read_limit":MAX_EVIDENCE_CANDLES,
        "read_limit_reached":false,"completed_only":true,"data_fingerprint":null})
}

fn read_history(
    market_directory: &str,
    instrument: &Value,
    timeframe: &str,
) -> Result<(Vec<StrategyBar>, Value, Value), String> {
    let Some(market) =
        open_market_history_for_symbol(market_directory, text(instrument, "storage_symbol"), false)
            .map_err(|error| error.to_string())?
    else {
        return Ok((
            Vec::new(),
            json!({"id":"unavailable","venue_native":false,"is_proxy":false}),
            empty_history(),
        ));
    };
    market
        .execute_batch("BEGIN DEFERRED")
        .map_err(|error| error.to_string())?;
    let symbol = text(instrument, "storage_symbol");
    let exchange = text(instrument, "exchange_id");
    let product = text(instrument, "product_id");
    let Some((source, provider)) =
        selected_candle_source_metadata_for_product(&market, symbol, exchange, product, timeframe)
            .map_err(|error| error.to_string())?
    else {
        return Ok((
            Vec::new(),
            json!({"id":"unavailable","venue_native":false,"is_proxy":false}),
            empty_history(),
        ));
    };
    let metadata = load_series_metadata_from_source(&market, symbol, timeframe, false, &source)
        .map_err(|error| error.to_string())?;
    // A rollup must retain either the exact venue/product scope or a compatible
    // shared research reference for the registered asset category.
    let storage_interval = metadata
        .get("storage_timeframe")
        .and_then(Value::as_str)
        .unwrap_or(timeframe);
    let shared_category = crate::research_proxy::shared::category_for_product(exchange, product)
        .filter(|category| *category == text(instrument, "asset_category_id"));
    let shared_reference = shared_category.is_some_and(|category| {
        crate::research_proxy::shared::is_shared_source_for_category(&source, category)
    });
    let compatible_scope: bool = market
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM series WHERE symbol=?1 AND interval=?2 AND source=?3
            AND ((exchange_id=?4 AND product_id=?5)
                OR (?6 AND exchange_id=?7 AND product_id='' AND instrument_id IS NULL
                    AND instrument_kind=?8)))",
            params![
                symbol,
                storage_interval,
                source,
                exchange,
                product,
                shared_reference,
                crate::research_proxy::shared::SOURCE_EXCHANGE_ID,
                shared_category.unwrap_or("")
            ],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !compatible_scope {
        return Err(
            "Selected history does not match the registered app/product or shared asset category."
                .into(),
        );
    }
    let candles =
        load_series_candles_from_source(&market, symbol, timeframe, MAX_EVIDENCE_CANDLES, &source)
            .map_err(|error| error.to_string())?;
    let now_ns = (now_secs() as i64).saturating_mul(NANOS_PER_SECOND);
    let duration = timeframe_duration_ns(timeframe).ok_or("Invalid timeframe")?;
    let read_limit_reached = candles.len() >= MAX_EVIDENCE_CANDLES as usize;
    let bars: Vec<StrategyBar> = candles
        .into_iter()
        .filter(|bar| bar.time.saturating_add(duration) <= now_ns)
        .map(|bar| StrategyBar {
            time: bar.time,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
        })
        .collect();
    let first = bars.first().map(|bar| bar.time);
    let last = bars.last().map(|bar| bar.time);
    let fingerprint = candle_fingerprint(&bars);
    let history = json!({"bars":bars.len(),"first_ts":first,"last_ts":last,
        "coverage_days":history_coverage_days(first,last),"stored_coverage_days":metadata["coverage_days"],
        "read_limit":MAX_EVIDENCE_CANDLES,"read_limit_reached":read_limit_reached,
        "completed_only":true,"data_fingerprint":fingerprint,
        "freshness_at_run":if metadata["is_stale"] == true {"STALE"} else {"CURRENT"}});
    let source_metadata = json!({"id":source,"data_provider":provider,
        "is_proxy":is_research_proxy_source(&source),
        "shared_reference":shared_reference,
        "venue_native":is_native_candle_source_for_exchange(&source,Some(exchange)),
        "price_basis":metadata["price_basis"],"instrument_kind":metadata["instrument_kind"],"timezone":metadata["timezone"]});
    market
        .execute_batch("COMMIT")
        .map_err(|error| error.to_string())?;
    Ok((bars, source_metadata, history))
}

// Stable change detector for the exact used candles; this is not a security signature.
fn candle_fingerprint(bars: &[StrategyBar]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for bar in bars {
        for value in [
            bar.time as u64,
            bar.open.to_bits(),
            bar.high.to_bits(),
            bar.low.to_bits(),
            bar.close.to_bits(),
            bar.volume.to_bits(),
        ] {
            for byte in value.to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
    }
    format!("fnv1a64:{hash:016x}")
}

fn fee_metadata(instrument: &Value) -> (f64, f64, bool, Value) {
    let (buy, sell, known, label) = match quant_core::venue_fee_policy(
        text(instrument, "exchange_id"),
        text(instrument, "product_id"),
    ) {
        VenueFeePolicy::Known {
            buy_fee_pct,
            sell_fee_pct,
            fee_model,
            ..
        } => (buy_fee_pct, sell_fee_pct, true, fee_model),
        VenueFeePolicy::Unknown { fee_model, .. }
        | VenueFeePolicy::Unsupported { fee_model, .. } => (0.0, 0.0, false, fee_model),
    };
    (
        buy,
        sell,
        known,
        json!({"buy_fee_pct":known.then_some(buy),"sell_fee_pct":known.then_some(sell),
        "fees_known":known,"fee_model":label,"execution_costs_complete":false,
        "omissions":["Slippage and order-book depth","Market impact","Funding and borrow costs where applicable"]}),
    )
}

fn build_report(
    connection: &Connection,
    market_directory: &str,
    instrument: Value,
    timeframe: &str,
) -> Result<Value, String> {
    let model_catalog = matching_models(connection, &instrument, timeframe)?;
    let eligible = text(&instrument, "research_eligibility") == "eligible"
        && instrument.get("venue_verified").and_then(Value::as_bool) == Some(true);
    let supported = !model_catalog.is_empty();
    let (buy, sell, fees_known, costs) = fee_metadata(&instrument);
    let (bars, source, history) = if eligible && supported {
        read_history(market_directory, &instrument, timeframe)?
    } else {
        (
            Vec::new(),
            json!({"id":"unavailable","venue_native":false,"is_proxy":false}),
            empty_history(),
        )
    };
    let mut models = Vec::new();
    if eligible && supported {
        for kind in [EntryModelKind::Nwe, EntryModelKind::Dss] {
            let Some(model) = model_catalog
                .iter()
                .find(|model| text(model, "score_model_id") == kind.score_model_id())
            else {
                continue;
            };
            let backtest = quant_core::evaluate_entry_score_backtest(
                &bars,
                kind,
                buy,
                sell,
                fees_known,
                text(&instrument, "asset_category_id") == "crypto_perpetual"
                    || text(&instrument, "asset_category_id") == "crypto_futures",
            );
            let mut checks = backtest.get("checks").cloned().unwrap_or_else(|| json!({}));
            checks["registered_identity"] = json!(eligible);
            checks["venue_native_history"] =
                json!(source["venue_native"] == true && source["is_proxy"] != true);
            checks["fees_known"] = json!(fees_known);
            checks["execution_costs_complete"] = json!(false);
            let requirements = [
                ("registered_identity","Exact verified app/product/ticker identity is required."),
                ("holdout_sample_sufficient","The untouched holdout needs more completed entry outcomes."),
                ("holdout_span_sufficient","The actual evaluated holdout does not meet the minimum calendar-day coverage."),
                ("profitable_holdout","The selected exit has not demonstrated positive held-out expectancy and profit factor."),
                ("score_discrimination_supported","Score bands do not have enough separated held-out evidence to support score discrimination."),
                ("venue_native_history","Venue-native history is required; research proxies cannot establish execution evidence."),
                ("fees_known","The exact app/product fee policy is unknown."),
                ("execution_costs_complete","Slippage, depth, impact and applicable funding/borrow costs are not fully modeled."),
            ];
            let blocking_reasons: Vec<&str> = requirements
                .iter()
                .filter(|(key, _)| checks[*key] != true)
                .map(|(_, reason)| *reason)
                .collect();
            let validated = blocking_reasons.is_empty()
                && text(&backtest, "status") == "HOLDOUT_RESEARCH_COMPLETE";
            models.push(json!({"kind":match kind {EntryModelKind::Nwe=>"nwe",EntryModelKind::Dss=>"dss"},
                "label":model["short_label"],"signal_strategy_id":model["id"],"score_model_id":kind.score_model_id(),
                "backtest":backtest,"checks":checks,"entry_validation_status":if validated {"VALIDATED_HOLDOUT"} else {"VALIDATION_PENDING"},
                "blocking_reasons":blocking_reasons,"sizing_eligible":false}));
        }
    }
    let status = if !eligible {
        "PENDING_VERIFICATION"
    } else if !supported {
        "UNSUPPORTED_MODEL"
    } else if bars.is_empty() {
        "NO_HISTORY"
    } else if models
        .iter()
        .any(|model| model["backtest"]["status"] == "HOLDOUT_RESEARCH_COMPLETE")
    {
        "BACKTEST_COMPLETE"
    } else {
        "VALIDATION_PENDING"
    };
    Ok(
        json!({"schema_version":1,"protocol_id":PROTOCOL_ID,"instrument":instrument,"timeframe":timeframe,
        "computed_at":now_secs(),"status":status,"source":source,"history":history,"costs":costs,"models":models,
        "eligibility":{"status":if eligible {"eligible"} else {"pending_verification"},
            "reason":if !eligible {"A verified eligible registration is required before backtesting."}
                else if !supported {"The current four-indicator entry models are not registered for this exact product/category. No substitute model was backtested."}
                else {"Exact registered identity selected. Historical research does not imply execution validation."}},
        "sizing":{"status":"BLOCKED","reason":"Sizing is a later modeling step. Exact entry evidence, full execution costs, model/data identity and uncertainty must qualify first.",
            "required_inputs":["Validated per-ticker/model/timeframe/source evidence","Conditional held-out return and loss distributions","Sample confidence and downside uncertainty","Complete execution costs and liquidity assumptions","A separately reviewed sizing rule and risk limits"]}}),
    )
}

#[cfg(test)]
#[path = "entry_validation_tests.rs"]
mod tests;
