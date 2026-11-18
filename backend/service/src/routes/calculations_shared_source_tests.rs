use super::calculations::load_calculation_chart_history;
use crate::{
    load_series_candles_from_source, load_series_candles_for_product, now_secs, open_db_minimal,
    selected_shared_chart_reference_source, NANOS_PER_SECOND,
};
use rusqlite::{params, Connection};

const SYMBOL: &str = "SAME-USD";
const SHARED_SPOT_SOURCE: &str = "shared_crypto_spot_binance_spot_proxy";
const NATIVE_FUTURES_SOURCE: &str = "pintu_futures_native";
const CHART_REFERENCE_NOTICE: &str =
    "Chart uses shared Spot reference candles for Spot and Futures; signal scores may use a different source.";

fn insert_history(
    connection: &Connection,
    source: &str,
    exchange: &str,
    product: &str,
    provider: &str,
    price_basis: &str,
    instrument_kind: &str,
    close: f64,
    timestamp: i64,
) {
    connection
        .execute(
            "INSERT INTO series(
                 symbol,interval,source,exchange_id,product_id,instrument_id,
                 data_provider,timezone,price_basis,instrument_kind,last_success
             ) VALUES(?1,'15m',?2,?3,?4,NULL,?5,'UTC',?6,?7,
                      strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC')",
            params![
                SYMBOL,
                source,
                exchange,
                product,
                provider,
                price_basis,
                instrument_kind
            ],
        )
        .expect("series should persist");
    connection
        .execute(
            "INSERT INTO bars(
                 symbol,interval,source,ts,open,high,low,close,volume,ingested_at
             ) VALUES(?1,'15m',?2,?3,?4,?5,?6,?7,100,'test')",
            params![
                SYMBOL,
                source,
                timestamp * NANOS_PER_SECOND,
                close - 1.0,
                close + 1.0,
                close - 2.0,
                close
            ],
        )
        .expect("candle should persist");
}

#[test]
fn indicators_use_the_same_shared_spot_source_as_the_chart_when_native_bars_overlap() {
    let connection = open_db_minimal(":memory:").expect("storage should initialize");
    let timestamp = (now_secs() as i64).div_euclid(900) * 900 - 900;
    insert_history(
        &connection,
        SHARED_SPOT_SOURCE,
        "research_reference",
        "",
        "binance",
        "binance_usdt_reference",
        "crypto_spot",
        100.0,
        timestamp,
    );
    insert_history(
        &connection,
        NATIVE_FUTURES_SOURCE,
        "pintu",
        "pintu_pro_futures",
        "pintu",
        "venue_native",
        "crypto_perpetual",
        200.0,
        timestamp,
    );

    let chart_source = selected_shared_chart_reference_source(
        &connection,
        SYMBOL,
        "pintu",
        "pintu_pro_futures",
        "15m",
    )
    .expect("chart source selection should succeed");
    assert_eq!(chart_source.as_deref(), Some(SHARED_SPOT_SOURCE));
    assert_eq!(
        selected_shared_chart_reference_source(
            &connection,
            SYMBOL,
            "pintu_futures",
            "pintu_pro_futures",
            "15m",
        )
        .expect("Pintu Futures alias should resolve the chart source")
        .as_deref(),
        Some(SHARED_SPOT_SOURCE)
    );

    let displayed = load_series_candles_from_source(
        &connection,
        SYMBOL,
        "15m",
        10,
        chart_source.as_deref().unwrap(),
    )
    .expect("shared chart candles should load");
    let native = load_series_candles_for_product(
        &connection,
        SYMBOL,
        "15m",
        10,
        "pintu",
        "pintu_pro_futures",
    )
    .expect("native product candles should load");
    let (indicator_candles, metadata) = load_calculation_chart_history(
        &connection,
        SYMBOL,
        "15m",
        10,
        "pintu",
        "pintu_pro_futures",
        true,
    )
    .expect("indicator chart history should load");
    let (signal_candles, signal_metadata) = load_calculation_chart_history(
        &connection,
        SYMBOL,
        "15m",
        10,
        "pintu",
        "pintu_pro_futures",
        false,
    )
    .expect("signal chart history should keep its product source");

    assert_eq!(displayed.len(), 1);
    assert_eq!(native.len(), 1);
    assert_eq!(indicator_candles.len(), 1);
    assert_eq!(signal_candles.len(), 1);
    assert_eq!(displayed[0].time, timestamp * NANOS_PER_SECOND);
    assert_eq!(indicator_candles[0].time, displayed[0].time);
    assert_eq!(displayed[0].close, 100.0);
    assert_eq!(native[0].close, 200.0);
    assert_eq!(indicator_candles[0].close, displayed[0].close);
    assert_eq!(metadata["source"], SHARED_SPOT_SOURCE);
    assert_eq!(metadata["chart_reference_notice"], CHART_REFERENCE_NOTICE);
    assert_eq!(signal_candles[0].close, native[0].close);
    assert_eq!(signal_metadata["source"], NATIVE_FUTURES_SOURCE);
}
