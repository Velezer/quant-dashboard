use super::calculations::load_calculation_chart_history;
use crate::{now_secs, open_db_minimal, NANOS_PER_SECOND};
use rusqlite::{params, Connection};

const SYMBOL: &str = "SAME-USD";

fn insert_product_history(
    connection: &Connection,
    product_id: &str,
    source: &str,
    first_close: f64,
) {
    connection
        .execute(
            "INSERT INTO series(
                 symbol,interval,source,exchange_id,product_id,data_provider,timezone,
                 price_basis,instrument_kind,last_success
             ) VALUES(?1,'15m',?2,'pluang',?3,'binance','UTC',
                      'binance_usdt_reference','spot',
                      strftime('%Y-%m-%d %H:%M:%S','now') || ' UTC')",
            params![SYMBOL, source, product_id],
        )
        .expect("product series should persist");
    let latest = now_secs() as i64 - 900;
    let first = (latest - 39 * 900) * NANOS_PER_SECOND;
    for index in 0..40_i64 {
        let close = first_close + index as f64;
        connection
            .execute(
                "INSERT INTO bars(
                     symbol,interval,source,ts,open,high,low,close,volume,ingested_at
                 ) VALUES(?1,'15m',?2,?3,?4,?5,?6,?4,100,'test')",
                params![
                    SYMBOL,
                    source,
                    first + index * 900 * NANOS_PER_SECOND,
                    close,
                    close + 1.0,
                    close - 1.0
                ],
            )
            .expect("product candle should persist");
    }
}

#[test]
fn calculation_chart_history_scopes_candles_and_metadata_to_the_product() {
    let connection = open_db_minimal(":memory:").expect("storage should initialize");
    insert_product_history(
        &connection,
        "pluang_spot",
        "pluang_spot_binance_spot_proxy",
        100.0,
    );
    insert_product_history(
        &connection,
        "pluang_perpetual",
        "pluang_perpetual_binance_usdm_futures_proxy",
        200.0,
    );

    let (spot, spot_metadata) = load_calculation_chart_history(
        &connection,
        SYMBOL,
        "15m",
        100,
        "pluang",
        "pluang_spot",
        false,
    )
    .expect("Spot chart history should load");
    let (perpetual, perpetual_metadata) = load_calculation_chart_history(
        &connection,
        SYMBOL,
        "15m",
        100,
        "pluang",
        "pluang_perpetual",
        false,
    )
    .expect("Perpetual chart history should load");

    assert_eq!(spot.len(), 40);
    assert_eq!(perpetual.len(), 40);
    assert_eq!(spot[0].close, 100.0);
    assert_eq!(perpetual[0].close, 200.0);
    assert_eq!(spot_metadata["source"], "pluang_spot_binance_spot_proxy");
    assert_eq!(spot_metadata["product_id"], "pluang_spot");
    assert_eq!(
        perpetual_metadata["source"],
        "pluang_perpetual_binance_usdm_futures_proxy"
    );
    assert_eq!(perpetual_metadata["product_id"], "pluang_perpetual");
}
