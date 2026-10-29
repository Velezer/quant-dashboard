use serde_json::Value;

pub(super) fn ensure_stockbit_signal_catalog(catalog: &mut Value) -> bool {
    let minimum_nwe_history = quant_core::stockbit_nwe_4h_minimum_history_bars();
    let minimum_dss_history = quant_core::DSS_BRESSERT_MINIMUM_HISTORY_BARS;
    let minimum_score = quant_core::FOUR_INDICATOR_MINIMUM_SCORE_OUT_OF_40;
    let nwe_band_distance_ratio =
        quant_core::STOCKBIT_NWE_4H_SIGNAL_CONFIG.maximum_band_distance_ratio;
    let maximum_score = quant_core::FOUR_INDICATOR_SCORE_SCALE_MAX;
    let component_maximum_score = quant_core::FOUR_INDICATOR_GROUP_SCORE_SCALE_MAX;
    let indicator_component_maximum_score = quant_core::FOUR_INDICATOR_COMPONENT_SCORE_SCALE_MAX;
    let relative_volume_lookback = quant_core::FOUR_INDICATOR_VOLUME_LOOKBACK_BARS;
    let indicator = serde_json::json!({
        "id": quant_core::NWE_METHOD_ID,
        "label": quant_core::NWE_METHOD_LABEL,
        "kind": "indicator",
        "signal_strategy": false,
    });
    let mut changed = super::upsert_catalog_record(catalog, "indicators", indicator);
    changed |= super::upsert_catalog_record(
        catalog,
        "indicators",
        serde_json::json!({
            "id": "estimated_volume_delta_ohlcv",
            "label": "Estimated Volume Delta (OHLCV proxy)",
            "kind": "indicator",
            "signal_strategy": false,
        }),
    );

    for timeframe in quant_core::SIGNAL_SCREENER_TIMEFRAMES.iter().copied() {
        let timeframe_source_note = if timeframe == "30m" {
            " 30m setup candles are aggregated locally from two adjacent, completed same-source 15m candles; no separate 30m provider request is made."
        } else {
            ""
        };
        let (Some(nwe_strategy_id), Some(dss_strategy_id)) = (
            quant_core::stockbit_nwe_signal_strategy_id(timeframe),
            quant_core::stockbit_dss_bressert_signal_strategy_id(timeframe),
        ) else {
            continue;
        };
        let nwe_direction_rule = if timeframe == quant_core::STOCKBIT_NWE_4H_TIMEFRAME {
            format!(
                "A lower-band cross or near-band state on a completed 4h candle is LONG; an upper-band cross or near-band state is SHORT."
            )
        } else {
            format!(
                "A lower-band cross or near-band state on a completed {timeframe} bar is LONG; an upper-band equivalent is SHORT."
            )
        };
        let nwe_setup_description = format!(
            "{nwe_direction_rule} Without a cross, a close within {:.0}% of the full envelope width from its nearest band qualifies on either side.",
            nwe_band_distance_ratio * 100.0,
        );
        let nwe_basis = four_indicator_score_basis(
            &nwe_setup_description,
            timeframe,
            relative_volume_lookback,
            minimum_score,
            maximum_score,
            component_maximum_score,
            indicator_component_maximum_score,
        );
        let nwe_description = format!(
            "Stockbit IDX research market-entry signal on the latest {timeframe} bar, shown only when that bar qualifies and a fresh same-source 15m price is available. A forming-bar score is provisional and can change before close. {nwe_basis} Require at least {minimum_nwe_history} {timeframe} bars.{timeframe_source_note} The score measures setup strength, not win probability or execution availability; the 15m price is a candle close, not a live bid/ask. Direction is informational; IDX plans and execution remain long-only. Research only, unvalidated; proxy candles are not venue-native execution evidence. No order is placed."
        );
        let nwe_strategy = serde_json::json!({
            "id": nwe_strategy_id,
            "label": format!("Stockbit {timeframe} Nadaraya-Watson Band Proximity · Research"),
            "short_label": format!("{timeframe} NWE Band Proximity"),
            "icon": "📈",
            "description": nwe_description,
            "status": "research_only",
            "validation_status": "UNVALIDATED",
            "research_only": true,
            "implemented": true,
            "indicator_ids": [quant_core::NWE_METHOD_ID, "dss_bressert", "relative_volume", "estimated_volume_delta_ohlcv"],
            "asset_category_ids": [quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID],
            "exchange_ids": [quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID],
            "product_ids": [quant_core::STOCKBIT_NWE_4H_PRODUCT_ID],
            "timeframe": timeframe,
            "score_model_id": quant_core::STOCKBIT_NWE_4H_SIGNAL_SCORE_MODEL_ID,
            "score_scale_max": maximum_score,
            "minimum_score_out_of_40": minimum_score,
            "component_score_scale_max": component_maximum_score,
            "indicator_component_score_scale_max": indicator_component_maximum_score,
            "strategy_score_label": "Price evidence (NWE + DSS)",
            "confluence_score_label": "Volume evidence (relative volume + delta proxy)",
            "minimum_history_bars": minimum_nwe_history,
            "score_basis_description": nwe_description,
        });
        changed |= super::upsert_catalog_record(catalog, "signal_strategies", nwe_strategy);

        let dss_setup_description = format!(
            "On a completed {timeframe} bar, a bullish DSS Bressert cross or approaching near-cross is LONG; a bearish cross or approaching near-cross is SHORT. A near-cross requires the DSS-to-trigger spread to approach zero within {:.1} points.",
            quant_core::DSS_BRESSERT_NEAR_SPREAD_THRESHOLD,
        );
        let dss_basis = four_indicator_score_basis(
            &dss_setup_description,
            timeframe,
            relative_volume_lookback,
            minimum_score,
            maximum_score,
            component_maximum_score,
            indicator_component_maximum_score,
        );
        let dss_description = format!(
            "Stockbit IDX research market-entry signal on the latest {timeframe} bar, shown only when that bar qualifies and a fresh same-source 15m price is available. A forming-bar score is provisional and can change before close. {dss_basis} Require at least {minimum_dss_history} {timeframe} bars.{timeframe_source_note} The score measures setup strength, not win probability or execution availability; the 15m price is a candle close, not a live bid/ask. Direction is informational; IDX plans and execution remain long-only. Research only, unvalidated; proxy candles are not venue-native execution evidence. No order is placed."
        );
        let dss_strategy = serde_json::json!({
            "id": dss_strategy_id,
            "label": format!("Stockbit {timeframe} DSS Bressert Near-Cross · Research"),
            "short_label": format!("{timeframe} DSS Near-Cross"),
            "icon": "📈",
            "description": dss_description,
            "status": "research_only",
            "validation_status": "UNVALIDATED",
            "research_only": true,
            "implemented": true,
            "indicator_ids": ["dss_bressert", quant_core::NWE_METHOD_ID, "relative_volume", "estimated_volume_delta_ohlcv"],
            "asset_category_ids": [quant_core::STOCKBIT_NWE_4H_ASSET_CATEGORY_ID],
            "exchange_ids": [quant_core::STOCKBIT_NWE_4H_EXCHANGE_ID],
            "product_ids": [quant_core::STOCKBIT_NWE_4H_PRODUCT_ID],
            "timeframe": timeframe,
            "score_model_id": quant_core::STOCKBIT_DSS_BRESSERT_4H_SIGNAL_SCORE_MODEL_ID,
            "score_scale_max": maximum_score,
            "minimum_score_out_of_40": minimum_score,
            "component_score_scale_max": component_maximum_score,
            "indicator_component_score_scale_max": indicator_component_maximum_score,
            "strategy_score_label": "Price evidence (NWE + DSS)",
            "confluence_score_label": "Volume evidence (relative volume + delta proxy)",
            "minimum_history_bars": minimum_dss_history,
            "score_basis_description": dss_description,
        });
        changed |= super::upsert_catalog_record(catalog, "signal_strategies", dss_strategy);
    }

    changed |= super::retire_stockbit_nwe_4h_legacy_strategies(catalog);
    changed
}

fn four_indicator_score_basis(
    setup_description: &str,
    timeframe: &str,
    relative_volume_lookback: usize,
    minimum_score: f64,
    maximum_score: f64,
    component_maximum_score: f64,
    indicator_component_maximum_score: f64,
) -> String {
    format!(
        "{setup_description} The current entry setup score has four equal-weight factors: NWE, DSS Bressert, {relative_volume_lookback}-bar relative volume, and estimated OHLCV volume delta. Each factor contributes 0–{indicator_component_maximum_score:.0}; the two price factors and two volume factors each form a group capped at {component_maximum_score:.0}, for {maximum_score:.0} total. Relative volume compares the current bar against the preceding {relative_volume_lookback} same-source {timeframe} bars and awards points only when candle direction agrees with the signal. The delta proxy uses candle close location; it is not true aggressor-side trade delta. Missing/zero volume or insufficient history is labeled unavailable and contributes zero. Scores below {minimum_score:.1}/{maximum_score:.0} are withheld. The score measures setup strength, not win probability or execution availability."
    )
}
