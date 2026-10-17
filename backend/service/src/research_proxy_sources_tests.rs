use super::super::{persist, Bar};
use super::{
    bybit_kline_url, parse_bybit_candles, parse_coinbase_candles, parse_okx_candles, MarketKind,
    ProviderKind, ResearchAdapter, ADAPTERS, BYBIT_MAINNET_MARKET_API_BASE_URLS, MAX_BYBIT_ROWS,
    MAX_COINBASE_ROWS, MAX_OKX_ROWS,
};
use serde_json::json;

#[test]
fn reference_adapters_map_only_supported_crypto_products() {
    let spot = ADAPTERS
        .iter()
        .copied()
        .filter(|adapter| adapter.supports("crypto_spot"))
        .collect::<Vec<_>>();
    assert_eq!(spot.len(), 3);
    assert!(spot.iter().any(|adapter| {
        adapter.provider_id() == "coinbase"
            && adapter.pair("BTC") == "BTC-USD"
            && adapter.source_suffix() == "coinbase_spot_proxy"
    }));
    assert!(spot.iter().any(|adapter| {
        adapter.provider_id() == "bybit"
            && adapter.pair("BTC") == "BTCUSDT"
            && adapter.source_suffix() == "bybit_spot_proxy"
    }));
    assert!(spot.iter().any(|adapter| {
        adapter.provider_id() == "okx"
            && adapter.pair("BTC") == "BTC-USDT"
            && adapter.source_suffix() == "okx_spot_proxy"
    }));
    assert_eq!(
        ADAPTERS
            .iter()
            .filter(|adapter| adapter.supports("crypto_perpetual"))
            .count(),
        1
    );
    assert!(!ADAPTERS
        .iter()
        .any(|adapter| adapter.supports("tokenized_stock")));
    assert!(!ADAPTERS.iter().any(|adapter| adapter.supports("idx_stock")));
    assert_eq!(
        ResearchAdapter {
            provider: ProviderKind::Bybit(MarketKind::Linear)
        }
        .source_suffix(),
        "bybit_linear_proxy"
    );
}

#[test]
fn coinbase_normalizes_rows_and_discards_open_candles() {
    let body = br#"[[7200,10,12,11,11,4],[3600,9,11,10,10,3]]"#;
    let bars = parse_coinbase_candles(body, 3_600, 10, 10_000).unwrap();
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].ts, 3_600);
    assert_eq!(bars[0].open, 10.0);
    assert_eq!(bars[0].high, 11.0);
    assert_eq!(bars[0].low, 9.0);
    assert_eq!(bars[0].close, 10.0);
    assert_eq!(bars[0].volume, 3.0);
}

#[test]
fn bybit_normalizes_reverse_order_and_rejects_provider_errors() {
    let body = br#"{"retCode":0,"result":{"list":[["7200000","11","12","10","11","4","44"],["3600000","10","11","9","10","3","30"]]}}"#;
    let bars = parse_bybit_candles(body, 3_600, 10, 10_000).unwrap();
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].ts, 3_600);
    assert_eq!(bars[0].volume, 3.0);

    let rejected = br#"{"retCode":10001,"retMsg":"invalid symbol","result":{"list":[]}}"#;
    assert!(parse_bybit_candles(rejected, 3_600, 10, 10_000)
        .unwrap_err()
        .contains("failed"));
}

#[test]
fn bybit_uses_documented_mainnet_hosts_in_regional_fallback_order() {
    assert_eq!(
        BYBIT_MAINNET_MARKET_API_BASE_URLS,
        [
            "https://api.bybit.id",
            "https://api.bytick.com",
            "https://api.bybit.com"
        ]
    );
    assert_eq!(
        bybit_kline_url(
            BYBIT_MAINNET_MARKET_API_BASE_URLS[0],
            "linear",
            "BTCUSDT",
            "15",
            200,
        ),
        "https://api.bybit.id/v5/market/kline?category=linear&symbol=BTCUSDT&interval=15&limit=200"
    );
}

#[test]
fn okx_keeps_only_confirmed_completed_candles() {
    let body = br#"{"code":"0","data":[["7200000","11","12","10","11","4","44","44","0"],["3600000","10","11","9","10","3","30","30","1"]]}"#;
    let bars = parse_okx_candles(body, 3_600, 10, 10_000).unwrap();
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].ts, 3_600);
    assert_eq!(bars[0].volume, 3.0);

    let rejected = br#"{"code":"51001","msg":"instrument does not exist","data":[]}"#;
    assert!(parse_okx_candles(rejected, 3_600, 10, 10_000)
        .unwrap_err()
        .contains("failed"));
}

#[test]
fn provider_parsers_reject_malformed_and_oversized_rows() {
    assert!(parse_coinbase_candles(br#"{"message":"bad symbol"}"#, 3_600, 10, 10_000).is_err());
    assert!(parse_bybit_candles(br#"{"retCode":0,"result":{}}"#, 3_600, 10, 10_000).is_err());
    assert!(parse_okx_candles(br#"{"code":"0","data":[["x"]]}"#, 3_600, 10, 10_000).is_err());

    let rows = vec![json!([]); MAX_COINBASE_ROWS + 1];
    let body = serde_json::to_vec(&rows).unwrap();
    assert!(
        parse_coinbase_candles(&body, 3_600, MAX_COINBASE_ROWS, 10_000)
            .unwrap_err()
            .contains("out of bounds")
    );

    let rows = vec![json!([]); MAX_BYBIT_ROWS + 1];
    let body = serde_json::to_vec(&json!({"retCode":0,"result":{"list":rows}})).unwrap();
    assert!(parse_bybit_candles(&body, 3_600, MAX_BYBIT_ROWS, 10_000)
        .unwrap_err()
        .contains("out of bounds"));

    let rows = vec![json!([]); MAX_OKX_ROWS + 1];
    let body = serde_json::to_vec(&json!({"code":"0","data":rows})).unwrap();
    assert!(parse_okx_candles(&body, 3_600, MAX_OKX_ROWS, 10_000)
        .unwrap_err()
        .contains("out of bounds"));
}

#[test]
fn persisted_provider_bars_keep_app_identity_and_proxy_provenance() {
    let path = crate::main_test_support::temporary_database_path("provider-provenance");
    let connection = super::super::super::open_db_minimal(&path).unwrap();
    let bars = [Bar {
        ts: 1_700_000_000,
        open: 100.0,
        high: 102.0,
        low: 99.0,
        close: 101.0,
        volume: 10.0,
    }];
    persist(
        &connection,
        "BTC-USD",
        "crypto_spot",
        "tokocrypto",
        "coinbase",
        "tokocrypto_spot_coinbase_spot_proxy",
        "BTC-USD",
        "1h",
        "coinbase_usd_reference",
        &bars,
    )
    .unwrap();

    let metadata: (String, String, String, String) = connection
        .query_row(
            "SELECT exchange_id, data_provider, source, price_basis
               FROM series WHERE symbol='BTC-USD' AND interval='1h'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(metadata.0, "tokocrypto");
    assert_eq!(metadata.1, "coinbase");
    assert_eq!(metadata.2, "tokocrypto_spot_coinbase_spot_proxy");
    assert_eq!(metadata.3, "coinbase_usd_reference");
    assert!(super::super::super::is_research_proxy_source(&metadata.2));
    assert!(!super::super::super::is_native_candle_source_for_exchange(
        &metadata.2,
        Some("tokocrypto")
    ));
    assert_eq!(
        super::super::super::candle_source_label(&metadata.2),
        "Coinbase Spot research proxy (USD quote)"
    );

    let provider_attempt: String = connection
        .query_row(
            "SELECT data_provider FROM ingestion_runs
              WHERE symbol='BTC-USD' AND source='tokocrypto_spot_coinbase_spot_proxy'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(provider_attempt, "coinbase");
    drop(connection);
    crate::main_test_support::remove_temporary_database(&path);
}
