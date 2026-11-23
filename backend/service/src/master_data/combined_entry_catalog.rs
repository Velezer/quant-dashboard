use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

pub(crate) fn combined_entry_strategy_id(timeframe: &str, is_stockbit: bool) -> Option<String> {
    quant_core::SIGNAL_SCREENER_TIMEFRAMES
        .contains(&timeframe)
        .then(|| {
            let market = if is_stockbit { "stockbit" } else { "crypto" };
            format!("{market}_combined_entry_{timeframe}_v1")
        })
}

pub(crate) fn pluang_commodities_combined_entry_strategy_id(timeframe: &str) -> Option<String> {
    (timeframe == "15m").then(|| format!("pluang_commodities_combined_entry_{timeframe}_v1"))
}

fn retired_entry_replacements() -> &'static HashMap<String, String> {
    static REPLACEMENTS: OnceLock<HashMap<String, String>> = OnceLock::new();
    REPLACEMENTS.get_or_init(|| {
        let mut replacements = HashMap::new();
        for timeframe in quant_core::SIGNAL_SCREENER_TIMEFRAMES {
            for (stockbit, legacy_ids) in [
                (
                    true,
                    [
                        quant_core::stockbit_nwe_signal_strategy_id(timeframe),
                        quant_core::stockbit_dss_bressert_signal_strategy_id(timeframe),
                    ],
                ),
                (
                    false,
                    [
                        quant_core::crypto_nwe_signal_strategy_id(timeframe),
                        quant_core::crypto_dss_bressert_signal_strategy_id(timeframe),
                    ],
                ),
            ] {
                if let Some(replacement) = combined_entry_strategy_id(timeframe, stockbit) {
                    for legacy_id in legacy_ids.into_iter().flatten() {
                        replacements.insert(legacy_id, replacement.clone());
                    }
                }
            }
        }
        replacements
    })
}

pub(crate) fn is_retired_entry_strategy(id: &str) -> bool {
    retired_entry_replacements().contains_key(id)
}

pub(super) fn ensure_combined_entry_catalog(catalog: &mut Value) -> bool {
    let crypto_products = crate::instrument_registry::REGISTERED_APP_PRODUCT_CATEGORIES
        .into_iter()
        .filter(|(exchange, _, category)| {
            category.starts_with("crypto_")
                && catalog["exchanges"].as_array().is_some_and(|exchanges| {
                    exchanges.iter().any(|record| {
                        record["id"] == *exchange
                            && super::catalog_array_contains(record, "asset_category_ids", category)
                    })
                })
        })
        .collect::<Vec<_>>();
    let categories = crypto_products
        .iter()
        .map(|(_, _, category)| (*category).to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let exchanges = crypto_products
        .iter()
        .map(|(exchange, _, _)| (*exchange).to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let products = crypto_products
        .iter()
        .map(|(_, product, _)| (*product).to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut changed = false;
    for (id, label) in [
        (quant_core::NWE_METHOD_ID, quant_core::NWE_METHOD_LABEL),
        (
            "estimated_volume_delta_ohlcv",
            "Estimated Volume Delta (OHLCV proxy)",
        ),
        ("markov_regime", "Markov Regime"),
        (
            "lag_one_return_autocorrelation",
            "Lag-one Return Autocorrelation",
        ),
    ] {
        changed |= super::upsert_catalog_record(
            catalog,
            "indicators",
            json!({
                "id": id, "label": label, "kind": "indicator", "signal_strategy": false,
            }),
        );
    }
    for timeframe in quant_core::SIGNAL_SCREENER_TIMEFRAMES.iter().copied() {
        for is_stockbit in [true, false] {
            let Some(strategy_id) = combined_entry_strategy_id(timeframe, is_stockbit) else {
                continue;
            };
            let market_label = if is_stockbit { "Stockbit" } else { "Crypto" };
            let (market_categories, market_exchanges, market_products) = if is_stockbit {
                (
                    vec![quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID.to_string()],
                    vec![quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID.to_string()],
                    vec![quant_core::STOCKBIT_NWE_4H_PRODUCT_ID.to_string()],
                )
            } else {
                (categories.clone(), exchanges.clone(), products.clone())
            };
            changed |= upsert_combined_entry_model(
                catalog,
                strategy_id,
                market_label,
                timeframe,
                market_categories,
                market_exchanges,
                market_products,
            );
        }
    }
    if let Some(strategy_id) = pluang_commodities_combined_entry_strategy_id("15m") {
        changed |= upsert_combined_entry_model(
            catalog,
            strategy_id,
            "Pluang Commodities",
            "15m",
            vec![crate::instrument_registry::PLUANG_COMMODITIES_ASSET_CATEGORY_ID.to_string()],
            vec!["pluang".to_string()],
            vec![crate::instrument_registry::PLUANG_COMMODITIES_PRODUCT_ID.to_string()],
        );
    }
    if let Some(strategies) = catalog
        .get_mut("signal_strategies")
        .and_then(Value::as_array_mut)
    {
        for strategy in strategies {
            let Some(replacement) = strategy["id"]
                .as_str()
                .and_then(|id| retired_entry_replacements().get(id))
            else {
                continue;
            };
            changed |= strategy["status"] != "retired"
                || strategy["validation_status"] != "RETIRED"
                || strategy["implemented"] != false
                || strategy["replaced_by"].as_str() != Some(replacement);
            strategy["status"] = json!("retired");
            strategy["validation_status"] = json!("RETIRED");
            strategy["implemented"] = json!(false);
            strategy["replaced_by"] = json!(replacement);
        }
    }
    changed
}

fn upsert_combined_entry_model(
    catalog: &mut Value,
    strategy_id: String,
    market_label: &str,
    timeframe: &str,
    market_categories: Vec<String>,
    market_exchanges: Vec<String>,
    market_products: Vec<String>,
) -> bool {
    let description = score_description(timeframe);
    super::upsert_catalog_record(
        catalog,
        "signal_strategies",
        json!({
            "id": strategy_id,
            "label": format!("{market_label} {timeframe} Combined Score · Research"),
            "short_label": format!("{timeframe} Combined Score"),
            "icon": "📈",
            "description": description,
            "status": "research_only",
            "validation_status": "UNVALIDATED",
            "research_only": true,
            "implemented": true,
            "indicator_ids": [quant_core::NWE_METHOD_ID, "dss_bressert", "relative_volume",
                "estimated_volume_delta_ohlcv", "markov_regime", "lag_one_return_autocorrelation"],
            "asset_category_ids": market_categories,
            "exchange_ids": market_exchanges,
            "product_ids": market_products,
            "timeframe": timeframe,
            "score_model_id": quant_core::ENTRY_SCORE_MODEL_ID,
            "score_scale_max": quant_core::SIX_INDICATOR_SCORE_SCALE_MAX,
            "minimum_score_out_of_60": quant_core::SIX_INDICATOR_MINIMUM_SCORE_OUT_OF_60,
            "component_score_scale_max": quant_core::FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX,
            "indicator_component_score_scale_max": quant_core::FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX,
            "strategy_score_label": "Weighted price contribution (NWE + DSS)",
            "confluence_score_label": "Weighted volume contribution (relative volume + delta proxy)",
            "minimum_history_bars": quant_core::ENTRY_SCORE_MINIMUM_HISTORY_BARS,
            "score_basis_description": description,
        }),
    )
}

fn score_description(timeframe: &str) -> String {
    let minimum_score = quant_core::SIX_INDICATOR_MINIMUM_SCORE_OUT_OF_60;
    let maximum_score = quant_core::SIX_INDICATOR_SCORE_SCALE_MAX;
    let minimum_history = quant_core::ENTRY_SCORE_MINIMUM_HISTORY_BARS;
    format!(
        "A {timeframe} candle supplies six signed readings from −10 to +10: NWE, DSS Bressert, relative volume, estimated OHLCV volume delta, Markov regime, and lag-one return autocorrelation. Unavailable readings contribute zero. Their weighted sum ranges from −{maximum_score:.0} to +{maximum_score:.0}; positive is LONG, negative is SHORT, and magnitudes below {minimum_score:.1} are neutral. The model needs at least {minimum_history} candles. The live screener can mark a forming candle’s score as provisional. Backtests use completed bars. The first four factors have equal weights until a validated after-fee profile supplies learned weights. Markov and autocorrelation each use a fixed 10-point weight. The screener shows each reading, weight, and weighted contribution. Same-source reference prices and research proxy labels remain visible. After-fee win rates are historical measurements."
    )
}
