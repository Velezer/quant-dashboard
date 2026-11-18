use super::*;

const SYMBOL: &str = "DIRECT-4H-USD";
const SOURCE: &str = "test_native";
const HOUR_NS: i64 = 3_600 * NANOS_PER_SECOND;

fn insert_series(connection: &Connection, timeframe: &str) {
    connection
        .execute(
            "INSERT INTO series(
                 symbol,interval,source,timezone,price_basis,instrument_kind
             ) VALUES(?1,?2,?3,'UTC','last_trade','spot')",
            params![SYMBOL, timeframe, SOURCE],
        )
        .expect("series should persist");
}

fn insert_bar(connection: &Connection, timeframe: &str, time: i64, close: f64) {
    connection
        .execute(
            "INSERT INTO bars(
                 symbol,interval,source,ts,open,high,low,close,volume,ingested_at
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?5,100,'test')",
            params![SYMBOL, timeframe, SOURCE, time, close, close + 1.0, close - 1.0],
        )
        .expect("bar should persist");
}

#[test]
fn older_hourly_rollup_does_not_replace_a_newer_direct_four_hour_series() {
    let connection = open_db_minimal(":memory:").expect("storage should initialize");
    insert_series(&connection, "1h");
    insert_series(&connection, "4h");
    for index in 0..4_i64 {
        insert_bar(&connection, "1h", index * HOUR_NS, 100.0 + index as f64);
    }
    let direct_time = 4 * HOUR_NS;
    insert_bar(&connection, "4h", direct_time, 250.0);

    let storage_timeframe =
        preferred_storage_timeframe_from_source(&connection, SYMBOL, "4h", SOURCE)
            .expect("history selection should succeed");
    let candles = load_series_candles_from_source(&connection, SYMBOL, "4h", 10, SOURCE)
        .expect("chart candles should load");
    let metadata =
        load_series_metadata_from_source(&connection, SYMBOL, "4h", false, SOURCE)
            .expect("chart metadata should load");

    assert_eq!(storage_timeframe, "4h");
    assert_eq!(candles.len(), 1);
    assert_eq!(candles[0].time, direct_time);
    assert_eq!(candles[0].close, 250.0);
    assert_eq!(metadata["storage_timeframe"], "4h");
    assert_eq!(metadata["last_ts"], direct_time);
}
