use super::{signal_candle_is_expired, timeframe_duration_ns};
use rusqlite::Connection;

#[test]
fn screener_schema_creates_storage_symbol_in_dedicated_database() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("ATTACH DATABASE ':memory:' AS app; ATTACH DATABASE ':memory:' AS signals;")
        .unwrap();

    super::super::initialize_screener_schema(&connection)
        .expect("signal snapshot schema should initialize");

    let has_storage_symbol = {
        let mut statement = connection
            .prepare("PRAGMA signals.table_info('signal_screener_rows')")
            .unwrap();
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap();
        columns
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .iter()
            .any(|name| name == "storage_symbol")
    };
    assert!(has_storage_symbol);
}

#[test]
fn fifteen_minute_signal_expires_two_hours_after_its_candle_closes() {
    let candle_open = 1_700_000_000_i64;
    let candle_close = candle_open + 15 * 60;
    let expiry = candle_close + 2 * 60 * 60;

    assert!(!signal_candle_is_expired(candle_open, "15m", expiry - 1));
    assert!(signal_candle_is_expired(candle_open, "15m", expiry));
}

#[test]
fn every_enabled_timeframe_expires_two_hours_after_its_candle_closes() {
    let candle_open = 1_700_000_000_i64;
    for definition in crate::DSS_BRESSERT_MTF_TIMEFRAMES {
        let candle_close =
            candle_open + timeframe_duration_ns(definition.timeframe).unwrap() / 1_000_000_000;
        let expiry = candle_close + 2 * 60 * 60;

        assert!(
            !signal_candle_is_expired(candle_open, definition.timeframe, expiry - 1),
            "{} should be current before its two-hour expiry",
            definition.timeframe
        );
        assert!(
            signal_candle_is_expired(candle_open, definition.timeframe, expiry),
            "{} should expire at its two-hour deadline",
            definition.timeframe
        );
    }
}
