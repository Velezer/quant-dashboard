use quant_core::{
    calculate_nadaraya_watson_envelope, NadarayaWatsonEnvelopeConfig, DAY_TRADE_NWE_BANDWIDTH_BARS,
    DAY_TRADE_NWE_ENVELOPE_MULTIPLIER, DAY_TRADE_NWE_WINDOW_BARS,
};

#[test]
fn defaults_match_the_supplied_luxalgo_parameters() {
    let config = NadarayaWatsonEnvelopeConfig::default();
    assert_eq!(config.window_bars, DAY_TRADE_NWE_WINDOW_BARS);
    assert_eq!(config.bandwidth_bars, DAY_TRADE_NWE_BANDWIDTH_BARS);
    assert_eq!(
        config.envelope_multiplier,
        DAY_TRADE_NWE_ENVELOPE_MULTIPLIER
    );
    assert_eq!(config.window_bars, 500);
    assert_eq!(config.bandwidth_bars, 8.0);
    assert_eq!(config.envelope_multiplier, 3.0);
    assert!(config.metadata().repainting_default);
}

#[test]
fn endpoint_mode_starts_bands_after_one_full_causal_window() {
    let config = NadarayaWatsonEnvelopeConfig {
        window_bars: 4,
        bandwidth_bars: 2.0,
        envelope_multiplier: 3.0,
    };
    let closes = vec![100.0; 6];
    let result = calculate_nadaraya_watson_envelope(&closes, &config);

    assert!(result.endpoint.upper[..3]
        .iter()
        .all(|value| value.is_nan()));
    assert!(result.endpoint.lower[..3]
        .iter()
        .all(|value| value.is_nan()));
    assert!((result.endpoint.midline[3] - 100.0).abs() < 1e-10);
    assert!((result.endpoint.upper[3] - 100.0).abs() < 1e-10);
    assert!((result.endpoint.lower[3] - 100.0).abs() < 1e-10);
    assert!(result.repainting.upper[..2]
        .iter()
        .all(|value| value.is_nan()));
    assert_eq!(result.repainting.upper[2], 100.0);
}

#[test]
fn repainting_uses_later_bars_but_endpoint_values_do_not() {
    let closes: Vec<f64> = (0..17)
        .map(|index| 100.0 + (index as f64 * 0.43).sin() * 3.0)
        .collect();
    let mut changed_future = closes.clone();
    changed_future[16] += 100.0;
    let config = NadarayaWatsonEnvelopeConfig {
        window_bars: 8,
        bandwidth_bars: 2.0,
        envelope_multiplier: 3.0,
    };
    let original = calculate_nadaraya_watson_envelope(&closes, &config);
    let changed = calculate_nadaraya_watson_envelope(&changed_future, &config);

    assert_ne!(
        original.repainting.midline[14],
        changed.repainting.midline[14]
    );
    assert_eq!(original.endpoint.midline[14], changed.endpoint.midline[14]);
    assert_eq!(original.endpoint.upper[14], changed.endpoint.upper[14]);
}

#[test]
fn both_modes_mark_upper_and_lower_crossings() {
    let config = NadarayaWatsonEnvelopeConfig {
        window_bars: 4,
        bandwidth_bars: 2.0,
        envelope_multiplier: 1.0,
    };
    let closes = vec![100.0, 100.0, 100.0, 100.0, 100.0, 100.0, 200.0];
    let result = calculate_nadaraya_watson_envelope(&closes, &config);

    assert!(result.endpoint.crossed_above_upper[6]);
    assert!(result.repainting.crossed_above_upper[6]);
    assert!(!result.endpoint.crossed_below_lower[6]);
    assert!(!result.repainting.crossed_below_lower[6]);
}

#[test]
fn invalid_configuration_uses_bounded_defaults() {
    let invalid_config = NadarayaWatsonEnvelopeConfig {
        window_bars: 0,
        bandwidth_bars: f64::NAN,
        envelope_multiplier: f64::INFINITY,
    };
    let defaults = NadarayaWatsonEnvelopeConfig::default();
    assert_eq!(invalid_config.validated(), defaults);

    let over_limit = NadarayaWatsonEnvelopeConfig {
        window_bars: usize::MAX,
        ..defaults
    };
    assert_eq!(
        over_limit.validated().window_bars,
        DAY_TRADE_NWE_WINDOW_BARS
    );
}
