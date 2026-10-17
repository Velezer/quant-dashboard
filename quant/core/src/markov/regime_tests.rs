use super::{
    build_payload, classify_regimes, contiguous_regime_sequences, walk_forward_evaluation,
    MarkovAsset, REGIME_COUNT,
};
use crate::{MarkovRegimeConfig, RegimeId, StrategyBar};

fn bar(index: usize, close: f64) -> StrategyBar {
    StrategyBar {
        time: (1_700_000_000 + index as i64 * 86_400) * 1_000_000_000,
        open: close,
        high: close + 1.0,
        low: close - 1.0,
        close,
        volume: 1_000.0,
    }
}

#[test]
fn short_history_remains_unclassified_during_warmup() {
    let bars: Vec<_> = (0..20).map(|i| bar(i, 100.0)).collect();
    assert!(classify_regimes(&bars, &MarkovRegimeConfig::default())
        .iter()
        .all(Option::is_none));
}

#[test]
fn payload_describes_causal_dss_nwe_markov_model() {
    let bars: Vec<_> = (0..560)
        .map(|i| bar(i, 100.0 + i as f64 * 0.1 + (i as f64 * 0.17).sin() * 2.0))
        .collect();
    let payload = build_payload(
        &[MarkovAsset {
            storage_symbol: "BTC-USD".to_string(),
            exchange_id: "tokocrypto".to_string(),
            asset_category_id: "crypto_spot".to_string(),
            bars,
        }],
        &MarkovRegimeConfig::default(),
    );
    assert_eq!(payload["regimes"].as_array().unwrap().len(), REGIME_COUNT);
    assert!(payload["global_model"]["transition_matrix"].is_array());
    assert_eq!(payload["assets"].as_array().unwrap().len(), 1);
    assert_eq!(
        payload["model_type"],
        "observed_state_discrete_time_markov_chain"
    );
    assert_eq!(payload["classification_mode"], "causal_completed_bars");
    assert!(payload["classification_inputs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "Nadaraya-Watson Envelope endpoint"));
    assert!(payload["probability_note"]
        .as_str()
        .unwrap()
        .contains("not validated forecasts"));
    assert_eq!(payload["timeframe"], "1d");
    assert!(payload["assets"][0]["warmup_bars"].as_u64().unwrap() > 0);
    assert!(
        payload["ticker_models"]["tokocrypto:crypto_spot:BTC-USD"]["transition_row_support"]
            .is_array()
    );
}

#[test]
fn registered_exchange_and_category_keep_same_symbol_models_separate() {
    let bars: Vec<_> = (0..560)
        .map(|i| bar(i, 100.0 + i as f64 * 0.1 + (i as f64 * 0.17).sin() * 2.0))
        .collect();
    let payload = build_payload(
        &[
            MarkovAsset {
                storage_symbol: "SAME".to_string(),
                exchange_id: "tokocrypto".to_string(),
                asset_category_id: "crypto_spot".to_string(),
                bars: bars.clone(),
            },
            MarkovAsset {
                storage_symbol: "SAME".to_string(),
                exchange_id: "tokocrypto".to_string(),
                asset_category_id: "tokenized_stock".to_string(),
                bars,
            },
        ],
        &MarkovRegimeConfig::default(),
    );

    assert_eq!(payload["assets"].as_array().unwrap().len(), 2);
    assert!(payload["universe_models"]["crypto_spot"].is_object());
    assert!(payload["universe_models"]["tokenized_stock"].is_object());
    assert!(payload["ticker_models"]["tokocrypto:crypto_spot:SAME"].is_object());
    assert!(payload["ticker_models"]["tokocrypto:tokenized_stock:SAME"].is_object());
}

#[test]
fn nwe_warmup_is_unclassified_and_does_not_create_chop_transitions() {
    let bars: Vec<_> = (0..540)
        .map(|i| bar(i, 100.0 + (i as f64 * 0.1).sin()))
        .collect();
    let observations = classify_regimes(&bars, &MarkovRegimeConfig::default());
    assert!(observations[..499].iter().all(Option::is_none));
    assert!(observations[520..].iter().all(Option::is_some));
    let sequences = contiguous_regime_sequences(&observations);
    assert_eq!(sequences.len(), 1);
    assert!(sequences[0]
        .iter()
        .all(|state| (0..REGIME_COUNT).contains(state)));
    assert_eq!(sequences[0].len(), 41);
}

#[test]
fn invalid_observation_splits_transition_sequence() {
    let observations = [
        Some(RegimeId::BullTrend),
        Some(RegimeId::BearTrend),
        None,
        Some(RegimeId::Accumulation),
        Some(RegimeId::Distribution),
    ];
    assert_eq!(
        contiguous_regime_sequences(&observations),
        vec![vec![0, 1], vec![2, 3]]
    );
}

#[test]
fn walk_forward_uses_only_prior_observations_and_beats_persistence_on_alternating_sequence() {
    let sequence: Vec<usize> = (0..80).map(|index| index % 2).collect();
    let evaluation = walk_forward_evaluation(&[sequence]);
    assert_eq!(evaluation["status"], "evaluated");
    assert_eq!(evaluation["evaluated_transitions"], 49);
    assert_eq!(evaluation["accuracy_pct"], 100.0);
    assert_eq!(
        evaluation["stay_in_current_state_baseline_accuracy_pct"],
        0.0
    );
}

#[test]
fn walk_forward_discloses_insufficient_training_history() {
    let evaluation = walk_forward_evaluation(&[vec![0, 1, 0, 1]]);
    assert_eq!(evaluation["status"], "insufficient_history");
    assert_eq!(evaluation["evaluated_transitions"], 0);
}

#[test]
fn appending_future_bars_does_not_repaint_existing_regime_labels() {
    let bars: Vec<_> = (0..620)
        .map(|i| bar(i, 100.0 + (i as f64 * 0.13).sin() * 4.0 + i as f64 * 0.02))
        .collect();
    let original = classify_regimes(&bars[..560], &MarkovRegimeConfig::default());
    let extended = classify_regimes(&bars, &MarkovRegimeConfig::default());
    assert_eq!(original, extended[..560]);
}

#[test]
fn classifier_configuration_controls_nwe_warmup_and_ema_context() {
    let bars: Vec<_> = (0..80)
        .map(|i| bar(i, 100.0 + i as f64 * 0.1 + (i as f64 * 0.17).sin()))
        .collect();
    let default_config = MarkovRegimeConfig::default();
    let default_states = classify_regimes(&bars, &default_config);
    assert!(default_states.iter().all(Option::is_none));

    let short_nwe_config = MarkovRegimeConfig {
        nwe: crate::NadarayaWatsonEnvelopeConfig {
            window_bars: 32,
            ..default_config.nwe
        },
        trend_ema_span_bars: 12,
        ..default_config
    };
    let short_states = classify_regimes(&bars, &short_nwe_config);
    assert!(short_states[..31].iter().all(Option::is_none));
    assert!(short_states[31..].iter().all(Option::is_some));
}

#[test]
fn invalid_classifier_threshold_ranges_fall_back_to_valid_defaults() {
    let config = MarkovRegimeConfig {
        dss_oversold_max: 90.0,
        dss_overbought_min: 20.0,
        dss_bull_pullback_min: 80.0,
        dss_bull_pullback_max: 10.0,
        trend_ema_span_bars: 0,
        ..MarkovRegimeConfig::default()
    }
    .validated();
    let defaults = MarkovRegimeConfig::default();
    assert_eq!(config.dss_oversold_max, defaults.dss_oversold_max);
    assert_eq!(config.dss_overbought_min, defaults.dss_overbought_min);
    assert_eq!(config.dss_bull_pullback_min, defaults.dss_bull_pullback_min);
    assert_eq!(config.dss_bull_pullback_max, defaults.dss_bull_pullback_max);
    assert_eq!(config.trend_ema_span_bars, defaults.trend_ema_span_bars);
}
