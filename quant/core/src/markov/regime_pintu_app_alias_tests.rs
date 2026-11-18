use super::{build_payload_for_timeframe, MarkovAsset, MarkovRegimeConfig};
use crate::StrategyBar;

#[test]
fn pintu_app_identity_is_classified_as_crypto_futures() {
    let bars = (0_i64..600)
        .map(|index| {
            let close = 100.0 + index as f64 * 0.1 + (index as f64 * 0.17).sin() * 2.0;
            StrategyBar {
                time: (1_700_000_000 + index * 900) * 1_000_000_000,
                open: close,
                high: close + 1.0,
                low: close - 1.0,
                close,
                volume: 1_000.0,
            }
        })
        .collect();
    let payload = build_payload_for_timeframe(
        &[MarkovAsset {
            storage_symbol: "BTC-USD".into(),
            exchange_id: "pintu".into(),
            asset_category_id: "crypto_perpetual".into(),
            bars,
        }],
        &MarkovRegimeConfig::default(),
        "15m",
    );

    assert_eq!(payload["assets"].as_array().unwrap().len(), 1);
    assert_eq!(payload["assets"][0]["exchange_id"], "pintu");
    assert_eq!(payload["assets"][0]["class"], "crypto_futures");
    assert_eq!(
        payload["ticker_models"]["pintu:crypto_perpetual:BTC-USD"]["model_key"],
        "pintu:crypto_perpetual:BTC-USD"
    );
}
