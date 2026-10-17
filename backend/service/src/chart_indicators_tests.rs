use super::{candles::new_candle, ChartIndicatorSettings};
use quant_core::NadarayaWatsonEnvelopeConfig;

#[test]
fn chart_candles_serialize_studies_under_one_typed_field() {
    let mut candles: Vec<_> = (0..80)
        .map(|index| {
            let close = 100.0 + index as f64;
            new_candle(index, close, close + 1.0, close - 1.0, close, 1_000.0)
        })
        .collect();
    ChartIndicatorSettings::default().attach(&mut candles);

    let candle = serde_json::to_value(candles.last().unwrap()).unwrap();
    let indicators = &candle["indicators"];
    let envelope = &indicators["nadaraya_watson_envelope"];
    assert!(envelope["repainting"]["upper"].is_number());
    assert!(envelope["repainting"]["lower"].is_number());
    assert!(envelope["endpoint"].as_object().unwrap().is_empty());
}

#[test]
fn chart_indicator_metadata_comes_from_the_default_configs() {
    let metadata = serde_json::to_value(ChartIndicatorSettings::default().metadata()).unwrap();
    assert_eq!(
        metadata["nadaraya_watson_envelope"],
        serde_json::to_value(NadarayaWatsonEnvelopeConfig::default().metadata()).unwrap()
    );
}
