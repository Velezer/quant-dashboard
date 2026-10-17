use super::{
    calculate_nadaraya_watson_endpoint_only, calculate_nadaraya_watson_envelope,
    NadarayaWatsonEnvelopeConfig,
};

#[test]
fn endpoint_only_matches_the_endpoint_from_the_full_envelope() {
    let closes = (0..160)
        .map(|index| 100.0 + (index as f64 * 0.37).sin() * 4.0 + index as f64 * 0.03)
        .collect::<Vec<_>>();
    let config = NadarayaWatsonEnvelopeConfig {
        window_bars: 48,
        bandwidth_bars: 6.0,
        envelope_multiplier: 2.25,
    };

    let full = calculate_nadaraya_watson_envelope(&closes, &config).endpoint;
    let endpoint_only = calculate_nadaraya_watson_endpoint_only(&closes, &config);

    assert_same_floats(&full.midline, &endpoint_only.midline);
    assert_same_floats(&full.upper, &endpoint_only.upper);
    assert_same_floats(&full.lower, &endpoint_only.lower);
    assert_eq!(full.crossed_above_upper, endpoint_only.crossed_above_upper);
    assert_eq!(full.crossed_below_lower, endpoint_only.crossed_below_lower);
}

#[test]
fn endpoint_bands_warm_up_with_the_first_full_causal_midline_window() {
    let config = NadarayaWatsonEnvelopeConfig {
        window_bars: 10,
        bandwidth_bars: 3.0,
        envelope_multiplier: 2.0,
    };
    let closes = (0..10)
        .map(|index| 100.0 + (index as f64 * 0.7).sin())
        .collect::<Vec<_>>();

    assert_eq!(config.minimum_endpoint_envelope_bars(), 10);
    let endpoint = calculate_nadaraya_watson_endpoint_only(&closes, &config);
    assert!(endpoint.midline[..9].iter().all(|value| value.is_nan()));
    assert!(endpoint.upper[..9].iter().all(|value| value.is_nan()));
    assert!(endpoint.lower[..9].iter().all(|value| value.is_nan()));
    assert!(endpoint.midline[9].is_finite());
    assert!(endpoint.upper[9].is_finite());
    assert!(endpoint.lower[9].is_finite());
}

fn assert_same_floats(expected: &[f64], actual: &[f64]) {
    assert_eq!(expected.len(), actual.len());
    for (expected, actual) in expected.iter().zip(actual) {
        if expected.is_nan() {
            assert!(actual.is_nan());
        } else {
            assert_eq!(expected, actual);
        }
    }
}
