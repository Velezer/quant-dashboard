use super::stored_source_coverage_days;
use rusqlite::{params, Connection};

#[test]
fn stored_coverage_uses_full_bounds_for_the_exact_source_interval() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE bars(symbol TEXT, interval TEXT, source TEXT, ts INTEGER);
             INSERT INTO bars VALUES
                ('BTC-USD','1h','shared_perpetual',1000000000),
                ('BTC-USD','1h','shared_perpetual',39571201000000000),
                ('BTC-USD','4h','shared_perpetual',1),
                ('BTC-USD','1h','other_source',1);",
        )
        .unwrap();

    let coverage =
        stored_source_coverage_days(&connection, "BTC-USD", "1h", "shared_perpetual").unwrap();

    assert_eq!(coverage, 459);
}

#[test]
fn stored_coverage_supports_histories_longer_than_the_chart_window() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("CREATE TABLE bars(symbol TEXT, interval TEXT, source TEXT, ts INTEGER);")
        .unwrap();
    let day_ns = crate::NANOS_PER_DAY;
    connection
        .execute(
            "INSERT INTO bars VALUES('BTC-USD','1d','shared_perpetual',?1),
                ('BTC-USD','1d','shared_perpetual',?2)",
            params![1_000_i64, 2_000_i64 + day_ns * 1_998],
        )
        .unwrap();

    let coverage =
        stored_source_coverage_days(&connection, "BTC-USD", "1d", "shared_perpetual").unwrap();

    assert_eq!(coverage, 1_999);
}
