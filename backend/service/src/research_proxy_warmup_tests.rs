use super::{
    sync_stockbit_4h_history_if_required, warmup_cooldown_active, STOCKBIT_4H_HISTORY_SHORT_PREFIX,
    STOCKBIT_4H_SOURCE, STOCKBIT_4H_WARMUP_ERROR_PREFIX, STOCKBIT_4H_WARMUP_INTERVAL,
};
use rusqlite::{params, Connection};

fn ingestion_fixture() -> Connection {
    let connection = Connection::open_in_memory().expect("in-memory fixture should open");
    connection
        .execute_batch(
            "CREATE TABLE ingestion_runs(
                id INTEGER PRIMARY KEY,symbol TEXT,interval TEXT,source TEXT,
                attempted_at TEXT,status TEXT,rows_received INTEGER,error TEXT
            );",
        )
        .expect("ingestion diagnostics should initialize");
    connection
}

fn insert_run(
    connection: &Connection,
    interval: &str,
    status: &str,
    error: &str,
    age_seconds: u64,
) {
    let attempted_at = crate::format_timestamp(crate::now_secs().saturating_sub(age_seconds));
    connection
        .execute(
            "INSERT INTO ingestion_runs(symbol,interval,source,attempted_at,status,error)
             VALUES('BBCA.JK',?1,?2,?3,?4,?5)",
            params![interval, STOCKBIT_4H_SOURCE, attempted_at, status, error],
        )
        .expect("warm-up diagnostic should persist");
}

#[test]
fn non_stockbit_and_non_hourly_syncs_do_not_request_a_warmup() {
    let connection = Connection::open_in_memory().expect("in-memory fixture should open");
    assert!(sync_stockbit_4h_history_if_required(
        &connection,
        "BTC-USD",
        "crypto_spot",
        "pintu",
        "pintu_retail_spot",
        "BTC-USD",
        "1h",
    )
    .is_none());
    assert!(sync_stockbit_4h_history_if_required(
        &connection,
        "BBCA.JK",
        "idx_stock",
        "stockbit",
        "stockbit_regular",
        "BBCA.JK",
        "15m",
    )
    .is_none());
}

#[test]
fn hourly_rollup_warmup_uses_the_existing_4h_aggregation_bound() {
    let target_bars = i64::try_from(quant_core::stockbit_nwe_4h_minimum_history_bars())
        .expect("strategy bar count should fit SQLite");
    assert_eq!(target_bars, 501);
    assert_eq!(
        crate::required_aggregation_source_candles("4h", "1h", target_bars),
        Some(2_124)
    );
}

#[test]
fn insufficient_history_cooldown_expires_after_thirty_days() {
    let connection = ingestion_fixture();
    insert_run(
        &connection,
        STOCKBIT_4H_WARMUP_INTERVAL,
        "insufficient_history",
        STOCKBIT_4H_HISTORY_SHORT_PREFIX,
        29 * 24 * 60 * 60,
    );
    assert!(warmup_cooldown_active(&connection, "BBCA.JK"));

    connection
        .execute(
            "UPDATE ingestion_runs SET attempted_at=?1 WHERE interval=?2",
            params![
                crate::format_timestamp(crate::now_secs().saturating_sub(31 * 24 * 60 * 60)),
                STOCKBIT_4H_WARMUP_INTERVAL
            ],
        )
        .expect("cooldown fixture should age");
    assert!(!warmup_cooldown_active(&connection, "BBCA.JK"));
}

#[test]
fn failed_warmup_cooldown_expires_after_one_hour_even_with_an_old_short_history_run() {
    let connection = ingestion_fixture();
    insert_run(
        &connection,
        STOCKBIT_4H_WARMUP_INTERVAL,
        "insufficient_history",
        STOCKBIT_4H_HISTORY_SHORT_PREFIX,
        31 * 24 * 60 * 60,
    );
    insert_run(
        &connection,
        "1h",
        "error",
        STOCKBIT_4H_WARMUP_ERROR_PREFIX,
        30 * 60,
    );
    assert!(warmup_cooldown_active(&connection, "BBCA.JK"));

    connection
        .execute(
            "UPDATE ingestion_runs SET attempted_at=?1 WHERE interval='1h'",
            [crate::format_timestamp(
                crate::now_secs().saturating_sub(2 * 60 * 60),
            )],
        )
        .expect("failed attempt fixture should age");
    assert!(!warmup_cooldown_active(&connection, "BBCA.JK"));
}

#[test]
fn failed_warmup_retries_after_a_later_successful_hourly_fetch() {
    let connection = ingestion_fixture();
    insert_run(
        &connection,
        "1h",
        "error",
        STOCKBIT_4H_WARMUP_ERROR_PREFIX,
        45 * 60,
    );
    assert!(warmup_cooldown_active(&connection, "BBCA.JK"));

    insert_run(&connection, "1h", "success", "", 5 * 60);
    assert!(!warmup_cooldown_active(&connection, "BBCA.JK"));
}

#[test]
fn later_hourly_failure_keeps_the_warmup_cooldown_active() {
    let connection = ingestion_fixture();
    insert_run(
        &connection,
        "1h",
        "error",
        STOCKBIT_4H_WARMUP_ERROR_PREFIX,
        45 * 60,
    );
    insert_run(&connection, "1h", "success", "", 30 * 60);
    insert_run(
        &connection,
        "1h",
        "error",
        "provider is unreachable",
        5 * 60,
    );
    assert!(warmup_cooldown_active(&connection, "BBCA.JK"));
}
