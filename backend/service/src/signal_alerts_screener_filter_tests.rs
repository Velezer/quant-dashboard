use super::listing::{score_basis_matches_indicator_filter, score_indicator_name};
use serde_json::json;

#[test]
fn the_server_accepts_all_six_registered_indicator_filters() {
    for (filter, label) in [
        ("nwe", "Nadaraya-Watson Envelope"),
        ("dss_bressert", "DSS Bressert"),
        ("relative_volume", "Relative Volume"),
        ("delta_proxy", "Estimated Volume Delta (OHLCV proxy)"),
        ("markov_regime", "Markov Regime"),
        (
            "lag_one_return_autocorrelation",
            "Lag-one Return Autocorrelation",
        ),
    ] {
        assert_eq!(score_indicator_name(filter), Some(label));
    }
}

#[test]
fn markov_and_autocorrelation_filter_signed_scores_by_magnitude() {
    let score_basis = json!({"components": [
        {"indicator":"Markov Regime", "state":"BEARISH", "points":-10.0},
        {"indicator":"Lag-one Return Autocorrelation", "state":"MEAN_REVERSION", "points":-7.5}
    ]});
    assert!(score_basis_matches_indicator_filter(
        &score_basis,
        Some("markov_regime"),
        Some(9.0)
    ));
    assert!(score_basis_matches_indicator_filter(
        &score_basis,
        Some("lag_one_return_autocorrelation"),
        Some(7.0)
    ));
    assert!(!score_basis_matches_indicator_filter(
        &score_basis,
        Some("lag_one_return_autocorrelation"),
        Some(8.0)
    ));
}

#[test]
fn display_labels_and_stable_component_ids_match_the_same_filter() {
    let by_label = json!({"components": [
        {"indicator":"DSS Bressert", "state":"SIGNED_SCORE", "points":5.0}
    ]});
    let by_id = json!({"components": [
        {"indicator_id":"lag_one_return_autocorrelation", "state":"CONTINUATION", "points":-6.0}
    ]});
    let by_key = json!({"components": [
        {"key":"markov_regime", "state":"BEARISH", "points":-10.0}
    ]});
    assert!(score_basis_matches_indicator_filter(&by_label, Some("dss_bressert"), Some(5.0)));
    assert!(score_basis_matches_indicator_filter(
        &by_id,
        Some("lag_one_return_autocorrelation"),
        Some(6.0)
    ));
    assert!(score_basis_matches_indicator_filter(
        &by_key,
        Some("markov_regime"),
        Some(10.0)
    ));
}

#[test]
fn unavailable_or_legacy_components_do_not_match_indicator_filters() {
    let unavailable = json!({"components": [
        {"indicator":"Markov Regime", "state":"UNAVAILABLE", "points":0.0}
    ]});
    let legacy = json!({"score_model_status":"legacy", "components": [
        {"indicator":"Markov Regime", "state":"BEARISH", "points":-10.0}
    ]});
    assert!(!score_basis_matches_indicator_filter(
        &unavailable,
        Some("markov_regime"),
        Some(0.0)
    ));
    assert!(!score_basis_matches_indicator_filter(
        &legacy,
        Some("markov_regime"),
        Some(0.0)
    ));
}
